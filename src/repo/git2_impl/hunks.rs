// Copyright 2026 Thomas Johannesson
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Pure tree/hunk manipulation helpers.
//!
//! These free functions operate on `git2` primitives and have no dependency
//! on `Git2Repo` or the `GitRepo` trait, so they sit cleanly in their own
//! module and can be unit-tested in isolation.

use anyhow::{Context, Result};
use std::collections::HashMap;

/// Build the commit message for the n-th commit in a split sequence.
///
/// The first line of the original message is kept and suffixed with "(n/total)".
/// If the original message has a body (text after the first line), it is
/// appended unchanged so no information is lost.
pub(super) fn split_message(original: &str, n: usize, total: usize) -> String {
    let mut lines = original.splitn(2, '\n');
    let first = lines.next().unwrap_or("split").trim_end();
    let rest = lines.next().unwrap_or("");
    if rest.trim().is_empty() {
        format!("{} ({}/{})", first, n, total)
    } else {
        format!("{} ({}/{})\n{}", first, n, total, rest)
    }
}

/// Append a parenthesised `suffix` to the summary line of `original`, keeping
/// the body intact.  Used by the "split out file" operation to mark the
/// peeled-out commit with the file name.
pub(super) fn summary_suffix_message(original: &str, suffix: &str) -> String {
    let mut lines = original.splitn(2, '\n');
    let first = lines.next().unwrap_or("split").trim_end();
    let rest = lines.next().unwrap_or("");
    if rest.trim().is_empty() {
        format!("{} ({})", first, suffix)
    } else {
        format!("{} ({})\n{}", first, suffix, rest)
    }
}

/// Apply a gitlink (submodule pointer) delta to `base_tree` and return the
/// updated tree OID.
///
/// `apply_to_tree` cannot handle gitlink entries because libgit2 treats them
/// as blobs, causing a crash.  `TreeUpdateBuilder` supports the `0o160000`
/// commit-mode entry directly and is used here instead.
pub(super) fn apply_gitlink_delta_to_tree(
    repo: &git2::Repository,
    base_tree: &git2::Tree<'_>,
    delta: &git2::DiffDelta<'_>,
) -> Result<git2::Oid> {
    let path = delta
        .new_file()
        .path()
        .or_else(|| delta.old_file().path())
        .context("gitlink delta has no path")?
        .to_owned();
    let path_str = path.to_str().context("submodule path is not valid UTF-8")?;

    let mut builder = git2::build::TreeUpdateBuilder::new();
    if delta.status() == git2::Delta::Deleted {
        builder.remove(path_str);
    } else {
        builder.upsert(path_str, delta.new_file().id(), git2::FileMode::Commit);
    }
    builder.create_updated(repo, base_tree).map_err(Into::into)
}

