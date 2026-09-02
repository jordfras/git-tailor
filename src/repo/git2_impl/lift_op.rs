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

//! Lift a working-tree row (the synthetic "staged" / "unstaged" entries) into a
//! temporary commit on top of HEAD, so squash and fixup can treat it as an
//! ordinary source commit.
//!
//! Writing `H` for HEAD's tree, `S` for the index tree and `W` for the working
//! tree, the row's diff is `H → S` (staged) or `S → W` (unstaged), and the
//! temporary commit's tree is:
//!
//! | row      | temp tree            | index afterwards |
//! |----------|----------------------|------------------|
//! | staged   | `S`                  | `S` (unchanged)  |
//! | unstaged | `H` + unstaged delta | `W`              |
//!
//! Either way the files on disk are untouched and the only remaining difference
//! from HEAD is the *other* row's changes, expressed purely as index-vs-HEAD.
//! The squash then runs against a working tree it can safely check out over,
//! and [`finish`] puts the other row's changes back where they came from.

use anyhow::{Context, Result};

use super::journal;
use super::{Git2Repo, WorktreeReset};
use crate::Oid;
use crate::repo::{ConflictState, InProgress, LiftedRow, RebaseOutcome, WorktreeSource};

/// Message on the temporary commit. Only ever visible if git-tailor dies
/// between creating it and folding it away, where naming it plainly helps.
const TEMP_MESSAGE: &str = "git-tailor: working-tree changes";

/// Whether a fold is in flight over a working tree the snapshot still accounts
/// for.
///
/// The snapshot is what makes it safe to run a squash over a dirty working tree,
/// so it only excuses the dirt it actually recorded. A snapshot stranded by an
/// earlier run describes a working tree that is long gone, and must not keep the
/// guard on uncommitted changes switched off.
pub(super) fn covers_working_tree(repo: &Git2Repo) -> Result<bool> {
    let Some(snapshot) = journal::worktree_source(repo)? else {
        return Ok(false);
    };
    let (_, worktree_tree) = snapshot_trees(repo)?;
    Ok(worktree_tree == git2::Oid::from(&snapshot.worktree_tree))
}

/// Create the temporary commit for `source`, or `Ok(None)` when that row has no
/// changes. See [`super::Git2Repo::lift_worktree_row`] for the contract.
pub(super) fn lift(repo: &Git2Repo, source: WorktreeSource) -> Result<Option<LiftedRow>> {
    let head = repo
        .inner
        .head()
        .context("failed to resolve HEAD")?
        .peel_to_commit()
        .context("failed to read HEAD commit")?;
    let head_tree = head.tree().context("failed to read HEAD tree")?;

    let (index_tree_before, worktree_tree) = snapshot_trees(repo)?;
    let temp_tree_oid = match source {
        WorktreeSource::Staged => index_tree_before,
        WorktreeSource::Unstaged => {
            unstaged_only_tree(repo, &head_tree, index_tree_before, worktree_tree)?
        }
    };
    if temp_tree_oid == head_tree.id() {
        return Ok(None);
    }

    // Created before anything is recorded: an unreferenced commit is garbage the
    // next gc collects, where a record naming a commit that was never made
    // describes a state nothing can be recovered to.
    let temp_tree = repo
        .inner
        .find_tree(temp_tree_oid)
        .context("failed to find the working-tree source tree")?;
    let sig = repo
        .inner
        .signature()
        .context("failed to build commit signature (set user.name / user.email)")?;
    let temp_oid = repo
        .inner
        .commit(None, &sig, &sig, TEMP_MESSAGE, &temp_tree, &[&head])
        .context("failed to commit the working-tree changes")?;

    let snapshot = LiftedRow {
        source,
        tip_before: Oid::from(head.id()),
        index_tree_before: Oid::from(index_tree_before),
        worktree_tree: Oid::from(worktree_tree),
        source_tree: Oid::from(temp_tree_oid),
        temp_oid: Oid::from(temp_oid),
    };
    // Write-ahead: the branch is about to move onto the temporary commit, so the
    // record has to be on disk before it does.
    journal::set_worktree_source(repo, Some(snapshot.clone()))?;

    match place_temp_commit(repo, &snapshot) {
        Ok(()) => Ok(Some(snapshot)),
        Err(e) => {
            // Never leave the caller a half-made operation to reason about: the
            // record is already on disk, so unwind through it and report the
            // original failure. Falls back to dropping the record when even that
            // does not work, so nothing is stranded either way.
            if restore(repo, &snapshot).is_err() {
                let _ = journal::set_worktree_source(repo, None);
            }
            Err(e)
        }
    }
}

