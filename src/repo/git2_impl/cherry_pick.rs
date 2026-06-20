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

//! Cherry-pick primitives shared by drop, move, squash, and conflict-resume
//! operations.

use anyhow::Result;

use super::super::{ConflictState, RebaseOutcome};
use super::Git2Repo;
use super::conflict;
use crate::Oid;

impl Git2Repo {
    /// Cherry-pick all commits strictly between `stop_oid` (exclusive) and
    /// `head_oid` (inclusive) onto `tip`, returning the new tip OID.
    pub(super) fn rebase_descendants(
        &self,
        stop_oid: git2::Oid,
        head_oid: git2::Oid,
        mut tip: git2::Oid,
    ) -> Result<git2::Oid> {
        let repo = &self.inner;
        if head_oid == stop_oid {
            return Ok(tip);
        }

        let mut revwalk = repo.revwalk()?;
        revwalk.push(head_oid)?;

        let mut descendants: Vec<git2::Oid> = Vec::new();
        for oid_result in revwalk {
            let oid = oid_result?;
            if oid == stop_oid {
                break;
            }
            descendants.push(oid);
        }
        descendants.reverse();

        for desc_oid in descendants {
            let desc_commit = repo.find_commit(desc_oid)?;
            let onto_commit = repo.find_commit(tip)?;

            let mut cherry_index = repo.cherrypick_commit(&desc_commit, &onto_commit, 0, None)?;
            if cherry_index.has_conflicts() {
                anyhow::bail!(
                    "Conflict rebasing {} onto split result",
                    &desc_oid.to_string()[..10]
                );
            }
            let new_tree_oid = cherry_index.write_tree_to(repo)?;
            let new_tree = repo.find_tree(new_tree_oid)?;

            let author = desc_commit.author();
            let committer = desc_commit.committer();
            tip = repo.commit(
                None,
                &author,
                &committer,
                desc_commit.message().unwrap_or(""),
                &new_tree,
                &[&onto_commit],
            )?;
        }

        Ok(tip)
    }

    /// Collect OIDs strictly between `stop_oid` (exclusive) and `head_oid`
    /// (inclusive), returned oldest-first.
    pub(super) fn collect_descendants(
        &self,
        stop_oid: git2::Oid,
        head_oid: git2::Oid,
    ) -> Result<Vec<git2::Oid>> {
        let repo = &self.inner;
        if head_oid == stop_oid {
            return Ok(Vec::new());
        }

        let mut revwalk = repo.revwalk()?;
        revwalk.push(head_oid)?;

        let mut descendants: Vec<git2::Oid> = Vec::new();
        for oid_result in revwalk {
            let oid = oid_result?;
            if oid == stop_oid {
                break;
            }
            descendants.push(oid);
        }
        descendants.reverse();
        Ok(descendants)
    }

    /// Cherry-pick a sequence of commits onto `tip`. Returns the final tip on
    /// success, or a `ConflictState` describing the first conflict.
    ///
    /// On conflict the operation is journaled and the conflicted index is
    /// written to the working tree — both handled by
    /// `write_conflicts_to_workdir`, which journals *before* mutating so a crash
    /// mid-operation is recoverable.
    pub(super) fn cherry_pick_chain(
        &self,
        mut tip: git2::Oid,
        commits: &[git2::Oid],
        ctx: &ChainCtx,
    ) -> Result<CherryPickResult> {
        let repo = &self.inner;

        for (idx, &desc_oid) in commits.iter().enumerate() {
            let desc_commit = repo.find_commit(desc_oid)?;
            let onto_commit = repo.find_commit(tip)?;

            // Root commits have no parent, so cherrypick_commit cannot compute
            // a diff base. Use merge_trees with the empty tree as the common
            // ancestor — this is the correct three-way merge equivalent of
            // cherry-picking a commit with no parent: files already present
            // with matching content are kept without conflict.
            let mut cherry_index = if desc_commit.parent_count() == 0 {
                let empty_tree_oid = repo.treebuilder(None)?.write()?;
                let empty_tree = repo.find_tree(empty_tree_oid)?;
                repo.merge_trees(
                    &empty_tree,
                    &onto_commit.tree()?,
                    &desc_commit.tree()?,
                    None,
                )?
            } else {
                repo.cherrypick_commit(&desc_commit, &onto_commit, 0, None)?
            };
            if cherry_index.has_conflicts() {
                let state = build_chain_conflict_state(&cherry_index, tip, commits, idx, ctx);
                conflict::write_conflicts_to_workdir(self, &cherry_index, &onto_commit, &state)?;
                return Ok(CherryPickResult::Conflict(Box::new(state)));
            }

            let new_tree_oid = cherry_index.write_tree_to(repo)?;
            let new_tree = repo.find_tree(new_tree_oid)?;

            tip = repo.commit(
                None,
                &desc_commit.author(),
                &desc_commit.committer(),
                desc_commit.message().unwrap_or(""),
                &new_tree,
                &[&onto_commit],
            )?;
        }

        Ok(CherryPickResult::Complete(tip))
    }
}

