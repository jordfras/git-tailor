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

//! Squash a source commit into an earlier target commit, optionally as a
//! fixup. Supports the conflict-resolution flow via `try_combine` /
//! `finalize` so the user can hand-edit the merged tree before finishing.

use anyhow::Result;

use super::super::{ConflictState, RebaseOutcome, SquashContext};
use super::Git2Repo;
use super::cherry_pick::{ChainCtx, CherryPickResult};
use super::conflict;
use crate::Oid;
use crate::app::SquashMode;

pub(super) fn squash_commits(
    repo: &Git2Repo,
    source_oid: &Oid,
    target_oid: &Oid,
    message: &str,
    head_oid: &Oid,
) -> Result<RebaseOutcome> {
    repo.check_no_dirty_state()?;

    let inputs = parse_squash_inputs(repo, source_oid, target_oid, head_oid)?;

    let mut merge_opts = git2::MergeOptions::new();
    merge_opts
        .find_renames(true)
        .rename_threshold(10)
        .target_limit(1000);
    let mut cherry_index = repo.inner.cherrypick_commit(
        &inputs.source_commit,
        &inputs.target_commit,
        0,
        Some(&merge_opts),
    )?;
    if cherry_index.has_conflicts() {
        // If rename detection in the 3-way merge didn't resolve the conflict,
        // try an explicit rename-aware tree merge as a fallback.
        if let Some(tree_oid) =
            rename_aware_squash_tree(&repo.inner, &inputs.source_commit, &inputs.target_commit)?
        {
            let combined_tree = repo.inner.find_tree(tree_oid)?;
            let base_commit = inputs
                .base_oid
                .map(|oid| repo.inner.find_commit(oid))
                .transpose()?;
            let parents: Vec<&git2::Commit<'_>> = base_commit.iter().collect();
            let squash_oid = repo.inner.commit(
                None,
                &inputs.target_commit.author(),
                &inputs.target_commit.committer(),
                message,
                &combined_tree,
                &parents,
            )?;

            let all_descendants =
                repo.collect_descendants(inputs.target_commit.id(), inputs.head_git_oid)?;
            let descendants: Vec<git2::Oid> = all_descendants
                .into_iter()
                .filter(|&oid| oid != inputs.source_commit.id())
                .collect();

            return replay_and_advance(
                repo,
                squash_oid,
                &descendants,
                head_oid.clone(),
                "Squash",
                "git-tailor: squash commits",
            );
        }
        return Ok(RebaseOutcome::Conflict(Box::new(build_conflict_state(
            repo,
            &cherry_index,
            &inputs,
            message,
            SquashMode::Squash,
        )?)));
    }

    let combined_tree_oid = cherry_index.write_tree_to(&repo.inner)?;
    let combined_tree = repo.inner.find_tree(combined_tree_oid)?;

    let base_commit = inputs
        .base_oid
        .map(|oid| repo.inner.find_commit(oid))
        .transpose()?;
    let parents: Vec<&git2::Commit<'_>> = base_commit.iter().collect();
    let squash_oid = repo.inner.commit(
        None,
        &inputs.target_commit.author(),
        &inputs.target_commit.committer(),
        message,
        &combined_tree,
        &parents,
    )?;

    let all_descendants =
        repo.collect_descendants(inputs.target_commit.id(), inputs.head_git_oid)?;
    let descendants: Vec<git2::Oid> = all_descendants
        .into_iter()
        .filter(|&oid| oid != inputs.source_commit.id())
        .collect();

    replay_and_advance(
        repo,
        squash_oid,
        &descendants,
        head_oid.clone(),
        "Squash",
        "git-tailor: squash commits",
    )
}