/// Move the branch onto the temporary commit, leaving the index describing what
/// the row did not take.
fn place_temp_commit(repo: &Git2Repo, snapshot: &LiftedRow) -> Result<()> {
    repo.advance_branch_ref(
        git2::Oid::from(&snapshot.temp_oid),
        "git-tailor: working-tree squash source",
    )?;

    // The unstaged row left its changes in the commit, so what remains staged is
    // the whole working tree relative to it.
    if snapshot.source == WorktreeSource::Unstaged {
        repo.set_index_tree(git2::Oid::from(&snapshot.worktree_tree))?;
    }
    Ok(())
}

/// Unwind back to `snapshot`. See
/// [`super::Git2Repo::restore_lifted_row`] for the contract.
pub(super) fn restore(repo: &Git2Repo, snapshot: &LiftedRow) -> Result<()> {
    // Captured before the ref moves: this is the tree the working tree reflects
    // right now, whether that is the temporary commit or a half-built rewrite.
    let current = head_tree_id(repo)?;
    repo.advance_branch_ref(
        git2::Oid::from(&snapshot.tip_before),
        "git-tailor: abort working-tree squash",
    )?;
    repo.reset_worktree(WorktreeReset {
        from_tree: current,
        worktree_tree: git2::Oid::from(&snapshot.worktree_tree),
        index_tree: git2::Oid::from(&snapshot.index_tree_before),
    })?;
    journal::set_worktree_source(repo, None)?;
    journal::clear_in_progress(repo)
}

/// Put the other row's changes back after the squash reached `tip_after`, and
/// report the resulting index tree so the undo record can restore it — or the
/// merge that says they have nowhere to go.
///
/// Nothing needs merging in the ordinary case: the squash committed exactly the
/// temporary commit's tree, so the working tree goes back to what it was. It is
/// only when the user resolved a conflict along the way that the new tip differs
/// from what was folded in — and then the other row's changes have to be carried
/// onto the resolution rather than reverting it, which is a three-way merge with
/// the temporary commit's tree as the base.
///
/// What ends up in the index differs by row: the staged row's changes are now
/// committed, so the index matches the new tip, while the unstaged row's are
/// committed and the staged ones stay staged, which is the whole working tree.
pub(super) fn finish(repo: &Git2Repo, snapshot: &LiftedRow, tip_after: &Oid) -> Result<Settled> {
    let tip_tree = repo
        .inner
        .find_commit(git2::Oid::from(tip_after))
        .context("failed to read the new branch tip")?
        .tree_id();
    let worktree_tree = match carry_onto(repo, snapshot, tip_tree)? {
        Carried::Tree(tree) => tree,
        Carried::Clash(merged) => return Ok(Settled::Clash(merged)),
    };
    let index_tree = match snapshot.source {
        WorktreeSource::Staged => tip_tree,
        WorktreeSource::Unstaged => worktree_tree,
    };
    repo.reset_worktree(WorktreeReset {
        from_tree: head_tree_id(repo)?,
        worktree_tree,
        index_tree,
    })?;
    Ok(Settled::Done(Oid::from(index_tree)))
}

/// What [`finish`] made of the other row's changes.
pub(super) enum Settled {
    /// They are back where they came from; the index tree the undo record needs.
    Done(Oid),
    /// They cannot go back as they are: the user resolved the fold's conflict
    /// into something they clash with. The merge that says so, for
    /// [`write_clash`] to put in front of the user.
    Clash(git2::Index),
}

/// Paths the carry could not settle on its own.
pub(super) fn clashing_paths(merged: &git2::Index) -> Vec<String> {
    super::conflict::collect_conflict_files_from_index(merged)
}

/// Put a clashing carry in front of the user: the merge into the index, and its
/// conflict markers into the files.
///
/// The caller journals the conflict first — a crash between the two would
/// otherwise leave markers on disk with nothing recording why they are there.
pub(super) fn write_clash(repo: &Git2Repo, merged: &git2::Index) -> Result<()> {
    let mut index = repo.inner.index().context("failed to open index")?;
    index.clear().context("failed to clear the index")?;
    for entry in merged.iter() {
        index
            .add(&entry)
            .context("failed to write the clash into the index")?;
    }
    index.write().context("failed to write the index")?;

    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.force();
    checkout.allow_conflicts(true);
    repo.inner
        .checkout_index(Some(&mut index), Some(&mut checkout))
        .context("failed to write the clash into the working tree")?;
    Ok(())
}

