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

use super::Git2Repo;
use super::journal;
use crate::Oid;
use crate::repo::{WorktreeSource, WorktreeSourceCommit, WorktreeSourceSnapshot};

/// Message on the temporary commit. Only ever visible if git-tailor dies
/// between creating it and folding it away, where naming it plainly helps.
const TEMP_MESSAGE: &str = "git-tailor: working-tree changes";

/// Create the temporary commit for `source`, or `Ok(None)` when that row has no
/// changes. See [`super::Git2Repo::begin_worktree_source`] for the contract.
pub(super) fn begin(
    repo: &Git2Repo,
    source: WorktreeSource,
) -> Result<Option<WorktreeSourceCommit>> {
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

    let snapshot = WorktreeSourceSnapshot {
        source,
        tip_before: Oid::from(head.id()),
        index_tree_before: Oid::from(index_tree_before),
        worktree_tree: Oid::from(worktree_tree),
    };
    // Write-ahead: from here on there is a temporary commit to unwind, so the
    // record must already be on disk if the process dies mid-way.
    journal::set_worktree_source(repo, Some(snapshot.clone()))?;

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
    repo.advance_branch_ref(temp_oid, "git-tailor: working-tree squash source")?;

    // The unstaged row left its changes in the commit, so what remains staged is
    // the whole working tree relative to it.
    if source == WorktreeSource::Unstaged {
        set_index_tree(repo, worktree_tree)?;
    }

    Ok(Some(WorktreeSourceCommit {
        temp_oid: Oid::from(temp_oid),
        snapshot,
    }))
}

/// Unwind back to `snapshot`. See
/// [`super::Git2Repo::abort_worktree_source`] for the contract.
pub(super) fn abort(repo: &Git2Repo, snapshot: &WorktreeSourceSnapshot) -> Result<()> {
    // Captured before the ref moves: this is the tree the working tree reflects
    // right now, whether that is the temporary commit or a half-built rewrite.
    let current = head_tree_id(repo)?;
    repo.advance_branch_ref(
        git2::Oid::from(&snapshot.tip_before),
        "git-tailor: abort working-tree squash",
    )?;
    restore(
        repo,
        current,
        snapshot,
        git2::Oid::from(&snapshot.index_tree_before),
    )?;
    journal::set_worktree_source(repo, None)?;
    journal::clear_in_progress(repo)
}

/// Put the other row's changes back after the squash reached `tip_after`, and
/// report the resulting index tree so the undo record can restore it.
///
/// The working tree goes back to what it was — the operation only moved content
/// between committed, staged and unstaged, never changed it. What ends up in the
/// index differs by row: the staged row's changes are now committed, so the
/// index matches the new tip, while the unstaged row's are committed and the
/// staged ones stay staged, which is the whole working tree.
pub(super) fn finish(
    repo: &Git2Repo,
    snapshot: &WorktreeSourceSnapshot,
    tip_after: &Oid,
) -> Result<Oid> {
    let index_tree = match snapshot.source {
        WorktreeSource::Staged => repo
            .inner
            .find_commit(git2::Oid::from(tip_after))
            .context("failed to read the new branch tip")?
            .tree_id(),
        WorktreeSource::Unstaged => git2::Oid::from(&snapshot.worktree_tree),
    };
    restore(repo, head_tree_id(repo)?, snapshot, index_tree)?;
    Ok(Oid::from(index_tree))
}

/// Reset the working tree to `snapshot.worktree_tree` and the index to
/// `index_tree`. `current` is the tree the working tree reflects on entry.
fn restore(
    repo: &Git2Repo,
    current: git2::Oid,
    snapshot: &WorktreeSourceSnapshot,
    index_tree: git2::Oid,
) -> Result<()> {
    let worktree_tree = repo
        .inner
        .find_tree(git2::Oid::from(&snapshot.worktree_tree))
        .context("failed to find the recorded working tree")?;

    // A force checkout alone leaves files behind: once the index holds the
    // target tree, anything absent from it counts as untracked and is skipped.
    // Delete exactly the paths the target tree drops, as checkout_head does, so
    // the user's own untracked files survive.
    let current = repo
        .inner
        .find_tree(current)
        .context("failed to find the current tree")?;
    remove_dropped_files(repo, &current, &worktree_tree)?;

    set_index_tree(repo, worktree_tree.id())?;
    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.force();
    repo.inner
        .checkout_index(None, Some(&mut checkout))
        .context("failed to restore the working tree")?;
    set_index_tree(repo, index_tree)
}

