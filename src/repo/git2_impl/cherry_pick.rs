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

    /// Cherry-pick a sequence of commits onto `tip`, returning the final tip
    /// or the point at which a conflict was detected.
    ///
    /// On conflict the conflicted index is written to the working tree so the
    /// user can resolve it. The returned `conflicting_idx` identifies which
    /// element of `commits` conflicted.
    pub(super) fn cherry_pick_chain(
        &self,
        mut tip: git2::Oid,
        commits: &[git2::Oid],
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
                conflict::write_conflicts_to_workdir(self, &cherry_index, &onto_commit)?;
                return Ok(CherryPickResult::Conflict {
                    tip,
                    conflicting_idx: idx,
                });
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

/// Internal result from `cherry_pick_chain`.
pub(super) enum CherryPickResult {
    Complete(git2::Oid),
    Conflict {
        tip: git2::Oid,
        conflicting_idx: usize,
    },
}

/// Build a `RebaseOutcome::Conflict` from the output of a failed
/// `cherry_pick_chain` call.
///
/// All four conflict-reporting sites share the same shape: extract the
/// conflicting OID and the remaining-OIDs tail from the `oids` slice using
/// `conflicting_idx`, then collect conflict files from the working-tree index.
/// Centralising this keeps future `ConflictState` field additions in one place.
pub(super) fn build_chain_conflict(
    repo: &Git2Repo,
    tip: git2::Oid,
    oids: &[git2::Oid],
    conflicting_idx: usize,
    operation_label: impl Into<String>,
    original_branch_oid: Oid,
    moved_commit_oid: Option<Oid>,
) -> RebaseOutcome {
    let conflicting_oid = oids[conflicting_idx];
    let remaining: Vec<Oid> = oids[conflicting_idx + 1..]
        .iter()
        .map(|&oid| Oid::from(oid))
        .collect();

    RebaseOutcome::Conflict(Box::new(ConflictState {
        operation_label: operation_label.into(),
        original_branch_oid,
        new_tip_oid: Oid::from(tip),
        conflicting_commit_oid: Oid::from(conflicting_oid),
        remaining_oids: remaining,
        conflicting_files: conflict::collect_conflict_files(&repo.inner),
        still_unresolved: false,
        moved_commit_oid,
        squash_context: None,
    }))
}