/// Finish a fold whose carry clashed, once the user has resolved it.
///
/// The resolution is already on disk and staged, so there is no merge left to
/// do — what remains is the same tail [`finish`] runs: put the index on the side
/// of the staged/unstaged line the row's own changes came from, and record the
/// whole fold as one undoable step.
pub(super) fn continue_carry(
    repo: &Git2Repo,
    lifted: &LiftedRow,
    state: &ConflictState,
) -> Result<RebaseOutcome> {
    let mut index = repo.inner.index().context("failed to open index")?;
    index.read(true).context("failed to refresh index")?;
    if index.has_conflicts() {
        // Journaled as well as returned: the chain path gets this from the
        // `journaled` wrapper, which a carry does not go through, and a crash
        // here should recover the dialog the user is looking at.
        let unresolved = ConflictState {
            conflicting_files: super::conflict::collect_conflict_files_from_index(&index),
            still_unresolved: true,
            ..state.clone()
        };
        journal::set_in_progress(repo, &InProgress::Conflict(Box::new(unresolved.clone())))?;
        return Ok(RebaseOutcome::Conflict(Box::new(unresolved)));
    }
    let worktree_tree = index
        .write_tree()
        .context("failed to write the resolved working tree")?;

    let tip_after = super::reads::head_oid(repo)?;
    let tip_tree = repo
        .inner
        .find_commit(git2::Oid::from(&tip_after))
        .context("failed to read the new branch tip")?
        .tree_id();
    let index_tree = match lifted.source {
        WorktreeSource::Staged => tip_tree,
        WorktreeSource::Unstaged => worktree_tree,
    };
    // Only the index moves: what the user resolved to is already the working
    // tree, and a checkout over it would be a no-op at best.
    repo.set_index_tree(index_tree)?;

    journal::record_mixed_undo(
        repo,
        &state.operation_label,
        journal::MixedUndo {
            tip_before: &lifted.tip_before,
            tip_after: &tip_after,
            index_tree_before: &lifted.index_tree_before,
            index_tree_after: &Oid::from(index_tree),
        },
    )?;
    journal::set_worktree_source(repo, None)?;
    journal::clear_in_progress(repo)?;
    Ok(RebaseOutcome::Complete)
}

/// The recorded working tree carried onto `tip_tree`.
///
/// Identical to the recorded tree whenever the squash committed what it was
/// given, which is every run that did not stop at a conflict. When the merge
/// itself conflicts the user resolved the fold into something the other row
/// cannot sit on, and that is theirs to settle: the merge comes back for
/// [`write_clash`] rather than being quietly dropped in favour of content that
/// would revert half the resolution.
fn carry_onto(repo: &Git2Repo, snapshot: &LiftedRow, tip_tree: git2::Oid) -> Result<Carried> {
    let recorded = git2::Oid::from(&snapshot.worktree_tree);
    let base = git2::Oid::from(&snapshot.source_tree);
    if tip_tree == base {
        return Ok(Carried::Tree(recorded));
    }
    let mut merged = repo.inner.merge_trees(
        &repo.inner.find_tree(base)?,
        &repo.inner.find_tree(tip_tree)?,
        &repo.inner.find_tree(recorded)?,
        None,
    )?;
    if merged.has_conflicts() {
        return Ok(Carried::Clash(merged));
    }
    Ok(Carried::Tree(merged.write_tree_to(&repo.inner)?))
}

/// The result of carrying the other row's changes onto the new tip.
enum Carried {
    Tree(git2::Oid),
    Clash(git2::Index),
}

