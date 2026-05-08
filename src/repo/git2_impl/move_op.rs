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

//! Move a commit to a new position in the branch by cherry-picking the
//! reordered chain onto the appropriate base.

use anyhow::{Context, Result};

use super::super::RebaseOutcome;
use super::Git2Repo;
use super::cherry_pick::{CherryPickResult, build_chain_conflict, replace_root_and_replay};
use crate::Oid;

pub(super) fn move_commit(
    repo: &Git2Repo,
    commit_oid: &Oid,
    insert_after_oid: Option<&Oid>,
    head_oid: &Oid,
) -> Result<RebaseOutcome> {
    repo.check_no_dirty_state()?;

    let commit_git_oid = git2::Oid::from(commit_oid);
    let head_git_oid = git2::Oid::from(head_oid);

    let commit = repo.inner.find_commit(commit_git_oid)?;
    if commit.parent_count() > 1 {
        anyhow::bail!("Cannot move a merge commit");
    }

    let original_branch_oid = head_oid.clone();

    // Moving the root commit to a later position needs special handling
    // (merge_trees for the new root + conflict routing), so it returns
    // RebaseOutcome directly rather than going through the plan/chain pattern.
    if let Some(insert_after) = insert_after_oid
        && commit.parent_count() == 0
    {
        return move_root_to_later(
            repo,
            commit_git_oid,
            git2::Oid::from(insert_after),
            head_git_oid,
            original_branch_oid,
            commit_oid.clone(),
        );
    }

    // Determine the chain base and the ordered commit list to replay.
    //
    // `None` for `insert_after_oid` is a sentinel meaning "make the source the
    // new root commit" (used by --all mode when the user moves a commit
    // before the first visible entry).
    let (chain_base, reordered) = match insert_after_oid {
        None => plan_move_to_root(repo, commit_git_oid, head_git_oid)?,
        Some(insert_after) => plan_reorder(
            repo,
            commit_git_oid,
            git2::Oid::from(insert_after),
            head_git_oid,
        )?,
    };

    let result = repo.cherry_pick_chain(chain_base, &reordered)?;
    match result {
        CherryPickResult::Complete(tip) => {
            repo.advance_branch_ref(tip, "git-tailor: move commit")?;
            repo.checkout_head()?;
            Ok(RebaseOutcome::Complete)
        }
        CherryPickResult::Conflict {
            tip,
            conflicting_idx,
        } => Ok(build_chain_conflict(
            repo,
            tip,
            &reordered,
            conflicting_idx,
            "Move",
            original_branch_oid,
            Some(commit_oid.clone()),
        )),
    }
}

/// Build the chain for moving `commit_git_oid` to the root position.
///
/// The source commit's diff is applied to an empty tree to create a new orphan
/// root commit, and all other commits are left in their original order to be
/// cherry-picked on top.  Returns `(new_root_oid, remaining_oids)`.
fn plan_move_to_root(
    repo: &Git2Repo,
    commit_git_oid: git2::Oid,
    head_git_oid: git2::Oid,
) -> Result<(git2::Oid, Vec<git2::Oid>)> {
    let commit = repo.inner.find_commit(commit_git_oid)?;

    let mut revwalk = repo.inner.revwalk()?;
    revwalk.push(head_git_oid)?;
    let mut all_oids: Vec<git2::Oid> = revwalk
        .collect::<Result<Vec<_>, git2::Error>>()
        .context("Failed to walk commits for root-position move")?;
    all_oids.reverse(); // oldest first: [root, …, HEAD]

    let parent_tree = commit.parent(0)?.tree()?;
    let commit_tree = commit.tree()?;
    let diff = repo
        .inner
        .diff_tree_to_tree(Some(&parent_tree), Some(&commit_tree), None)?;

    let empty_tree_oid = repo.inner.treebuilder(None)?.write()?;
    let empty_tree = repo.inner.find_tree(empty_tree_oid)?;
    let mut new_idx = repo.inner.apply_to_tree(&empty_tree, &diff, None)?;
    if new_idx.has_conflicts() {
        anyhow::bail!("Unexpected conflict creating new root from moved commit");
    }
    let new_tree_oid = new_idx.write_tree_to(&repo.inner)?;
    let new_tree = repo.inner.find_tree(new_tree_oid)?;

    let new_root_oid = repo.inner.commit(
        None,
        &commit.author(),
        &commit.committer(),
        commit.message().unwrap_or(""),
        &new_tree,
        &[], // no parents — this becomes the new root
    )?;

    let remaining: Vec<git2::Oid> = all_oids
        .into_iter()
        .filter(|&oid| oid != commit_git_oid)
        .collect();

    Ok((new_root_oid, remaining))
}

