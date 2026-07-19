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

//! Split a single commit into multiple commits — per file, per hunk, or per
//! hunk-group (where groups come from the fragmap clustering).  Also exposes
//! "count" helpers used by the UI to decide whether each strategy is
//! applicable.

use anyhow::{Context, Result};
use std::collections::{BTreeSet, HashMap, HashSet};

use crate::{Oid, fragmap};

use super::Git2Repo;
use super::hunks;
use super::reads;

pub(super) fn split_commit_per_file(
    repo: &Git2Repo,
    commit_oid: &Oid,
    head_oid: &Oid,
) -> Result<()> {
    let target = load_split_commit(repo, commit_oid)?;

    let full_diff =
        repo.inner
            .diff_tree_to_tree(Some(&target.parent_tree), Some(&target.commit_tree), None)?;
    let file_count = full_diff.deltas().len();

    if file_count < 2 {
        anyhow::bail!("Commit touches fewer than 2 files — nothing to split");
    }

    repo.check_dirty_overlap(&collect_commit_paths(&full_diff, true))?;

    let mut current_base = initial_split_base(&target.commit)?;
    let mut current_tree_oid = target.parent_tree.id();
    for delta_idx in 0..file_count {
        let delta = full_diff.get_delta(delta_idx).expect("delta index valid");
        let path = delta
            .new_file()
            .path()
            .or_else(|| delta.old_file().path())
            .expect("delta has a path")
            .to_string_lossy()
            .into_owned();

        let base_tree = repo.inner.find_tree(current_tree_oid)?;

        let is_gitlink = delta.new_file().mode() == git2::FileMode::Commit
            || delta.old_file().mode() == git2::FileMode::Commit;

        let new_tree_oid = if is_gitlink {
            // apply_to_tree cannot process gitlink (submodule pointer) entries
            // because libgit2 tries to patch them as blobs, causing a crash.
            hunks::apply_gitlink_delta_to_tree(&repo.inner, &base_tree, &delta)?
        } else {
            let mut opts = git2::DiffOptions::new();
            opts.pathspec(&path);
            let file_diff = repo.inner.diff_tree_to_tree(
                Some(&target.parent_tree),
                Some(&target.commit_tree),
                Some(&mut opts),
            )?;
            let mut new_index = repo.inner.apply_to_tree(&base_tree, &file_diff, None)?;
            if new_index.has_conflicts() {
                anyhow::bail!("Conflict applying changes for file: {}", path);
            }
            new_index.write_tree_to(&repo.inner)?
        };

        current_base = Some(commit_split_piece(
            repo,
            &target.commit,
            new_tree_oid,
            current_base,
            delta_idx + 1,
            file_count,
        )?);
        current_tree_oid = new_tree_oid;
    }

    finalize_split(
        repo,
        target.commit_oid,
        head_oid,
        current_base.expect("loop produced at least two commits"),
        "git-tailor: split per-file",
    )
}

pub(super) fn split_commit_per_hunk(
    repo: &Git2Repo,
    commit_oid: &Oid,
    head_oid: &Oid,
) -> Result<()> {
    let target = load_split_commit(repo, commit_oid)?;

    let mut diff_opts = zero_context_diff_opts();
    let full_diff = repo.inner.diff_tree_to_tree(
        Some(&target.parent_tree),
        Some(&target.commit_tree),
        Some(&mut diff_opts),
    )?;

    let hunk_count = count_hunks(&full_diff)?;
    if hunk_count < 2 {
        anyhow::bail!("Commit has fewer than 2 hunks — nothing to split per hunk");
    }

    repo.check_dirty_overlap(&collect_commit_paths(&full_diff, false))?;

    // Build one commit per hunk using incremental blob manipulation.  At each
    // step, recompute diff(current_tree → commit_tree) with 0 context and
    // apply exactly its first hunk directly to the blob — bypassing
    // apply_to_tree to avoid libgit2 validating rejected hunks against the
    // modified output buffer (which shifts line positions and causes "hunk
    // did not apply").
    let mut current_base = initial_split_base(&target.commit)?;
    let mut current_tree_oid = target.parent_tree.id();
    for target_k in 0..hunk_count {
        let next_tree_oid = if target_k == hunk_count - 1 {
            target.commit_tree.id()
        } else {
            let current_tree = repo.inner.find_tree(current_tree_oid)?;
            let mut diff_opts = zero_context_diff_opts();
            let incremental_diff = repo.inner.diff_tree_to_tree(
                Some(&current_tree),
                Some(&target.commit_tree),
                Some(&mut diff_opts),
            )?;
            hunks::apply_single_hunk_to_tree(&repo.inner, &current_tree, &incremental_diff)
                .with_context(|| format!("applying hunk {}", target_k + 1))?
        };

        current_base = Some(commit_split_piece(
            repo,
            &target.commit,
            next_tree_oid,
            current_base,
            target_k + 1,
            hunk_count,
        )?);
        current_tree_oid = next_tree_oid;
    }

    finalize_split(
        repo,
        target.commit_oid,
        head_oid,
        current_base.expect("loop produced at least two commits"),
        "git-tailor: split per-hunk",
    )
}

