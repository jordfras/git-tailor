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

//! Conflict-resolution operations: continuing or aborting an in-progress
//! rebase, reading the set of conflicting files, and writing a conflicted
//! merge to the working tree.

use anyhow::{Context, Result};

use std::path::{Path, PathBuf};

use super::super::{ConflictState, InProgress, RebaseOutcome, Resume};
use super::Git2Repo;
use super::cherry_pick::{ChainCtx, advance_and_finish};

pub(super) fn rebase_continue(repo: &mut Git2Repo, state: &ConflictState) -> Result<RebaseOutcome> {
    let tip_oid = git2::Oid::from(&state.new_tip_oid);
    let conflicting_oid = git2::Oid::from(&state.conflicting_commit_oid);

    // Re-read index from disk — the user (or another process) resolved
    // conflicts by editing the on-disk index.
    let mut index = repo.inner.index()?;
    index.read(true)?;
    if index.has_conflicts() {
        // The user pressed Enter but some files still have conflict
        // markers. Stay in RebaseConflict mode with a refreshed file
        // list so the dialog keeps the user informed rather than bailing
        // out and leaving the repo in a broken state.
        return Ok(RebaseOutcome::Conflict(Box::new(ConflictState {
            conflicting_files: collect_conflict_files(&repo.inner),
            still_unresolved: true,
            ..state.clone()
        })));
    }

    let new_tree_oid = index.write_tree()?;
    drop(index);

    // Squash-tree conflicts resume via squash_finalize, never here; a Squash
    // resume reaching this point is a routing bug, so fail loudly rather than
    // silently committing a plain chain.
    let Resume::Chain {
        remaining_oids,
        orphan_root,
        moved_commit_oid,
    } = &state.resume
    else {
        anyhow::bail!(
            "rebase_continue called for a squash-tree conflict; resume via squash_finalize"
        );
    };
    let orphan_root = *orphan_root;
    let moved_commit_oid = moved_commit_oid.as_ref();

    // Scoped: these handles borrow the repository, and replaying the rest of the
    // chain below needs it mutably.
    let new_tip = {
        let conflicting_commit = repo.inner.find_commit(conflicting_oid)?;
        let new_tree = repo.inner.find_tree(new_tree_oid)?;
        // An orphan root has no parents; every other commit keeps the tip it
        // conflicted onto.
        let parents: Vec<git2::Commit<'_>> = if orphan_root {
            Vec::new()
        } else {
            vec![repo.inner.find_commit(tip_oid)?]
        };
        repo.commit_preserving_message(
            &conflicting_commit.author(),
            &conflicting_commit.committer(),
            conflicting_commit.message_bytes(),
            conflicting_commit.message_encoding().ok().flatten(),
            &new_tree,
            &parents.iter().collect::<Vec<_>>(),
        )?
    };

    // Continue cherry-picking remaining descendants.
    let remaining: Vec<git2::Oid> = remaining_oids.iter().map(git2::Oid::from).collect();

    let ctx = ChainCtx {
        label: &state.operation_label,
        original_branch_oid: &state.original_branch_oid,
        moved_commit_oid,
    };
    let result = repo.cherry_pick_chain(new_tip, &remaining, &ctx)?;
    let label = state.operation_label.to_lowercase();
    advance_and_finish(
        repo,
        result,
        &state.original_branch_oid,
        &format!("git-tailor: {label} (continue)"),
    )
}

pub(super) fn rebase_abort(repo: &mut Git2Repo, state: &ConflictState) -> Result<()> {
    let original_oid = git2::Oid::from(&state.original_branch_oid);
    let label = state.operation_label.to_lowercase();

    // The conflict checkout wrote exactly the index `write_conflicts_to_workdir`
    // populated, and that index is still in place — so read the list of paths it
    // may have created off it now, before the reset below replaces it.
    let written = index_paths(repo)?;

    // Putting the original tip back is still a checkout: it reintroduces every
    // path the operation removed. Checked before the ref moves, so a refusal
    // leaves the conflict as it was and the user can abort again once the file
    // is out of the way.
    let head_tree_oid = repo
        .inner
        .find_commit(original_oid)?
        .tree()
        .context("failed to read the original tree")?
        .id();
    repo.refuse_tree_collisions(head_tree_oid)?;

    repo.advance_branch_ref(original_oid, &format!("git-tailor: {label} (abort)"))?;

    // Reset the index to HEAD's tree before checkout. write_conflicts_to_workdir
    // clears the index and repopulates it from the cherry-pick result (rooted in
    // the target commit's tree), so checkout_head alone cannot restore files that
    // exist in HEAD but were absent from that tree.
    repo.set_index_tree(head_tree_oid)?;

    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.force();
    repo.inner.checkout_head(Some(&mut checkout))?;

    remove_conflict_debris(repo, &written, head_tree_oid)
}

/// Delete what the conflict left in the working tree that checking out HEAD
/// does not take back: paths the operation introduced which HEAD does not
/// track, and which the checkout therefore has no opinion about.
///
/// `CheckoutBuilder::remove_untracked` would do this in one line, but libgit2
/// does not scope it to our own debris — it removes every untracked file under
/// the checkout, the ones the user wrote while the operation sat paused
/// included. Aborting must undo the operation, not clean the working tree.
fn remove_conflict_debris(
    repo: &mut Git2Repo,
    written: &[PathBuf],
    head_tree_oid: git2::Oid,
) -> Result<()> {
    let workdir = repo
        .inner
        .workdir()
        .ok_or_else(|| anyhow::anyhow!("repository has no working directory"))?
        .to_path_buf();
    let head_tree = repo.inner.find_tree(head_tree_oid)?;

    for path in written {
        // Tracked by HEAD: the checkout above already restored the right content.
        if head_tree.get_path(path).is_ok() {
            continue;
        }
        let full = workdir.join(path);
        if full.symlink_metadata().is_err() {
            continue;
        }
        std::fs::remove_file(&full).with_context(|| {
            format!(
                "failed to remove leftover conflict file `{}`",
                path.display()
            )
        })?;
        remove_empty_parents(&workdir, full.parent());
    }
    Ok(())
}