pub(super) fn squash_try_combine(
    repo: &Git2Repo,
    source_oid: &Oid,
    target_oid: &Oid,
    combined_message: &str,
    squash_mode: SquashMode,
    head_oid: &Oid,
) -> Result<Option<ConflictState>> {
    repo.check_no_dirty_state()?;

    let inputs = parse_squash_inputs(repo, source_oid, target_oid, head_oid)?;

    let mut merge_opts = git2::MergeOptions::new();
    merge_opts
        .find_renames(true)
        .rename_threshold(10)
        .target_limit(1000);
    let cherry_index = repo.inner.cherrypick_commit(
        &inputs.source_commit,
        &inputs.target_commit,
        0,
        Some(&merge_opts),
    )?;
    if !cherry_index.has_conflicts() {
        return Ok(None);
    }

    // If rename detection in the 3-way merge didn't resolve the conflict,
    // try explicit rename-aware tree merge as a fallback.
    if rename_aware_squash_tree(&repo.inner, &inputs.source_commit, &inputs.target_commit)?
        .is_some()
    {
        return Ok(None);
    }

    Ok(Some(build_conflict_state(
        repo,
        &cherry_index,
        &inputs,
        combined_message,
        squash_mode,
    )?))
}

pub(super) fn squash_finalize(
    repo: &Git2Repo,
    ctx: &SquashContext,
    message: &str,
    original_branch_oid: &Oid,
) -> Result<RebaseOutcome> {
    let mut index = repo.inner.index()?;
    index.read(true)?;
    if index.has_conflicts() {
        anyhow::bail!("Cannot finalize squash: index still has unresolved conflicts");
    }

    let target_git_oid = git2::Oid::from(&ctx.target_oid);
    let target_commit = repo.inner.find_commit(target_git_oid)?;

    let combined_tree_oid = index.write_tree()?;
    let combined_tree = repo.inner.find_tree(combined_tree_oid)?;

    let base_commit = ctx
        .base_oid
        .as_ref()
        .map(|oid| repo.inner.find_commit(git2::Oid::from(oid)))
        .transpose()?;
    let parents: Vec<&git2::Commit<'_>> = base_commit.iter().collect();
    let squash_oid = repo.inner.commit(
        None,
        &target_commit.author(),
        &target_commit.committer(),
        message,
        &combined_tree,
        &parents,
    )?;

    let descendants: Vec<git2::Oid> = ctx.descendant_oids.iter().map(git2::Oid::from).collect();

    replay_and_advance(
        repo,
        squash_oid,
        &descendants,
        original_branch_oid.clone(),
        ctx.squash_mode.label(),
        "git-tailor: squash commits (finalize)",
    )
}

/// Resolved inputs to a squash operation: parsed commits and OIDs.
struct SquashInputs<'a> {
    source_oid: &'a Oid,
    target_oid: &'a Oid,
    head_oid: &'a Oid,
    source_commit: git2::Commit<'a>,
    target_commit: git2::Commit<'a>,
    /// Parent of the target commit. `None` when target is root.
    base_oid: Option<git2::Oid>,
    head_git_oid: git2::Oid,
}

fn parse_squash_inputs<'a>(
    repo: &'a Git2Repo,
    source_oid: &'a Oid,
    target_oid: &'a Oid,
    head_oid: &'a Oid,
) -> Result<SquashInputs<'a>> {
    let source_git_oid = git2::Oid::from(source_oid);
    let target_git_oid = git2::Oid::from(target_oid);
    let head_git_oid = git2::Oid::from(head_oid);

    let source_commit = repo.inner.find_commit(source_git_oid)?;
    let target_commit = repo.inner.find_commit(target_git_oid)?;

    if target_commit.parent_count() > 1 {
        anyhow::bail!("Cannot squash into a merge commit");
    }
    let base_oid = if target_commit.parent_count() == 0 {
        None
    } else {
        Some(target_commit.parent_id(0)?)
    };

    Ok(SquashInputs {
        source_oid,
        target_oid,
        head_oid,
        source_commit,
        target_commit,
        base_oid,
        head_git_oid,
    })
}