pub(super) fn split_commit_per_hunk_group(
    repo: &Git2Repo,
    commit_oid: &Oid,
    head_oid: &Oid,
    reference_oid: &Oid,
) -> Result<()> {
    let target = load_split_commit(repo, commit_oid)?;

    // Build the fragmap over all branch commits so hunk grouping reflects how
    // this commit interacts with its neighbours in the branch.  In --all mode
    // the root commit IS the reference point, so the commit being split must
    // be kept even when it equals `reference_oid`.
    let assignment =
        compute_hunk_group_assignment(repo, commit_oid, head_oid, reference_oid, true)?;

    // Build a 0-context full diff (parent_tree → commit_tree) for tree
    // manipulation; hunk indices here correspond to those in `assignment`.
    let mut diff_opts = zero_context_diff_opts();
    let full_diff = repo.inner.diff_tree_to_tree(
        Some(&target.parent_tree),
        Some(&target.commit_tree),
        Some(&mut diff_opts),
    )?;

    repo.check_dirty_overlap(&collect_commit_paths(&full_diff, false))?;

    // For each (delta_idx, hunk_idx), record its group index.
    let mut delta_hunk_groups: Vec<Vec<usize>> = Vec::new();
    let num_deltas = full_diff.deltas().len();
    for delta_idx in 0..num_deltas {
        let delta = full_diff.get_delta(delta_idx).context("delta index")?;
        let path = delta
            .new_file()
            .path()
            .or_else(|| delta.old_file().path())
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let patch = git2::Patch::from_diff(&full_diff, delta_idx)?;
        let num_hunks = patch.as_ref().map(|p| p.num_hunks()).unwrap_or(0);
        let file_groups = assignment.by_file.get(&path);
        let groups_for_delta: Vec<usize> = (0..num_hunks)
            .map(|h| {
                file_groups
                    .and_then(|fg| fg.get(h))
                    .and_then(|a| a.whole_group())
                    .unwrap_or(0)
            })
            .collect();
        delta_hunk_groups.push(groups_for_delta);
    }

    // Only group indices touched by K's hunks produce output commits — not
    // every column the full fragmap has.
    let mut touched: BTreeSet<usize> = BTreeSet::new();
    for groups in &delta_hunk_groups {
        touched.extend(groups);
    }
    let k_groups: Vec<usize> = touched.into_iter().collect();
    let split_count = k_groups.len();

    if split_count < 2 {
        anyhow::bail!("Commit has fewer than 2 hunk groups — nothing to split per hunk group");
    }

    // For each touched group gk (in order), build the intermediate tree by
    // applying all of K's hunks whose group index ≤ gk to parent_tree in one
    // sweep (positions relative to the original, no cumulative offset issues).
    let mut current_base = initial_split_base(&target.commit)?;
    for (out_pos, &gk) in k_groups.iter().enumerate() {
        let next_tree_oid = if out_pos == split_count - 1 {
            target.commit_tree.id()
        } else {
            let mut selected: HashMap<usize, Vec<usize>> = HashMap::new();
            for (delta_idx, hunk_groups) in delta_hunk_groups.iter().enumerate() {
                let chosen: Vec<usize> = hunk_groups
                    .iter()
                    .enumerate()
                    .filter_map(|(h, &g)| if g <= gk { Some(h) } else { None })
                    .collect();
                if !chosen.is_empty() {
                    selected.insert(delta_idx, chosen);
                }
            }
            hunks::apply_selected_hunks_to_tree(
                &repo.inner,
                &target.parent_tree,
                &full_diff,
                &selected,
            )
            .with_context(|| format!("building tree for hunk group {}", out_pos + 1))?
        };

        current_base = Some(commit_split_piece(
            repo,
            &target.commit,
            next_tree_oid,
            current_base,
            out_pos + 1,
            split_count,
        )?);
    }

    finalize_split(
        repo,
        target.commit_oid,
        head_oid,
        current_base.expect("loop produced at least two commits"),
        "git-tailor: split per-hunk-group",
    )
}

pub(super) fn count_split_per_file(repo: &Git2Repo, commit_oid: &Oid) -> Result<usize> {
    let target = load_split_commit(repo, commit_oid)?;
    let diff =
        repo.inner
            .diff_tree_to_tree(Some(&target.parent_tree), Some(&target.commit_tree), None)?;
    Ok(diff.deltas().len())
}