/// The index tree and the working tree (tracked paths only) as tree objects.
///
/// Not a plain read: building `W` means staging every tracked change into the
/// repository's *shared* index, so the on-disk index is reloaded afterwards to
/// put it back — unconditionally, including when building the tree failed.
///
/// `update_all` mirrors `stage_all`: it only revisits paths already in the
/// index, so untracked files stay out of `W` exactly as they stay out of the
/// unstaged row's diff. The changes are never written to disk — the on-disk
/// index is reloaded afterwards.
fn snapshot_trees(repo: &Git2Repo) -> Result<(git2::Oid, git2::Oid)> {
    // Without this, a same-size edit would be judged unchanged and silently
    // dropped from `W`.
    repo.refresh_index_stat_cache()?;

    let mut index = repo.inner.index().context("failed to open index")?;
    index.read(true).context("failed to refresh index")?;
    if index.has_conflicts() {
        anyhow::bail!("cannot squash working-tree changes while the index has conflicts");
    }
    let index_tree = index.write_tree().context("failed to write index tree")?;

    // `update_all` judges a submodule by what is checked out at its path, so a
    // pointer with nothing checked out reads as deleted. Submodule contents are
    // part of neither row, so carry the index's pointers through untouched
    // rather than letting the snapshot quietly drop them. Re-adding a pointer
    // `update_all` left alone is a no-op, so this stays correct whichever way
    // libgit2 decides to treat them.
    let gitlink_mode = u32::from(git2::FileMode::Commit);
    let gitlinks: Vec<git2::IndexEntry> = index
        .iter()
        .filter(|entry| entry.mode == gitlink_mode)
        .collect();
    let worktree_tree = build_worktree_tree(&mut index, gitlinks);
    // Unconditional: the staging above happens in the repository's shared
    // in-memory index, so it has to be put back whether or not the tree was
    // built, rather than leaking into whatever reads the index next.
    index
        .read(true)
        .context("failed to restore index after snapshot")?;

    Ok((index_tree, worktree_tree?))
}

/// Stage every tracked change into `index` and write the result as a tree,
/// keeping the submodule pointers `update_all` would drop.
fn build_worktree_tree(
    index: &mut git2::Index,
    gitlinks: Vec<git2::IndexEntry>,
) -> Result<git2::Oid> {
    index
        .update_all(["*"].iter(), None)
        .context("failed to read working-tree changes")?;
    for entry in gitlinks {
        index
            .add(&entry)
            .context("failed to keep a submodule pointer")?;
    }
    index
        .write_tree()
        .context("failed to write working-tree tree")
}

/// HEAD's tree plus the unstaged delta and nothing else — the tree a commit of
/// just the unstaged row would have.
///
/// This is a three-way merge, not a patch: with the index as the base, taking
/// HEAD on one side undoes the staged changes and taking the working tree on the
/// other keeps the unstaged ones, which leaves exactly the row's own changes on
/// top of HEAD. Going through trees rather than hunks means binary files and
/// submodule pointers separate as readily as text, and the only thing that can
/// fail is a genuine overlap — the two rows editing the same lines, which has no
/// answer worth guessing at.
///
/// With nothing staged the base equals HEAD and the result is the working tree
/// itself.
fn unstaged_only_tree(
    repo: &Git2Repo,
    head_tree: &git2::Tree,
    index_tree: git2::Oid,
    worktree_tree: git2::Oid,
) -> Result<git2::Oid> {
    if index_tree == head_tree.id() {
        return Ok(worktree_tree);
    }

    let mut separated = repo
        .inner
        .merge_trees(
            &repo.inner.find_tree(index_tree)?,
            head_tree,
            &repo.inner.find_tree(worktree_tree)?,
            None,
        )
        .context("failed to separate the unstaged changes")?;
    if separated.has_conflicts() {
        anyhow::bail!(
            "the unstaged changes overlap your staged changes and cannot be \
             separated — commit or unstage the staged changes first"
        );
    }
    separated
        .write_tree_to(&repo.inner)
        .context("failed to write the unstaged-only tree")
}

/// Keep `lifted`'s recorded working tree reachable under a ref. See
/// [`super::Git2Repo::rescue_lifted_row`] for the contract.
pub(super) fn rescue(repo: &Git2Repo, lifted: &LiftedRow) -> Result<Option<String>> {
    if head_tree_id(repo)? == git2::Oid::from(&lifted.worktree_tree) {
        return Ok(None);
    }
    let name = journal::rescue_ref(&lifted.worktree_tree);
    repo.inner
        .reference(
            &name,
            git2::Oid::from(&lifted.worktree_tree),
            true,
            "git-tailor: kept the working tree of a discarded fold",
        )
        .context("failed to keep the recorded working tree")?;
    Ok(Some(name))
}

/// The tree HEAD currently points at.
fn head_tree_id(repo: &Git2Repo) -> Result<git2::Oid> {
    Ok(repo
        .inner
        .head()
        .context("failed to resolve HEAD")?
        .peel_to_tree()
        .context("failed to read HEAD tree")?
        .id())
}
