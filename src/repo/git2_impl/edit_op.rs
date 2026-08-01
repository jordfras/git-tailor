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

//! "Edit" a commit: rewind the branch to it and check it out (`begin_edit`) so
//! the user can rewrite it by hand in a shell, then splice the resulting
//! user-authored chain back in and replay descendants (`finish_edit`).
//!
//! Architecturally this is a Split whose pieces are user-authored rather than
//! computed: it reuses `checkout_head`, `collect_descendants`, and the
//! conflict-aware `cherry_pick_chain`. The shell itself is spawned by
//! `main.rs`; everything here is pure git so it is testable without a shell.

use anyhow::{Context, Result};

use super::super::{EditInProgress, EditOutcome, InProgress};
use super::Git2Repo;
use super::cherry_pick::{ChainCtx, CherryPickResult};
use super::journal;
use crate::Oid;

/// Rewind the current branch to `commit_oid` and check it out, after recording
/// a write-ahead journal entry so a crash mid-edit is recoverable.
pub(super) fn begin_edit(repo: &Git2Repo, commit_oid: &Oid, head_oid: &Oid) -> Result<()> {
    repo.check_no_dirty_state()?;

    let commit_git = git2::Oid::from(commit_oid);
    let commit = repo.inner.find_commit(commit_git)?;
    if commit.parent_count() > 1 {
        anyhow::bail!("Cannot edit a merge commit");
    }

    // HEAD must be on a branch here (the caller fetched head_oid, which fails on
    // a detached HEAD). Capture the branch name so finish/abort can restore it
    // by name even if the user detaches HEAD inside the shell.
    let branch_refname = current_branch_refname(repo)?;

    // Write-ahead: journal the in-progress edit BEFORE mutating the ref, so a
    // crash anywhere below still leaves a recoverable record.
    journal::set_in_progress(
        repo,
        &InProgress::Edit(EditInProgress {
            branch_refname,
            original_branch_oid: head_oid.clone(),
            edited_commit_oid: commit_oid.clone(),
        }),
    )?;

    // Rewind the branch to the edited commit and sync the working tree to it.
    repo.advance_branch_ref(commit_git, "git-tailor: edit (begin)")?;
    repo.checkout_head(head_oid)?;
    Ok(())
}

