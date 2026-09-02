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

use super::super::{ConflictState, InProgress, RebaseOutcome, Resume};
use super::Git2Repo;
use super::cherry_pick::{ChainCtx, advance_and_finish};

pub(super) fn rebase_continue(repo: &Git2Repo, state: &ConflictState) -> Result<RebaseOutcome> {
    let tip_oid = git2::Oid::from(&state.new_tip_oid);
    let conflicting_oid = git2::Oid::from(&state.conflicting_commit_oid);
    let conflicting_commit = repo.inner.find_commit(conflicting_oid)?;
    let onto_commit = repo.inner.find_commit(tip_oid)?;

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
    let new_tree = repo.inner.find_tree(new_tree_oid)?;

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

    let new_tip = if orphan_root {
        // The conflicting commit becomes an orphan root (no parents).
        repo.inner.commit(
            None,
            &conflicting_commit.author(),
            &conflicting_commit.committer(),
            conflicting_commit.message().unwrap_or(""),
            &new_tree,
            &[],
        )?
    } else {
        repo.inner.commit(
            None,
            &conflicting_commit.author(),
            &conflicting_commit.committer(),
            conflicting_commit.message().unwrap_or(""),
            &new_tree,
            &[&onto_commit],
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

pub(super) fn rebase_abort(repo: &Git2Repo, state: &ConflictState) -> Result<()> {
    let original_oid = git2::Oid::from(&state.original_branch_oid);
    let label = state.operation_label.to_lowercase();
    repo.advance_branch_ref(original_oid, &format!("git-tailor: {label} (abort)"))?;

    // Reset the index to HEAD's tree before checkout. write_conflicts_to_workdir
    // clears the index and repopulates it from the cherry-pick result (rooted in
    // the target commit's tree), so checkout_head alone cannot restore files that
    // exist in HEAD but were absent from that tree.
    let head_commit = repo.inner.find_commit(original_oid)?;
    repo.set_index_tree(head_commit.tree()?.id())?;

    // Force-checkout HEAD and remove files that were written to the workdir
    // by the conflict checkout but are not tracked by the original HEAD.
    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.force();
    checkout.remove_untracked(true);
    repo.inner.checkout_head(Some(&mut checkout))?;
    Ok(())
}

pub(super) fn read_conflicting_files(repo: &Git2Repo) -> Vec<String> {
    collect_conflict_files(&repo.inner)
}

pub(super) fn auto_stage_resolved_conflicts(repo: &Git2Repo, files: &[String]) -> Result<()> {
    let workdir = repo
        .inner
        .workdir()
        .ok_or_else(|| anyhow::anyhow!("repository has no working directory"))?;

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
    repo: &Git2Repo,
    cherry_index: &git2::Index,
    onto_commit: &git2::Commit,
    state: &ConflictState,
) -> Result<()> {
    // Write-ahead: record the in-progress operation before mutating anything.
    super::journal::set_in_progress(repo, &InProgress::Conflict(Box::new(state.clone())))?;

    // Point the branch at the onto commit so HEAD matches the partially
    // rebased chain.
    let label = state.operation_label.to_lowercase();
    repo.advance_branch_ref(onto_commit.id(), &format!("git-tailor: {label} (conflict)"))?;

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