/// A directory the operation created only to hold a file it introduced would
/// otherwise stay behind, empty, once that file is gone.
fn remove_empty_parents(workdir: &Path, mut dir: Option<&Path>) {
    while let Some(d) = dir {
        if d == workdir || std::fs::remove_dir(d).is_err() {
            return;
        }
        dir = d.parent();
    }
}

/// Every path the index mentions, one entry per path regardless of how many
/// conflict stages it is recorded under.
fn index_paths(repo: &Git2Repo) -> Result<Vec<PathBuf>> {
    let mut index = repo.inner.index()?;
    // The user may have staged resolutions since we wrote it.
    index.read(false)?;
    // Not `String::from_utf8`: a path that is not UTF-8 would drop out of the
    // list and its debris file would be left behind after an abort.
    let mut paths: Vec<PathBuf> = index
        .iter()
        .map(|entry| super::bytes_to_path(&entry.path))
        .collect();
    paths.sort();
    paths.dedup();
    Ok(paths)
}

pub(super) fn read_conflicting_files(repo: &Git2Repo) -> Vec<String> {
    collect_conflict_files(&repo.inner)
}

pub(super) fn auto_stage_resolved_conflicts(repo: &mut Git2Repo, files: &[String]) -> Result<()> {
    let workdir = repo
        .inner
        .workdir()
        .ok_or_else(|| anyhow::anyhow!("repository has no working directory"))?
        .to_path_buf();

    for path in files {
        let full_path = workdir.join(path);
        if !full_path.exists() {
            // File was deleted — stage the deletion to clear conflict entries.
            repo.stage_file(path)?;
            continue;
        }
        let content = std::fs::read(&full_path)
            .with_context(|| format!("failed to read '{path}' from working tree"))?;
        if !content.windows(b"<<<<<<<".len()).any(|w| w == b"<<<<<<<") {
            repo.stage_file(path)?;
        }
    }
    Ok(())
}

/// Read the on-disk index and return paths with conflict (non-zero) stages.
pub(super) fn collect_conflict_files(repo: &git2::Repository) -> Vec<String> {
    let mut index = match repo.index() {
        Ok(i) => i,
        Err(_) => return Vec::new(),
    };
    let _ = index.read(true);
    collect_conflict_files_from_index(&index)
}

/// Return paths with conflict (non-zero) stages from a specific index. Lets
/// callers read conflicts from an in-memory merge index before it is written
/// to the on-disk index.
pub(super) fn collect_conflict_files_from_index(index: &git2::Index) -> Vec<String> {
    let mut paths: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for entry in index.iter() {
        // stage is encoded in the high bits of flags
        let stage = (entry.flags >> 12) & 0x3;
        if stage > 0
            && let Ok(p) = std::str::from_utf8(&entry.path)
        {
            paths.insert(p.to_string());
        }
    }
    paths.into_iter().collect()
}

/// Write a conflicted merge index to the repo index and working tree so
/// the user can resolve conflicts manually.
///
/// The `state` describing the operation is journaled **first**, before any
/// durable change, so that a crash anywhere in this function (advancing the
/// ref, writing the index, checking out) still leaves a recoverable journal
/// entry rather than a partially-rebased branch with no record of it.
pub(super) fn write_conflicts_to_workdir(
    repo: &mut Git2Repo,
    cherry_index: &git2::Index,
    onto_oid: git2::Oid,
    state: &mut ConflictState,
) -> Result<()> {
    // Recorded here because this is where the branch is chosen: whatever HEAD
    // resolves to now is what the ref moves below, and resuming or aborting has
    // to come back to the same one.
    state.branch_refname = repo.current_branch_refname().unwrap_or_default();
    // Before the write-ahead record and the ref move: a conflict is still a
    // checkout over the working tree, and refusing here leaves the branch, the
    // index and the files exactly as they were. The merge is recomputed on the
    // retry, which costs nothing anyone can measure.
    repo.refuse_index_collisions(cherry_index)?;

    // Write-ahead: record the in-progress operation before mutating anything.
    super::journal::set_in_progress(repo, &InProgress::Conflict(Box::new(state.clone())))?;

    // Point the branch at the onto commit so HEAD matches the partially
    // rebased chain.
    let label = state.operation_label.to_lowercase();
    repo.advance_branch_ref(onto_oid, &format!("git-tailor: {label} (conflict)"))?;

    // Write the conflicted index entries (including conflict markers) into
    // the repo's index so `git status` and the user's editor see them.
    let mut repo_index = repo.inner.index()?;
    // Clear stale entries before populating the index with the cherry-pick
    // result.  Without this, leftover files from the previous index state
    // (typically HEAD) leak into the written index and end up in trees
    // created by rebase_continue / squash_finalize.
    repo_index.clear()?;
    for entry in cherry_index.iter() {
        repo_index.add(&entry)?;
    }
    repo_index.write()?;

    // Check out the index to the working tree. Force-checkout writes
    // conflict markers into the working-tree files.
    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.force();
    checkout.allow_conflicts(true);
    repo.inner
        .checkout_index(Some(&mut repo_index), Some(&mut checkout))?;

    Ok(())
}
