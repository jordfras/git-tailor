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
use super::cherry_pick::{ChainCtx, CherryPickResult, replace_root_and_replay};
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
    let ctx = ChainCtx {
        label: "Drop",
        original_branch_oid: &original_branch_oid,
        moved_commit_oid: None,
    };
    match repo.cherry_pick_chain(parent_oid, &descendants, &ctx)? {
        CherryPickResult::Complete(tip) => {
            repo.advance_branch_ref(tip, "git-tailor: drop commit")?;
            repo.checkout_head(&original_branch_oid)?;
            Ok(RebaseOutcome::Complete)
        }
        CherryPickResult::Conflict(state) => Ok(RebaseOutcome::Conflict(state)),
    }
}

/// Drop the root commit (parent_count == 0) by three-way merging the first
/// descendant onto the empty tree, then cherry-picking remaining descendants.
fn drop_root_commit(
    repo: &Git2Repo,
    commit_git_oid: git2::Oid,
    head_git_oid: git2::Oid,
    original_branch_oid: Oid,
) -> Result<RebaseOutcome> {
    let mut revwalk = repo.inner.revwalk()?;
    revwalk.push(head_git_oid)?;
    let mut all_oids: Vec<git2::Oid> = revwalk.collect::<Result<Vec<_>, git2::Error>>()?;
    all_oids.reverse();

    let descendants: Vec<git2::Oid> = all_oids
        .into_iter()
        .filter(|&oid| oid != commit_git_oid)
        .collect();

    if descendants.is_empty() {
        anyhow::bail!("Cannot drop the only commit on the branch");
    }

    let root_tree = repo.inner.find_commit(commit_git_oid)?.tree()?;
    let first = repo.inner.find_commit(descendants[0])?;

    replace_root_and_replay(
        repo,
        &root_tree,
        &first,
        &descendants[1..],
        "Drop",
        original_branch_oid,
        None,
        "git-tailor: drop root commit",
    )
}
