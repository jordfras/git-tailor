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

use anyhow::{Context, Result};

use super::super::{ConflictState, RebaseOutcome};
use super::CherryPickResult;
use super::Git2Repo;
use super::conflict;

pub(super) fn drop_commit(
    repo: &Git2Repo,
    commit_oid: &str,
    head_oid: &str,
) -> Result<RebaseOutcome> {
    repo.check_no_dirty_state()?;

    let commit_git_oid = git2::Oid::from_str(commit_oid).context("Invalid commit OID for drop")?;
    let head_git_oid = git2::Oid::from_str(head_oid).context("Invalid HEAD OID for drop")?;
    let commit = repo.inner.find_commit(commit_git_oid)?;

    if commit.parent_count() != 1 {
        anyhow::bail!("Cannot drop a merge or root commit");
    }
    let parent_oid = commit.parent_id(0)?;

    let original_branch_oid = head_oid.to_string();

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
        } => {
            let conflicting_oid = descendants[conflicting_idx];
            let remaining: Vec<String> = descendants[conflicting_idx + 1..]
                .iter()
                .map(|oid| oid.to_string())
                .collect();

            Ok(RebaseOutcome::Conflict(Box::new(ConflictState {
                operation_label: "Drop".to_string(),
                original_branch_oid,
                new_tip_oid: tip.to_string(),
                conflicting_commit_oid: conflicting_oid.to_string(),
                remaining_oids: remaining,
                conflicting_files: conflict::collect_conflict_files(&repo.inner),
                still_unresolved: false,
                moved_commit_oid: None,
                squash_context: None,
            })))
        }
    }
}