/// Delete working-tree files present in `from` but absent from `to`.
fn remove_dropped_files(repo: &Git2Repo, from: &git2::Tree, to: &git2::Tree) -> Result<()> {
    let Some(workdir) = repo.inner.workdir() else {
        return Ok(());
    };
    let diff = repo
        .inner
        .diff_tree_to_tree(Some(from), Some(to), None)
        .context("failed to diff for dropped files")?;
    for delta in diff.deltas() {
        if delta.status() == git2::Delta::Deleted
            && let Some(path) = delta.old_file().path()
        {
            let full = workdir.join(path);
            if full.exists() {
                std::fs::remove_file(&full)
                    .with_context(|| format!("failed to remove {}", full.display()))?;
            }
        }
    }
    Ok(())
}

/// The index tree and the working tree (tracked paths only) as tree objects.
///
/// `update_all` mirrors `stage_all`: it only revisits paths already in the
/// index, so untracked files stay out of `W` exactly as they stay out of the
/// unstaged row's diff. The changes are never written to disk — the on-disk
/// index is reloaded afterwards.
fn snapshot_trees(repo: &Git2Repo) -> Result<(git2::Oid, git2::Oid)> {
    // Refresh the index against the working tree first. libgit2 decides which
    // tracked files are dirty from each entry's cached stat (size + mtime), so a
    // same-size edit whose mtime collides with that cache would be judged
    // unchanged and silently dropped from `W`. See `save_autostash`.
    {
        let mut opts = git2::StatusOptions::new();
        opts.include_untracked(true).update_index(true);
        repo.inner
            .statuses(Some(&mut opts))
            .context("failed to refresh the index")?;
    }

    let mut index = repo.inner.index().context("failed to open index")?;
    index.read(true).context("failed to refresh index")?;
    if index.has_conflicts() {
        anyhow::bail!("cannot squash working-tree changes while the index has conflicts");
    }
    let index_tree = index.write_tree().context("failed to write index tree")?;

    index
        .update_all(["*"].iter(), None)
        .context("failed to read working-tree changes")?;
    let worktree_tree = index
        .write_tree()
        .context("failed to write working-tree tree")?;
    index
        .read(true)
        .context("failed to restore index after snapshot")?;

    Ok((index_tree, worktree_tree))
}

/// HEAD's tree plus the unstaged delta and nothing else — the tree a commit of
/// just the unstaged row would have.
///
/// With nothing staged that is the working tree itself. Otherwise the unstaged
/// delta has to be lifted off the staged changes it sits on, which only works
/// when the two do not overlap.
fn unstaged_only_tree(
    repo: &Git2Repo,
    head_tree: &git2::Tree,
    index_tree: git2::Oid,
    worktree_tree: git2::Oid,
) -> Result<git2::Oid> {
    if index_tree == head_tree.id() {
        return Ok(worktree_tree);
    }

    let staged = repo.inner.find_tree(index_tree)?;
    let worktree = repo.inner.find_tree(worktree_tree)?;
    let unstaged_delta = repo
        .inner
        .diff_tree_to_tree(Some(&staged), Some(&worktree), None)
        .context("failed to diff the unstaged changes")?;
    let mut applied = repo
        .inner
        .apply_to_tree(head_tree, &unstaged_delta, None)
        .context(
            "the unstaged changes overlap your staged changes and cannot be \
             separated — commit or unstage the staged changes first",
        )?;
    applied
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

/// Point the on-disk index at `tree`, clearing any conflict stages.
fn set_index_tree(repo: &Git2Repo, tree: git2::Oid) -> Result<()> {
    let tree = repo
        .inner
        .find_tree(tree)
        .context("failed to find index tree")?;
    let mut index = repo.inner.index().context("failed to open index")?;
    index.read_tree(&tree).context("failed to set index tree")?;
    index.write().context("failed to write index")?;
    Ok(())
}