pub(super) fn count_split_per_hunk(repo: &Git2Repo, commit_oid: &Oid) -> Result<usize> {
    let target = load_split_commit(repo, commit_oid)?;
    let mut diff_opts = zero_context_diff_opts();
    let diff = repo.inner.diff_tree_to_tree(
        Some(&target.parent_tree),
        Some(&target.commit_tree),
        Some(&mut diff_opts),
    )?;
    count_hunks(&diff)
}

pub(super) fn count_split_per_hunk_group(
    repo: &Git2Repo,
    commit_oid: &Oid,
    head_oid: &Oid,
    reference_oid: &Oid,
) -> Result<usize> {
    let assignment =
        compute_hunk_group_assignment(repo, commit_oid, head_oid, reference_oid, false)?;
    Ok(assignment.touched_groups().len())
}

pub(super) fn split_commit_out_file(
    repo: &Git2Repo,
    commit_oid: &Oid,
    file_path: &str,
    head_oid: &Oid,
) -> Result<()> {
    let target = load_split_commit(repo, commit_oid)?;

    let full_diff =
        repo.inner
            .diff_tree_to_tree(Some(&target.parent_tree), Some(&target.commit_tree), None)?;
    let file_count = full_diff.deltas().len();
    if file_count < 2 {
        anyhow::bail!("Commit touches fewer than 2 files — nothing to split out");
    }

    let chosen = (0..file_count)
        .find_map(|i| {
            let delta = full_diff.get_delta(i)?;
            (delta_path(&delta).as_deref() == Some(file_path)).then_some(delta)
        })
        .ok_or_else(|| anyhow::anyhow!("File not changed by this commit: {file_path}"))?;

    repo.check_dirty_overlap(&collect_commit_paths(&full_diff, true))?;

    // The first commit keeps every change except `file_path`.  Build its tree
    // by taking the full commit tree and reverting just the chosen path to its
    // parent state (or removing it when the file was newly added).  This pure
    // tree surgery handles added/modified/deleted files and gitlinks uniformly
    // and can never produce a merge conflict.
    let mut builder = git2::build::TreeUpdateBuilder::new();
    if chosen.old_file().id().is_zero() {
        builder.remove(file_path);
    } else {
        builder.upsert(file_path, chosen.old_file().id(), chosen.old_file().mode());
    }
    let rest_tree_oid = builder.create_updated(&repo.inner, &target.commit_tree)?;

    let base = initial_split_base(&target.commit)?;
    let original_message = target.commit.message().unwrap_or("split");
    let first = commit_with_message(repo, &target.commit, rest_tree_oid, base, original_message)?;

    let peeled_message = hunks::summary_suffix_message(original_message, file_path);
    let second = commit_with_message(
        repo,
        &target.commit,
        target.commit_tree.id(),
        Some(first),
        &peeled_message,
    )?;

    finalize_split(
        repo,
        target.commit_oid,
        head_oid,
        second,
        "git-tailor: split out file",
    )
}

/// Resolved inputs to a split operation.  `commit_oid` is the parsed form of
/// the caller's `&str` argument; `parent_tree` is an empty tree when `commit`
/// is a root commit.
struct SplitTarget<'r> {
    commit_oid: git2::Oid,
    commit: git2::Commit<'r>,
    parent_tree: git2::Tree<'r>,
    commit_tree: git2::Tree<'r>,
}

/// Parse `commit_oid`, look up the commit, validate it's not a merge, and
/// load the parent and commit trees.
fn load_split_commit<'r>(repo: &'r Git2Repo, commit_oid: &Oid) -> Result<SplitTarget<'r>> {
    let oid = git2::Oid::from(commit_oid);
    let commit = repo.inner.find_commit(oid)?;
    if commit.parent_count() > 1 {
        anyhow::bail!("Cannot split a merge commit");
    }
    let parent_tree = if commit.parent_count() == 0 {
        let empty_oid = repo.inner.treebuilder(None)?.write()?;
        repo.inner.find_tree(empty_oid)?
    } else {
        commit.parent(0)?.tree()?
    };
    let commit_tree = commit.tree()?;
    Ok(SplitTarget {
        commit_oid: oid,
        commit,
        parent_tree,
        commit_tree,
    })
}

/// Parent OID for the first split piece, or `None` for a root commit (the
/// first piece becomes a new orphan root and subsequent pieces stack on it).
fn initial_split_base(commit: &git2::Commit<'_>) -> Result<Option<git2::Oid>> {
    if commit.parent_count() == 0 {
        Ok(None)
    } else {
        Ok(Some(commit.parent_id(0)?))
    }
}

