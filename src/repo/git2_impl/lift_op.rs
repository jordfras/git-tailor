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
use crate::repo::{LiftedRow, WorktreeSource};

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
/// report the resulting index tree so the undo record can restore it.
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
pub(super) fn finish(repo: &Git2Repo, snapshot: &LiftedRow, tip_after: &Oid) -> Result<Oid> {
    let tip_tree = repo
        .inner
        .find_commit(git2::Oid::from(tip_after))
        .context("failed to read the new branch tip")?
        .tree_id();
    let worktree_tree = carry_onto(repo, snapshot, tip_tree)?;
    let index_tree = match snapshot.source {
        WorktreeSource::Staged => tip_tree,
        WorktreeSource::Unstaged => worktree_tree,
    };
    repo.reset_worktree(WorktreeReset {
        from_tree: head_tree_id(repo)?,
        worktree_tree,
        index_tree,
    })?;
    Ok(Oid::from(index_tree))
}

/// The recorded working tree carried onto `tip_tree`.
///
/// Identical to the recorded tree whenever the squash committed what it was
/// given, which is every run that did not stop at a conflict. Falls back to it
/// too when the merge itself conflicts — the user's changes are all still there,
/// which is what matters, even if they no longer sit on the resolution.
fn carry_onto(repo: &Git2Repo, snapshot: &LiftedRow, tip_tree: git2::Oid) -> Result<git2::Oid> {
    let recorded = git2::Oid::from(&snapshot.worktree_tree);
    let base = git2::Oid::from(&snapshot.source_tree);
    if tip_tree == base {
        return Ok(recorded);
    }
    let mut merged = repo.inner.merge_trees(
        &repo.inner.find_tree(base)?,
        &repo.inner.find_tree(tip_tree)?,
        &repo.inner.find_tree(recorded)?,
        None,
    )?;
    if merged.has_conflicts() {
        return Ok(recorded);
    }
    Ok(merged.write_tree_to(&repo.inner)?)
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