/// Persist a conflicted cherry-pick to the working tree and build the
/// `ConflictState` describing it for the user-facing resolution flow.
fn build_conflict_state(
    repo: &Git2Repo,
    cherry_index: &git2::Index,
    inputs: &SquashInputs<'_>,
    combined_message: &str,
    squash_mode: SquashMode,
) -> Result<ConflictState> {
    let all_descendants =
        repo.collect_descendants(inputs.target_commit.id(), inputs.head_git_oid)?;
    let descendant_oids: Vec<Oid> = all_descendants
        .into_iter()
        .filter(|&oid| oid != inputs.source_commit.id())
        .map(Oid::from)
        .collect();

    let state = ConflictState {
        operation_label: squash_mode.label().to_string(),
        original_branch_oid: inputs.head_oid.clone(),
        new_tip_oid: Oid::from(inputs.target_commit.id()),
        conflicting_commit_oid: Oid::from(inputs.source_commit.id()),
        remaining_oids: vec![],
        conflicting_files: conflict::collect_conflict_files_from_index(cherry_index),
        still_unresolved: false,
        moved_commit_oid: None,
        is_orphan_root: false,
        squash_context: Some(SquashContext {
            base_oid: inputs.base_oid.map(Oid::from),
            source_oid: inputs.source_oid.clone(),
            target_oid: inputs.target_oid.clone(),
            combined_message: combined_message.to_string(),
            descendant_oids,
            squash_mode,
        }),
    };
    // Journals write-ahead, then mutates the ref/index/workdir.
    conflict::write_conflicts_to_workdir(repo, cherry_index, &inputs.target_commit, &state)?;
    Ok(state)
}

/// Cherry-pick descendants onto the new squash commit and either advance
/// the branch ref (success) or report the next conflict.
fn replay_and_advance(
    repo: &Git2Repo,
    squash_oid: git2::Oid,
    descendants: &[git2::Oid],
    original_branch_oid: Oid,
    label: &str,
    advance_msg: &str,
) -> Result<RebaseOutcome> {
    let ctx = ChainCtx {
        label,
        original_branch_oid: &original_branch_oid,
        moved_commit_oid: None,
    };
    match repo.cherry_pick_chain(squash_oid, descendants, &ctx)? {
        CherryPickResult::Complete(tip) => {
            repo.advance_branch_ref(tip, advance_msg)?;
            repo.checkout_head()?;
            Ok(RebaseOutcome::Complete)
        }
        CherryPickResult::Conflict(state) => Ok(RebaseOutcome::Conflict(state)),
    }
}

/// Apply source's changes onto the target tree, accounting for renames between
/// source's parent and target. Returns the merged tree OID on success, or
/// `None` when the conflict cannot be auto-resolved or no rename was detected.
fn rename_aware_squash_tree(
    repo: &git2::Repository,
    source: &git2::Commit,
    target: &git2::Commit,
) -> Result<Option<git2::Oid>> {
    let source_tree = source.tree()?;
    let target_tree = target.tree()?;
    let base_tree = match source.parent(0) {
        Ok(parent) => Some(parent.tree()?),
        Err(err) if err.code() == git2::ErrorCode::NotFound => None,
        Err(err) => return Err(err.into()),
    };
    let base_tree_ref = base_tree.as_ref();

    let rename_map = build_rename_map(repo, base_tree_ref, &target_tree)?;
    if rename_map.is_empty() {
        return Ok(None);
    }

    let source_diff = repo.diff_tree_to_tree(base_tree_ref, Some(&source_tree), None)?;

    struct RenameEdit {
        base_path: String,
        target_path: String,
        base_oid: git2::Oid,
        target_oid: git2::Oid,
        source_oid: git2::Oid,
    }

    let mut edits: Vec<RenameEdit> = Vec::new();
    for i in 0..source_diff.deltas().len() {
        let delta = source_diff.get_delta(i).unwrap();
        let old_path = match delta.old_file().path().and_then(|p| p.to_str()) {
            Some(p) => p.to_string(),
            None => continue,
        };
        // If the path exists in target under the same name, normal 3-way merge handles it.
        if target_tree
            .get_path(std::path::Path::new(&old_path))
            .is_ok()
        {
            continue;
        }
        if let Some(target_path) = rename_map.get(&old_path) {
            // TreeBuilder::insert takes a plain filename, so skip nested paths for now.
            if target_path.contains('/') {
                continue;
            }
            let base_entry = if let Some(tree) = base_tree_ref {
                tree_entry_optional(tree, &old_path)?
            } else {
                None
            };
            let target_entry = tree_entry_optional(&target_tree, target_path)?;
            let source_entry = tree_entry_optional(&source_tree, &old_path)?;
            if let (Some(be), Some(te), Some(se)) = (base_entry, target_entry, source_entry) {
                edits.push(RenameEdit {
                    base_path: old_path,
                    target_path: target_path.clone(),
                    base_oid: be.id(),
                    target_oid: te.id(),
                    source_oid: se.id(),
                });
            }
        }
    }

    if edits.is_empty() {
        return Ok(None);
    }

    let mut builder = repo.treebuilder(Some(&target_tree))?;
    for edit in &edits {
        let base_blob = repo.find_blob(edit.base_oid)?;
        let target_blob = repo.find_blob(edit.target_oid)?;
        let source_blob = repo.find_blob(edit.source_oid)?;

        let ancestor = make_index_entry(edit.base_oid, &edit.base_path, base_blob.content().len());
        let ours = make_index_entry(
            edit.target_oid,
            &edit.target_path,
            target_blob.content().len(),
        );
        let theirs = make_index_entry(
            edit.source_oid,
            &edit.base_path,
            source_blob.content().len(),
        );

        let result = repo.merge_file_from_index(&ancestor, &ours, &theirs, None)?;
        if !result.is_automergeable() {
            return Ok(None);
        }
        let merged_oid = repo.blob(result.content())?;
        builder.insert(&edit.target_path, merged_oid, 0o100644)?;
    }

    Ok(Some(builder.write()?))
}

