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

use super::super::{ConflictState, RebaseOutcome};
use super::CherryPickResult;
use super::Git2Repo;
use super::conflict;

pub(super) fn move_commit(
    repo: &Git2Repo,
    commit_oid: &str,
    insert_after_oid: &str,
    head_oid: &str,
) -> Result<RebaseOutcome> {
    repo.check_no_dirty_state()?;

    let commit_git_oid = git2::Oid::from_str(commit_oid).context("Invalid commit OID for move")?;
    let head_git_oid = git2::Oid::from_str(head_oid).context("Invalid HEAD OID for move")?;

    let commit = repo.inner.find_commit(commit_git_oid)?;
    if commit.parent_count() > 1 {
        anyhow::bail!("Cannot move a merge commit");
    }

    let original_branch_oid = head_oid.to_string();

    // Determine the chain base and the ordered commit list to replay.
    //
    // Empty `insert_after_oid` is a sentinel meaning "make the source the
    // new root commit" (used by --all mode when the user moves a commit
    // before the first visible entry).  In that case the source commit's
    // diff is applied to an empty tree to create the new orphan root, and
    // all other commits are cherry-picked on top in their original order.
    let (chain_base, reordered) = if insert_after_oid.is_empty() {
        // Collect every commit from root to HEAD oldest-first.
        let mut revwalk = repo.inner.revwalk()?;
        revwalk.push(head_git_oid)?;
        let mut all_oids: Vec<git2::Oid> = revwalk
            .collect::<Result<Vec<_>, git2::Error>>()
            .context("Failed to walk commits for root-position move")?;
        all_oids.reverse(); // oldest first: [root, …, HEAD]

        // Build the new root commit from the source commit's own diff
        // applied to an empty tree.
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

        (new_root_oid, remaining)
    } else if commit.parent_count() == 0 {
        // Source IS the root commit; move it to a later position.
        // The new first commit in the reordered list becomes the orphan
        // root (its full tree is used as-is), then the rest (including
        // the original root) are cherry-picked on top.
        let insert_after_git_oid =
            git2::Oid::from_str(insert_after_oid).context("Invalid insert-after OID for move")?;

        let mut revwalk = repo.inner.revwalk()?;
        revwalk.push(head_git_oid)?;
        let mut all_oids: Vec<git2::Oid> = revwalk
            .collect::<Result<Vec<_>, git2::Error>>()
            .context("Failed to walk commits for root-move")?;
        all_oids.reverse(); // oldest first: [root, …, HEAD]

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

        // Commit the new first entry as an orphan root using its tree.
        let first_commit = repo.inner.find_commit(reordered[0])?;
        let new_root_oid = repo.inner.commit(
            None,
            &first_commit.author(),
            &first_commit.committer(),
            first_commit.message().unwrap_or(""),
            &first_commit.tree()?,
            &[],
        )?;

        (new_root_oid, reordered[1..].to_vec())
    } else {
        let insert_after_git_oid =
            git2::Oid::from_str(insert_after_oid).context("Invalid insert-after OID for move")?;
        let source_parent_oid = commit.parent_id(0)?;

        // The rebase base is the earlier of insert_after and source's parent.
        let base_oid = repo
            .inner
            .merge_base(insert_after_git_oid, source_parent_oid)?;

        // Collect all commits between base (exclusive) and HEAD (inclusive).
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

        (base_oid, reordered)
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
        } => {
            let conflicting_oid = reordered[conflicting_idx];
            let remaining: Vec<String> = reordered[conflicting_idx + 1..]
                .iter()
                .map(|oid| oid.to_string())
                .collect();

            Ok(RebaseOutcome::Conflict(Box::new(ConflictState {
                operation_label: "Move".to_string(),
                original_branch_oid,
                new_tip_oid: tip.to_string(),
                conflicting_commit_oid: conflicting_oid.to_string(),
                remaining_oids: remaining,
                conflicting_files: conflict::collect_conflict_files(&repo.inner),
                still_unresolved: false,
                moved_commit_oid: Some(commit_oid.to_string()),
                squash_context: None,
            })))
        }
    }
}