/// Apply the first hunk of the first non-empty delta in `diff` to `base_tree`
/// and return the resulting tree OID.
///
/// The diff must have been computed from `base_tree`, so the hunk's old-side
/// content matches exactly.  This avoids `apply_to_tree` with `hunk_callback`
/// filtering, which fails because libgit2 validates rejected hunks against the
/// already-modified output buffer (whose line positions have shifted).
pub(super) fn apply_single_hunk_to_tree(
    repo: &git2::Repository,
    base_tree: &git2::Tree,
    diff: &git2::Diff,
) -> Result<git2::Oid> {
    for delta_idx in 0..diff.deltas().len() {
        let mut patch = match git2::Patch::from_diff(diff, delta_idx)? {
            Some(p) => p,
            None => continue,
        };
        if patch.num_hunks() == 0 {
            continue;
        }
        let delta = diff.get_delta(delta_idx).context("delta index in range")?;
        let file_path = delta
            .new_file()
            .path()
            .or_else(|| delta.old_file().path())
            .context("delta has no file path")?
            .to_owned();

        let (old_content, mode) = match delta.status() {
            git2::Delta::Added => {
                let m: u32 = delta.new_file().mode().into();
                (Vec::new(), m)
            }
            _ => {
                let entry = base_tree
                    .get_path(&file_path)
                    .with_context(|| format!("'{}' not in base tree", file_path.display()))?;
                let blob = repo.find_blob(entry.id())?;
                (blob.content().to_owned(), entry.filemode() as u32)
            }
        };

        let new_content = apply_hunk_to_content(&old_content, &mut patch, 0)
            .with_context(|| format!("applying hunk to '{}'", file_path.display()))?;

        let new_blob_oid = repo.blob(&new_content)?;

        // Load base_tree into an in-memory index, update the one file, write tree.
        let mut idx = git2::Index::new()?;
        idx.read_tree(base_tree)?;

        let path_bytes = file_path
            .to_str()
            .context("file path is not valid UTF-8")?
            .as_bytes()
            .to_vec();
        idx.add(&git2::IndexEntry {
            ctime: git2::IndexTime::new(0, 0),
            mtime: git2::IndexTime::new(0, 0),
            dev: 0,
            ino: 0,
            mode,
            uid: 0,
            gid: 0,
            file_size: new_content.len() as u32,
            id: new_blob_oid,
            flags: 0,
            flags_extended: 0,
            path: path_bytes,
        })?;

        return idx.write_tree_to(repo).map_err(Into::into);
    }
    Ok(base_tree.id())
}
/// Apply a selected subset of hunks from `full_diff` to `parent_tree`,
/// returning the new tree OID.
///
/// `selected_hunks` maps each delta index to the list of hunk indices
/// (within that delta) to apply.  Files with no selected hunks keep their
/// original content from `parent_tree`.
pub(super) fn apply_selected_hunks_to_tree(
    repo: &git2::Repository,
    parent_tree: &git2::Tree,
    full_diff: &git2::Diff,
    selected_hunks: &HashMap<usize, Vec<usize>>,
) -> Result<git2::Oid> {
    let mut idx = git2::Index::new()?;
    idx.read_tree(parent_tree)?;

    for (&delta_idx, hunk_indices) in selected_hunks {
        if hunk_indices.is_empty() {
            continue;
        }

        let mut patch = match git2::Patch::from_diff(full_diff, delta_idx)? {
            Some(p) => p,
            None => continue,
        };

        let delta = full_diff
            .get_delta(delta_idx)
            .context("delta index in range")?;
        let file_path = delta
            .new_file()
            .path()
            .or_else(|| delta.old_file().path())
            .context("delta has no file path")?
            .to_owned();

        let (old_content, mode) = match delta.status() {
            git2::Delta::Added => {
                let m: u32 = delta.new_file().mode().into();
                (Vec::new(), m)
            }
            _ => {
                let entry = parent_tree
                    .get_path(&file_path)
                    .with_context(|| format!("'{}' not in parent tree", file_path.display()))?;
                let blob = repo.find_blob(entry.id())?;
                (blob.content().to_owned(), entry.filemode() as u32)
            }
        };

        let new_content =
            apply_multiple_hunks_to_content(&old_content, &mut patch, hunk_indices)
                .with_context(|| format!("applying selected hunks to '{}'", file_path.display()))?;

        let path_bytes = file_path
            .to_str()
            .context("file path is not valid UTF-8")?
            .as_bytes()
            .to_vec();

        if delta.status() == git2::Delta::Deleted && new_content.is_empty() {
            // All lines removed → delete the file from the intermediate tree.
            idx.remove(&file_path, 0)?;
        } else {
            let new_blob_oid = repo.blob(&new_content)?;
            idx.add(&git2::IndexEntry {
                ctime: git2::IndexTime::new(0, 0),
                mtime: git2::IndexTime::new(0, 0),
                dev: 0,
                ino: 0,
                mode,
                uid: 0,
                gid: 0,
                file_size: new_content.len() as u32,
                id: new_blob_oid,
                flags: 0,
                flags_extended: 0,
                path: path_bytes,
            })?;
        }
    }

    idx.write_tree_to(repo).map_err(Into::into)
}

/// Apply hunk `hunk_idx` from `patch` to `content`, returning the new bytes.
///
/// Replacement splices in context + added lines, dropping deleted lines.
fn apply_hunk_to_content(
    content: &[u8],
    patch: &mut git2::Patch,
    hunk_idx: usize,
) -> Result<Vec<u8>> {
    let (hunk_header, _) = patch.hunk(hunk_idx)?;
    let old_start = hunk_header.old_start() as usize; // 1-based
    let old_count = hunk_header.old_lines() as usize;

    let lines = split_lines_keep_eol(content);

    let num_lines = patch.num_lines_in_hunk(hunk_idx)?;
    let mut replacement: Vec<Vec<u8>> = Vec::new();
    for line_idx in 0..num_lines {
        let line = patch.line_in_hunk(hunk_idx, line_idx)?;
        match line.origin() {
            ' ' | '+' => replacement.push(line.content().to_owned()),
            _ => {}
        }
    }

    // old_start is 1-based.  For a substitution or deletion (old_count > 0) it
    // is the first line to remove, so the 0-based index is old_start-1.
    // For a pure insertion (old_count == 0) git convention says "insert after
    // line old_start", so the splice point is old_start (0-based).
    let start = if old_count == 0 {
        old_start.min(lines.len())
    } else {
        old_start.saturating_sub(1).min(lines.len())
    };
    let end = (start + old_count).min(lines.len());

    let mut result: Vec<u8> = Vec::new();
    for l in &lines[..start] {
        result.extend_from_slice(l);
    }
    for r in &replacement {
        result.extend_from_slice(r);
    }
    for l in &lines[end..] {
        result.extend_from_slice(l);
    }
    Ok(result)
}

