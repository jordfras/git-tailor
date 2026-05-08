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

//! Drop a commit by cherry-picking its descendants onto its parent.

use anyhow::Result;

use super::super::RebaseOutcome;
use super::Git2Repo;
use super::cherry_pick::{CherryPickResult, build_chain_conflict};
use super::conflict;
use crate::Oid;

pub(super) fn drop_commit(
    repo: &Git2Repo,
    commit_oid: &Oid,
    head_oid: &Oid,
) -> Result<RebaseOutcome> {
    repo.check_no_dirty_state()?;

    let commit_git_oid = git2::Oid::from(commit_oid);
    let head_git_oid = git2::Oid::from(head_oid);
    let commit = repo.inner.find_commit(commit_git_oid)?;

    if commit.parent_count() > 1 {
        anyhow::bail!("Cannot drop a merge commit");
    }

    let original_branch_oid = head_oid.clone();

    if commit.parent_count() == 0 {
        return drop_root_commit(repo, commit_git_oid, head_git_oid, original_branch_oid);
    }

    let parent_oid = commit.parent_id(0)?;

    // Collect descendants: commits strictly between commit_oid and head_oid.
    let descendants = repo.collect_descendants(commit_git_oid, head_git_oid)?;

    // Cherry-pick each descendant onto the new chain, starting from the
    // dropped commit's parent.
    let result = repo.cherry_pick_chain(parent_oid, &descendants)?;
    match result {
        CherryPickResult::Complete(tip) => {
            repo.advance_branch_ref(tip, "git-tailor: drop commit")?;
            repo.checkout_head()?;
            Ok(RebaseOutcome::Complete)
        }
        CherryPickResult::Conflict {
            tip,
            conflicting_idx,
        } => Ok(build_chain_conflict(
            repo,
            tip,
            &descendants,
            conflicting_idx,
            "Drop",
            original_branch_oid,
            None,
        )),
    }
}

/// Drop the root commit (parent_count == 0) by three-way merging the first
/// descendant onto the empty tree, then cherry-picking remaining descendants.
///
/// The merge uses `ancestor=root_tree, ours=empty_tree, theirs=descendant_tree`
/// which correctly detects delete/modify conflicts for files the root created
/// that descendants modified.
fn drop_root_commit(
    repo: &Git2Repo,
    commit_git_oid: git2::Oid,
    head_git_oid: git2::Oid,
    original_branch_oid: Oid,
) -> Result<RebaseOutcome> {
    let mut revwalk = repo.inner.revwalk()?;
    revwalk.push(head_git_oid)?;
    let mut all_oids: Vec<git2::Oid> = revwalk.collect::<Result<Vec<_>, git2::Error>>()?;
    all_oids.reverse(); // oldest first

    let descendants: Vec<git2::Oid> = all_oids
        .into_iter()
        .filter(|&oid| oid != commit_git_oid)
        .collect();

    if descendants.is_empty() {
        anyhow::bail!("Cannot drop the only commit on the branch");
    }

    let root_tree = repo.inner.find_commit(commit_git_oid)?.tree()?;
    let empty_tree_oid = repo.inner.treebuilder(None)?.write()?;
    let empty_tree = repo.inner.find_tree(empty_tree_oid)?;
    let first = repo.inner.find_commit(descendants[0])?;

    let mut cherry_index = repo
        .inner
        .merge_trees(&root_tree, &empty_tree, &first.tree()?, None)?;

    if cherry_index.has_conflicts() {
        // Create a temporary empty-tree orphan commit as the "tip" anchor.
        // rebase_continue will create the real orphan root from the resolved
        // index (is_orphan_root flag).
        let sig = first.author();
        let anchor_oid = repo
            .inner
            .commit(None, &sig, &first.committer(), "", &empty_tree, &[])?;
        let anchor_commit = repo.inner.find_commit(anchor_oid)?;
        conflict::write_conflicts_to_workdir(repo, &cherry_index, &anchor_commit)?;

        let remaining: Vec<Oid> = descendants[1..].iter().map(|&oid| Oid::from(oid)).collect();

        return Ok(RebaseOutcome::Conflict(Box::new(
            super::super::ConflictState {
                operation_label: "Drop".to_string(),
                original_branch_oid,
                new_tip_oid: Oid::from(anchor_oid),
                conflicting_commit_oid: Oid::from(descendants[0]),
                remaining_oids: remaining,
                conflicting_files: conflict::collect_conflict_files(&repo.inner),
                still_unresolved: false,
                moved_commit_oid: None,
                squash_context: None,
                is_orphan_root: true,
            },
        )));
    }

    let new_tree_oid = cherry_index.write_tree_to(&repo.inner)?;
    let new_tree = repo.inner.find_tree(new_tree_oid)?;

    let new_root_oid = repo.inner.commit(
        None,
        &first.author(),
        &first.committer(),
        first.message().unwrap_or(""),
        &new_tree,
        &[], // orphan — no parents
    )?;

    let remaining = &descendants[1..];
    let result = repo.cherry_pick_chain(new_root_oid, remaining)?;
    match result {
        CherryPickResult::Complete(tip) => {
            repo.advance_branch_ref(tip, "git-tailor: drop root commit")?;
            repo.checkout_head()?;
            Ok(RebaseOutcome::Complete)
        }
        CherryPickResult::Conflict {
            tip,
            conflicting_idx,
        } => Ok(build_chain_conflict(
            repo,
            tip,
            remaining,
            conflicting_idx,
            "Drop",
            original_branch_oid,
            None,
        )),
    }
}
