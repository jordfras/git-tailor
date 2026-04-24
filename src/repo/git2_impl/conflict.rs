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

use super::super::{ConflictState, RebaseOutcome};
use super::CherryPickResult;
use super::Git2Repo;

pub(super) fn rebase_continue(repo: &Git2Repo, state: &ConflictState) -> Result<RebaseOutcome> {
    let tip_oid =
        git2::Oid::from_str(&state.new_tip_oid).context("Invalid tip OID in conflict state")?;
    let conflicting_oid = git2::Oid::from_str(&state.conflicting_commit_oid)
        .context("Invalid conflicting OID in conflict state")?;
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
            operation_label: state.operation_label.clone(),
            original_branch_oid: state.original_branch_oid.clone(),
            new_tip_oid: state.new_tip_oid.clone(),
            conflicting_commit_oid: state.conflicting_commit_oid.clone(),
            remaining_oids: state.remaining_oids.clone(),
            conflicting_files: collect_conflict_files(&repo.inner),
            still_unresolved: true,
            moved_commit_oid: state.moved_commit_oid.clone(),
            squash_context: state.squash_context.clone(),
        })));
    }

    let new_tree_oid = index.write_tree()?;
    let new_tree = repo.inner.find_tree(new_tree_oid)?;

    let new_tip = repo.inner.commit(
        None,
        &conflicting_commit.author(),
        &conflicting_commit.committer(),
        conflicting_commit.message().unwrap_or(""),
        &new_tree,
        &[&onto_commit],
    )?;

    // Continue cherry-picking remaining descendants.
    let remaining: Vec<git2::Oid> = state
        .remaining_oids
        .iter()
        .map(|s| git2::Oid::from_str(s))
        .collect::<std::result::Result<_, _>>()
        .context("Invalid OID in remaining list")?;

    let result = repo.cherry_pick_chain(new_tip, &remaining)?;
    match result {
        CherryPickResult::Complete(final_tip) => {
            let label = state.operation_label.to_lowercase();
            repo.advance_branch_ref(final_tip, &format!("git-tailor: {label} (continue)"))?;
            repo.checkout_head()?;
            Ok(RebaseOutcome::Complete)
        }
        CherryPickResult::Conflict {
            tip,
            conflicting_idx,
        } => {
            let conflicting_oid = remaining[conflicting_idx];
            let new_remaining: Vec<String> = remaining[conflicting_idx + 1..]
                .iter()
                .map(|oid| oid.to_string())
                .collect();

            Ok(RebaseOutcome::Conflict(Box::new(ConflictState {
                operation_label: state.operation_label.clone(),
                original_branch_oid: state.original_branch_oid.clone(),
                new_tip_oid: tip.to_string(),
                conflicting_commit_oid: conflicting_oid.to_string(),
                remaining_oids: new_remaining,
                conflicting_files: collect_conflict_files(&repo.inner),
                still_unresolved: false,
                moved_commit_oid: state.moved_commit_oid.clone(),
                squash_context: None,
            })))
        }
    }
}

pub(super) fn rebase_abort(repo: &Git2Repo, state: &ConflictState) -> Result<()> {
    let original_oid = git2::Oid::from_str(&state.original_branch_oid)
        .context("Invalid original branch OID in conflict state")?;
    let label = state.operation_label.to_lowercase();
    repo.advance_branch_ref(original_oid, &format!("git-tailor: {label} (abort)"))?;

    // Reset the index to HEAD's tree before checkout. write_conflicts_to_workdir
    // clears the index and repopulates it from the cherry-pick result (rooted in
    // the target commit's tree), so checkout_head alone cannot restore files that
    // exist in HEAD but were absent from that tree.
    let head_commit = repo.inner.find_commit(original_oid)?;
    let mut index = repo.inner.index()?;
    index.read_tree(&head_commit.tree()?)?;
    index.write()?;

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

/// Read the index and return paths with conflict (non-zero) stages.
pub(super) fn collect_conflict_files(repo: &git2::Repository) -> Vec<String> {
    let mut index = match repo.index() {
        Ok(i) => i,
        Err(_) => return Vec::new(),
    };
    let _ = index.read(true);
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
pub(super) fn write_conflicts_to_workdir(
    repo: &Git2Repo,
    cherry_index: &git2::Index,
    onto_commit: &git2::Commit,
) -> Result<()> {
    // Point the branch at the onto commit so HEAD matches the partially
    // rebased chain.
    repo.advance_branch_ref(onto_commit.id(), "git-tailor: drop commit (conflict)")?;

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