/// Split raw bytes into lines keeping each `\n` terminator attached.
fn split_lines_keep_eol(data: &[u8]) -> Vec<&[u8]> {
    let mut lines: Vec<&[u8]> = Vec::new();
    let mut start = 0;
    for (i, &b) in data.iter().enumerate() {
        if b == b'\n' {
            lines.push(&data[start..=i]);
            start = i + 1;
        }
    }
    if start < data.len() {
        lines.push(&data[start..]);
    }
    lines
}

/// Apply the hunks at `hunk_indices` from `patch` to `content` in a single
/// sweep, using each hunk's original `old_start` position.  Hunks NOT listed
/// in `hunk_indices` are preserved unchanged.
///
/// `hunk_indices` need not be sorted on entry; the function sorts them by
/// `old_start` before processing.
fn apply_multiple_hunks_to_content(
    content: &[u8],
    patch: &mut git2::Patch,
    hunk_indices: &[usize],
) -> Result<Vec<u8>> {
    // Sort by old_start so we can sweep top-to-bottom through the original.
    let mut sorted: Vec<(usize, u32)> = hunk_indices
        .iter()
        .map(|&i| {
            patch
                .hunk(i)
                .map(|(h, _)| (i, h.old_start()))
                .map_err(anyhow::Error::from)
        })
        .collect::<Result<_>>()?;
    sorted.sort_by_key(|&(_, s)| s);

    let lines = split_lines_keep_eol(content);
    let mut result: Vec<u8> = Vec::new();
    let mut src_pos: usize = 0;

    for (hunk_idx, _) in &sorted {
        let (hunk_header, _) = patch.hunk(*hunk_idx)?;
        let old_start = hunk_header.old_start() as usize; // 1-based
        let old_count = hunk_header.old_lines() as usize;

        let splice_start = if old_count == 0 {
            old_start.min(lines.len())
        } else {
            old_start.saturating_sub(1).min(lines.len())
        };

        for line in &lines[src_pos..splice_start] {
            result.extend_from_slice(line);
        }
        src_pos = splice_start + old_count;

        let num_lines = patch.num_lines_in_hunk(*hunk_idx)?;
        for line_idx in 0..num_lines {
            let line = patch.line_in_hunk(*hunk_idx, line_idx)?;
            if matches!(line.origin(), ' ' | '+') {
                result.extend_from_slice(line.content());
            }
        }
    }

    for line in &lines[src_pos..] {
        result.extend_from_slice(line);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::{split_message, summary_suffix_message};

    #[test]
    fn summary_suffix_message_summary_only() {
        assert_eq!(
            summary_suffix_message("my fix", "src/a.rs"),
            "my fix (src/a.rs)"
        );
    }

    #[test]
    fn summary_suffix_message_preserves_body() {
        let original = "my fix\n\nBody line 1.\nBody line 2.";
        let result = summary_suffix_message(original, "src/a.rs");
        assert_eq!(result, "my fix (src/a.rs)\n\nBody line 1.\nBody line 2.");
    }

    #[test]
    fn summary_suffix_message_trailing_newline_on_summary() {
        assert_eq!(
            summary_suffix_message("my fix\n", "src/a.rs"),
            "my fix (src/a.rs)"
        );
    }

    #[test]
    fn split_message_summary_only() {
        assert_eq!(split_message("my fix", 1, 3), "my fix (1/3)");
    }

    #[test]
    fn split_message_preserves_body() {
        let original = "my fix\n\nBody line 1.\nBody line 2.";
        let result = split_message(original, 2, 3);
        assert_eq!(result, "my fix (2/3)\n\nBody line 1.\nBody line 2.");
    }

    #[test]
    fn split_message_body_whitespace_only_treated_as_no_body() {
        let original = "my fix\n\n  \n";
        let result = split_message(original, 1, 2);
        assert_eq!(result, "my fix (1/2)");
    }

    #[test]
    fn split_message_trailing_newline_on_summary() {
        // git commit messages often end with a newline
        let result = split_message("my fix\n", 1, 1);
        assert_eq!(result, "my fix (1/1)");
    }
}