/// Move the current root commit to a later position.
///
/// The source is the existing root (parent_count == 0). After removing it from
/// the ordered list and inserting it at the target position, the commit that
/// ends up first becomes the new orphan root — its tree is built via
/// `replace_root_and_replay` which strips the root's content and handles
/// conflicts.
fn move_root_to_later(
    repo: &Git2Repo,
    commit_git_oid: git2::Oid,
    insert_after_git_oid: git2::Oid,
    head_git_oid: git2::Oid,
    original_branch_oid: Oid,
    moved_commit_oid: Oid,
) -> Result<RebaseOutcome> {
    let mut revwalk = repo.inner.revwalk()?;
    revwalk.push(head_git_oid)?;
    let mut all_oids: Vec<git2::Oid> = revwalk
        .collect::<Result<Vec<_>, git2::Error>>()
        .context("Failed to walk commits for root-move")?;
    all_oids.reverse();

    let mut reordered: Vec<git2::Oid> = all_oids
        .into_iter()
        .filter(|&oid| oid != commit_git_oid)
        .collect();

    let insert_pos = reordered
        .iter()
        .position(|&oid| oid == insert_after_git_oid)
        .context("insert_after_oid not found among branch commits")?
        + 1;
    reordered.insert(insert_pos, commit_git_oid);

    let root_tree = repo.inner.find_commit(commit_git_oid)?.tree()?;
    let first_commit = repo.inner.find_commit(reordered[0])?;

    replace_root_and_replay(
        repo,
        &root_tree,
        &first_commit,
        &reordered[1..],
        "Move",
        original_branch_oid,
        Some(moved_commit_oid),
        "git-tailor: move root commit",
    )
}

/// Build the chain for a standard mid-chain reorder.
///
/// Finds the merge-base of `insert_after` and the source's parent, collects
/// all descendants from that base to HEAD, removes the source from its current
/// position, and inserts it after `insert_after`.
/// Returns `(chain_base_oid, reordered_oids)`.
fn plan_reorder(
    repo: &Git2Repo,
    commit_git_oid: git2::Oid,
    insert_after_git_oid: git2::Oid,
    head_git_oid: git2::Oid,
) -> Result<(git2::Oid, Vec<git2::Oid>)> {
    let commit = repo.inner.find_commit(commit_git_oid)?;
    let source_parent_oid = commit.parent_id(0)?;

    // The rebase base is the earlier of insert_after and source's parent.
    let base_oid = repo
        .inner
        .merge_base(insert_after_git_oid, source_parent_oid)?;

    let all_descendants = repo.collect_descendants(base_oid, head_git_oid)?;

    let mut reordered: Vec<git2::Oid> = all_descendants
        .iter()
        .filter(|&&oid| oid != commit_git_oid)
        .copied()
        .collect();

    let insert_pos = if insert_after_git_oid == base_oid {
        0
    } else {
        reordered
            .iter()
            .position(|&oid| oid == insert_after_git_oid)
            .context("insert_after_oid not found among branch commits")?
            + 1
    };
    reordered.insert(insert_pos, commit_git_oid);

    Ok((base_oid, reordered))
}