/// Splice the user-authored chain (now on the branch) in place of the edited
/// commit and replay the original descendants onto it.
pub(super) fn finish_edit(repo: &Git2Repo, commit_oid: &Oid) -> Result<EditOutcome> {
    let edit = match journal::in_progress(repo)? {
        Some(InProgress::Edit(edit)) => edit,
        _ => anyhow::bail!("no edit in progress"),
    };
    let branch_refname = edit.branch_refname.clone();
    let original = edit.original_branch_oid.clone();

    let commit_git = git2::Oid::from(commit_oid);
    let commit = repo.inner.find_commit(commit_git)?;
    // `None` when editing the root commit — there is no parent to build on.
    let parent = if commit.parent_count() > 0 {
        Some(commit.parent_id(0)?)
    } else {
        None
    };

    let branch_tip = repo
        .inner
        .refname_to_id(&branch_refname)
        .with_context(|| format!("failed to resolve {branch_refname}"))?;

    // Uncommitted changes must NEVER be discarded. Check this *before* the
    // no-op and abort paths below — both restore the original tip with a force
    // checkout, which would silently lose the user's work. Bail without
    // restoring so the work stays on disk and the edit remains in progress; the
    // caller re-opens the shell so the user can commit or discard it.
    if repo.is_worktree_dirty()? {
        anyhow::bail!(
            "Edit not applied: the working tree has uncommitted changes — \
             commit or discard them, then finish the edit."
        );
    }

    // No-op / cancel: the user exited without changing the commit (tree is
    // clean here, so restoring the original tip loses nothing).
    if branch_tip == commit_git {
        restore_original(repo, &branch_refname, &original)?;
        journal::clear_in_progress(repo)?;
        return Ok(EditOutcome::Cancelled);
    }

    // Validate the state the user left. On anything unexpected, restore the
    // branch to its original tip and error out rather than rewrite blindly.
    let abort = |reason: &str| -> Result<EditOutcome> {
        restore_original(repo, &branch_refname, &original)?;
        journal::clear_in_progress(repo)?;
        anyhow::bail!("Edit aborted: {reason}. Restored the branch to its previous state.")
    };

    if !head_on_branch(repo, &branch_refname) {
        return abort("HEAD is no longer on the edited branch");
    }
    // The new tip must not still contain the edited commit (or its old
    // descendants) — replaying descendants onto such a tip would duplicate
    // them. This also rejects a reset back onto the old history.
    if repo.inner.graph_descendant_of(branch_tip, commit_git)? {
        return abort("the new commits still include the edited commit");
    }
    // For a non-root commit, the new tip must build on the edited commit's
    // parent (== parent means the content was fully discarded — like a drop).
    // The root commit has no parent to check against.
    if let Some(parent) = parent {
        let built_on_parent =
            branch_tip == parent || repo.inner.graph_descendant_of(branch_tip, parent)?;
        if !built_on_parent {
            return abort("the new commits do not build on the edited commit's parent");
        }
    }
    if repo.range_has_merge(parent, branch_tip)? {
        return abort("a merge commit was created");
    }

    // Replay the original descendants onto the user's chain. A conflict here
    // routes through the normal RebaseConflict flow (cherry_pick_chain journals
    // the conflict state, replacing this edit's in-progress record).
    let original_git = git2::Oid::from(&original);
    let descendants = repo.collect_descendants(commit_git, original_git)?;
    let ctx = ChainCtx {
        label: "Edit",
        original_branch_oid: &original,
        moved_commit_oid: None,
    };
    match repo.cherry_pick_chain(branch_tip, &descendants, &ctx)? {
        CherryPickResult::Complete(tip) => {
            repo.advance_branch_ref(tip, "git-tailor: edit")?;
            // The working tree currently reflects the user's chain tip.
            repo.checkout_head(&Oid::from(branch_tip))?;
            journal::clear_in_progress(repo)?;
            Ok(EditOutcome::Complete)
        }
        CherryPickResult::Conflict(state) => Ok(EditOutcome::Conflict(state)),
    }
}

/// Restore the branch to its original tip (abort / crash-recovery), using the
/// branch name + original tip recorded in the in-progress journal.
pub(super) fn abort_edit(repo: &Git2Repo) -> Result<()> {
    let Some(InProgress::Edit(edit)) = journal::in_progress(repo)? else {
        return Ok(());
    };
    restore_original(repo, &edit.branch_refname, &edit.original_branch_oid)?;
    journal::clear_in_progress(repo)?;
    Ok(())
}

/// Force the named branch back to `original`, reattach HEAD to it (in case the
/// user detached HEAD or checked out elsewhere in the shell), and hard-reset
/// the index + working tree to match.
fn restore_original(repo: &Git2Repo, branch_refname: &str, original: &Oid) -> Result<()> {
    let original_git = git2::Oid::from(original);
    repo.inner.reference(
        branch_refname,
        original_git,
        true,
        "git-tailor: edit (abort)",
    )?;
    repo.inner.set_head(branch_refname)?;

    let commit = repo.inner.find_commit(original_git)?;
    let mut index = repo.inner.index()?;
    index.read_tree(&commit.tree()?)?;
    index.write()?;

    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.force();
    checkout.remove_untracked(true);
    repo.inner.checkout_head(Some(&mut checkout))?;
    Ok(())
}

/// Full ref name of the branch HEAD points at. Errors if HEAD is detached.
fn current_branch_refname(repo: &Git2Repo) -> Result<String> {
    let head = repo.inner.head()?;
    let name = head
        .resolve()
        .context("HEAD is not on a branch")?
        .name()
        .context("branch ref has no name")?
        .to_string();
    Ok(name)
}

/// Whether HEAD is currently a symbolic ref pointing at `branch_refname`.
fn head_on_branch(repo: &Git2Repo, branch_refname: &str) -> bool {
    if repo.inner.head_detached().unwrap_or(true) {
        return false;
    }
    let head = match repo.inner.find_reference("HEAD") {
        Ok(h) => h,
        Err(_) => return false,
    };
    matches!(head.symbolic_target(), Ok(Some(t)) if t == branch_refname)
}