fn tree_entry_optional<'repo>(
    tree: &git2::Tree<'repo>,
    path: &str,
) -> Result<Option<git2::TreeEntry<'repo>>> {
    match tree.get_path(std::path::Path::new(path)) {
        Ok(entry) => Ok(Some(entry)),
        Err(err) if err.code() == git2::ErrorCode::NotFound => Ok(None),
        Err(err) => Err(err.into()),
    }
}

/// Detect renames between `base_tree` and `target_tree` by content similarity.
/// Returns a map from the old path (as it existed in base) to the new path (in target).
fn build_rename_map(
    repo: &git2::Repository,
    base_tree: Option<&git2::Tree>,
    target_tree: &git2::Tree,
) -> Result<std::collections::HashMap<String, String>> {
    let diff = repo.diff_tree_to_tree(base_tree, Some(target_tree), None)?;

    let mut deleted: Vec<(String, Vec<u8>)> = Vec::new();
    let mut added: Vec<(String, Vec<u8>)> = Vec::new();

    for i in 0..diff.deltas().len() {
        let delta = diff.get_delta(i).unwrap();
        match delta.status() {
            git2::Delta::Deleted => {
                if let Some(path) = delta.old_file().path().and_then(|p| p.to_str())
                    && let Ok(blob) = repo.find_blob(delta.old_file().id())
                {
                    deleted.push((path.to_string(), blob.content().to_vec()));
                }
            }
            git2::Delta::Added => {
                if let Some(path) = delta.new_file().path().and_then(|p| p.to_str())
                    && let Ok(blob) = repo.find_blob(delta.new_file().id())
                {
                    added.push((path.to_string(), blob.content().to_vec()));
                }
            }
            _ => {}
        }
    }

    let mut rename_map = std::collections::HashMap::new();
    for (del_path, del_content) in &deleted {
        let mut best_score = 0.4_f64;
        let mut best_match: Option<&str> = None;
        for (add_path, add_content) in &added {
            let score = content_similarity(del_content, add_content);
            if score > best_score {
                best_score = score;
                best_match = Some(add_path);
            }
        }
        if let Some(target_path) = best_match {
            rename_map.insert(del_path.clone(), target_path.to_string());
        }
    }

    Ok(rename_map)
}

fn content_similarity(a: &[u8], b: &[u8]) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let prefix = a.iter().zip(b.iter()).take_while(|(x, y)| x == y).count();
    let suffix = a
        .iter()
        .rev()
        .zip(b.iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let common = (prefix + suffix).min(a.len().min(b.len()));
    2.0 * common as f64 / (a.len() + b.len()) as f64
}

fn make_index_entry(oid: git2::Oid, path: &str, file_size: usize) -> git2::IndexEntry {
    git2::IndexEntry {
        ctime: git2::IndexTime::new(0, 0),
        mtime: git2::IndexTime::new(0, 0),
        dev: 0,
        ino: 0,
        mode: 0o100644,
        uid: 0,
        gid: 0,
        file_size: file_size as u32,
        id: oid,
        flags: 0,
        flags_extended: 0,
        path: path.as_bytes().to_vec(),
    }
}
