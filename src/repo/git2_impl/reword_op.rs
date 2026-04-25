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

//! Reword a commit's message by rewriting it and replaying its descendants.

use anyhow::Result;

use super::Git2Repo;
use crate::Oid;

pub(super) fn reword_commit(
    repo: &Git2Repo,
    commit_oid: &Oid,
    new_message: &str,
    head_oid: &Oid,
) -> Result<()> {
    let commit_git_oid = git2::Oid::from(commit_oid);
    let head_git_oid = git2::Oid::from(head_oid);
    let commit = repo.inner.find_commit(commit_git_oid)?;

    let parents: Vec<git2::Commit> = (0..commit.parent_count())
        .map(|i| commit.parent(i))
        .collect::<std::result::Result<_, _>>()?;
    let parent_refs: Vec<&git2::Commit> = parents.iter().collect();

    let new_oid = repo.inner.commit(
        None,
        &commit.author(),
        &commit.committer(),
        new_message,
        &commit.tree()?,
        &parent_refs,
    )?;

    let tip = repo.rebase_descendants(commit_git_oid, head_git_oid, new_oid)?;
    repo.advance_branch_ref(tip, "reword: update branch ref")?;
    Ok(())
}