/// Collect the file paths touched by `diff`.  When `exclude_gitlinks` is set,
/// submodule-pointer deltas are skipped — used by the per-file split path
/// which applies them via tree manipulation only and so cannot be tripped by
/// a dirty submodule state.
fn collect_commit_paths(diff: &git2::Diff<'_>, exclude_gitlinks: bool) -> HashSet<String> {
    diff.deltas()
        .filter(|d| {
            !exclude_gitlinks
                || (d.new_file().mode() != git2::FileMode::Commit
                    && d.old_file().mode() != git2::FileMode::Commit)
        })
        .filter_map(|d| {
            d.new_file()
                .path()
                .or_else(|| d.old_file().path())
                .map(|p| p.to_string_lossy().into_owned())
        })
        .collect()
}

/// `DiffOptions` configured to keep adjacent hunks separate (no surrounding
/// or inter-hunk context).
fn zero_context_diff_opts() -> git2::DiffOptions {
    let mut opts = git2::DiffOptions::new();
    opts.context_lines(0);
    opts.interhunk_lines(0);
    opts
}

/// Total hunk count across all files in `diff`.
fn count_hunks(diff: &git2::Diff<'_>) -> Result<usize> {
    let mut count = 0usize;
    diff.foreach(
        &mut |_, _| true,
        None,
        Some(&mut |_, _| {
            count += 1;
            true
        }),
        None,
    )?;
    Ok(count)
}

/// Run the fragmap hunk-group clustering for the branch `head_oid..reference_oid`
/// and return the per-file group assignment for `commit_oid`.  When
/// `keep_self_when_reference` is set, `commit_oid` is included even if it
/// equals `reference_oid` (needed by the per-hunk-group split, which must
/// treat the root commit as a normal commit in --all mode).
fn compute_hunk_group_assignment(
    repo: &Git2Repo,
    commit_oid: &Oid,
    head_oid: &Oid,
    reference_oid: &Oid,
    keep_self_when_reference: bool,
) -> Result<fragmap::HunkGroupAssignment> {
    let branch_commits = reads::list_commits(repo, head_oid, reference_oid)?;
    let branch_diffs: Vec<crate::CommitDiff> = branch_commits
        .iter()
        .filter(|c| {
            let is_reference = c.oid.as_oid() == Some(reference_oid);
            let is_split_commit = c.oid.as_oid() == Some(commit_oid);
            let keep = if keep_self_when_reference {
                !is_reference || is_split_commit
            } else {
                !is_reference
            };
            keep && !c.oid.is_synthetic()
        })
        .map(|c| {
            c.oid
                .as_oid()
                .map(|oid| reads::commit_diff_for_fragmap(repo, oid))
                .transpose()
        })
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect();

    fragmap::assign_hunk_groups(&branch_diffs, commit_oid)
        .ok_or_else(|| anyhow::anyhow!("Commit {} not found in branch diff list", commit_oid))
}

/// Create one piece of a split: a commit with the given tree, parented on
/// `current_base` (or as an orphan root when `None`), inheriting author and
/// committer from `original` and using a numbered split message.
fn commit_split_piece(
    repo: &Git2Repo,
    original: &git2::Commit<'_>,
    new_tree_oid: git2::Oid,
    current_base: Option<git2::Oid>,
    piece_num: usize,
    total_pieces: usize,
) -> Result<git2::Oid> {
    let message = hunks::split_message(
        original.message().unwrap_or("split"),
        piece_num,
        total_pieces,
    );
    commit_with_message(repo, original, new_tree_oid, current_base, &message)
}

/// Create a commit with the given tree and message, parented on `current_base`
/// (or as an orphan root when `None`), inheriting author and committer from
/// `original`.
fn commit_with_message(
    repo: &Git2Repo,
    original: &git2::Commit<'_>,
    new_tree_oid: git2::Oid,
    current_base: Option<git2::Oid>,
    message: &str,
) -> Result<git2::Oid> {
    let new_tree = repo.inner.find_tree(new_tree_oid)?;
    let parents: Vec<git2::Commit> = match current_base {
        Some(oid) => vec![repo.inner.find_commit(oid)?],
        None => vec![],
    };
    let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
    Ok(repo.inner.commit(
        None,
        &original.author(),
        &original.committer(),
        message,
        &new_tree,
        &parent_refs,
    )?)
}

/// New-or-old path of a delta as an owned `String`.
fn delta_path(delta: &git2::DiffDelta<'_>) -> Option<String> {
    delta
        .new_file()
        .path()
        .or_else(|| delta.old_file().path())
        .map(|p| p.to_string_lossy().into_owned())
}

/// Replay descendants of the split commit onto the last split piece and
/// fast-forward the branch ref.
fn finalize_split(
    repo: &Git2Repo,
    original_commit_oid: git2::Oid,
    head_oid: &Oid,
    final_tip: git2::Oid,
    log_msg: &str,
) -> Result<()> {
    let head_git_oid = git2::Oid::from(head_oid);
    let rebased_tip = repo.rebase_descendants(original_commit_oid, head_git_oid, final_tip)?;
    repo.advance_branch_ref(rebased_tip, log_msg)?;
    Ok(())
}