/// Operation context threaded into [`Git2Repo::cherry_pick_chain`] so it can
/// build (and journal) a `ConflictState` at the point a conflict is detected,
/// before any durable change is made.
pub(super) struct ChainCtx<'a> {
    pub label: &'a str,
    pub original_branch_oid: &'a Oid,
    pub moved_commit_oid: Option<&'a Oid>,
}

/// Internal result from `cherry_pick_chain`.
pub(super) enum CherryPickResult {
    Complete(git2::Oid),
    Conflict(Box<ConflictState>),
}

/// Build the `ConflictState` for a chain conflict, reading the conflicting
/// paths from the in-memory `cherry_index` (before it is written to disk).
fn build_chain_conflict_state(
    cherry_index: &git2::Index,
    tip: git2::Oid,
    oids: &[git2::Oid],
    conflicting_idx: usize,
    ctx: &ChainCtx,
) -> ConflictState {
    let conflicting_oid = oids[conflicting_idx];
    let remaining: Vec<Oid> = oids[conflicting_idx + 1..]
        .iter()
        .map(|&oid| Oid::from(oid))
        .collect();

    ConflictState {
        operation_label: ctx.label.to_string(),
        original_branch_oid: ctx.original_branch_oid.clone(),
        new_tip_oid: Oid::from(tip),
        conflicting_commit_oid: Oid::from(conflicting_oid),
        remaining_oids: remaining,
        conflicting_files: conflict::collect_conflict_files_from_index(cherry_index),
        still_unresolved: false,
        moved_commit_oid: ctx.moved_commit_oid.cloned(),
        squash_context: None,
        is_orphan_root: false,
    }
}

/// Strip `root_tree`'s content from `first_commit` via a three-way merge to
/// create a new orphan root, then cherry-pick `remaining` commits on top.
///
/// Used by both drop-root and move-root-to-later: the caller prepares the
/// ordered commit list and this function handles the merge_trees, conflict
/// routing (with `is_orphan_root: true`), orphan commit creation, and the
/// cherry-pick chain.
#[allow(clippy::too_many_arguments)]
pub(super) fn replace_root_and_replay(
    repo: &Git2Repo,
    root_tree: &git2::Tree,
    first_commit: &git2::Commit,
    remaining: &[git2::Oid],
    operation_label: &str,
    original_branch_oid: Oid,
    moved_commit_oid: Option<Oid>,
    reflog_msg: &str,
) -> Result<RebaseOutcome> {
    let empty_tree_oid = repo.inner.treebuilder(None)?.write()?;
    let empty_tree = repo.inner.find_tree(empty_tree_oid)?;

    let mut cherry_index =
        repo.inner
            .merge_trees(root_tree, &empty_tree, &first_commit.tree()?, None)?;

    if cherry_index.has_conflicts() {
        let sig = first_commit.author();
        let anchor_oid =
            repo.inner
                .commit(None, &sig, &first_commit.committer(), "", &empty_tree, &[])?;
        let anchor_commit = repo.inner.find_commit(anchor_oid)?;

        let remaining_oids: Vec<Oid> = remaining.iter().map(|&oid| Oid::from(oid)).collect();
        let state = ConflictState {
            operation_label: operation_label.to_string(),
            original_branch_oid,
            new_tip_oid: Oid::from(anchor_oid),
            conflicting_commit_oid: Oid::from(first_commit.id()),
            remaining_oids,
            conflicting_files: conflict::collect_conflict_files_from_index(&cherry_index),
            still_unresolved: false,
            moved_commit_oid,
            squash_context: None,
            is_orphan_root: true,
        };
        // Journals write-ahead, then mutates the ref/index/workdir.
        conflict::write_conflicts_to_workdir(repo, &cherry_index, &anchor_commit, &state)?;
        return Ok(RebaseOutcome::Conflict(Box::new(state)));
    }

    let new_tree_oid = cherry_index.write_tree_to(&repo.inner)?;
    let new_tree = repo.inner.find_tree(new_tree_oid)?;

    let new_root_oid = repo.inner.commit(
        None,
        &first_commit.author(),
        &first_commit.committer(),
        first_commit.message().unwrap_or(""),
        &new_tree,
        &[],
    )?;

    let ctx = ChainCtx {
        label: operation_label,
        original_branch_oid: &original_branch_oid,
        moved_commit_oid: moved_commit_oid.as_ref(),
    };
    match repo.cherry_pick_chain(new_root_oid, remaining, &ctx)? {
        CherryPickResult::Complete(tip) => {
            repo.advance_branch_ref(tip, reflog_msg)?;
            repo.checkout_head()?;
            Ok(RebaseOutcome::Complete)
        }
        CherryPickResult::Conflict(state) => Ok(RebaseOutcome::Conflict(state)),
    }
}
