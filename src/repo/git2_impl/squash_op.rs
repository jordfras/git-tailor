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

//! Squash a source commit into an earlier target commit, optionally as a
//! fixup. Supports the conflict-resolution flow via `try_combine` /
//! `finalize` so the user can hand-edit the merged tree before finishing.

use anyhow::{Context, Result};

use super::super::{ConflictState, RebaseOutcome, SquashContext};
use super::CherryPickResult;
use super::Git2Repo;
use super::conflict;

pub(super) fn squash_commits(
    repo: &Git2Repo,
    source_oid: &str,
    target_oid: &str,
    message: &str,
    head_oid: &str,
) -> Result<RebaseOutcome> {
    repo.check_no_dirty_state()?;

    let inputs = parse_squash_inputs(repo, source_oid, target_oid, head_oid)?;

    let mut cherry_index =
        repo.inner
            .cherrypick_commit(&inputs.source_commit, &inputs.target_commit, 0, None)?;
    if cherry_index.has_conflicts() {
        return Ok(RebaseOutcome::Conflict(Box::new(build_conflict_state(
            repo,
            &cherry_index,
            &inputs,
            message,
            false,
        )?)));
    }

    let base_commit = repo.inner.find_commit(inputs.base_oid)?;
    let combined_tree_oid = cherry_index.write_tree_to(&repo.inner)?;
    let combined_tree = repo.inner.find_tree(combined_tree_oid)?;

    let squash_oid = repo.inner.commit(
        None,
        &inputs.target_commit.author(),
        &inputs.target_commit.committer(),
        message,
        &combined_tree,
        &[&base_commit],
    )?;

    let all_descendants =
        repo.collect_descendants(inputs.target_commit.id(), inputs.head_git_oid)?;
    let descendants: Vec<git2::Oid> = all_descendants
        .into_iter()
        .filter(|&oid| oid != inputs.source_commit.id())
        .collect();

    replay_and_advance(
        repo,
        squash_oid,
        &descendants,
        head_oid.to_string(),
        "git-tailor: squash commits",
    )
}

pub(super) fn squash_try_combine(
    repo: &Git2Repo,
    source_oid: &str,
    target_oid: &str,
    combined_message: &str,
    is_fixup: bool,
    head_oid: &str,
) -> Result<Option<ConflictState>> {
    repo.check_no_dirty_state()?;

    let inputs = parse_squash_inputs(repo, source_oid, target_oid, head_oid)?;

    let cherry_index =
        repo.inner
            .cherrypick_commit(&inputs.source_commit, &inputs.target_commit, 0, None)?;
    if !cherry_index.has_conflicts() {
        return Ok(None);
    }

    Ok(Some(build_conflict_state(
        repo,
        &cherry_index,
        &inputs,
        combined_message,
        is_fixup,
    )?))
}

pub(super) fn squash_finalize(
    repo: &Git2Repo,
    ctx: &SquashContext,
    message: &str,
    original_branch_oid: &str,
) -> Result<RebaseOutcome> {
    let mut index = repo.inner.index()?;
    index.read(true)?;
    if index.has_conflicts() {
        anyhow::bail!("Cannot finalize squash: index still has unresolved conflicts");
    }

    let base_git_oid =
        git2::Oid::from_str(&ctx.base_oid).context("Invalid base OID in squash context")?;
    let target_git_oid =
        git2::Oid::from_str(&ctx.target_oid).context("Invalid target OID in squash context")?;
    let base_commit = repo.inner.find_commit(base_git_oid)?;
    let target_commit = repo.inner.find_commit(target_git_oid)?;

    let combined_tree_oid = index.write_tree()?;
    let combined_tree = repo.inner.find_tree(combined_tree_oid)?;

    let squash_oid = repo.inner.commit(
        None,
        &target_commit.author(),
        &target_commit.committer(),
        message,
        &combined_tree,
        &[&base_commit],
    )?;

    let descendants: Vec<git2::Oid> = ctx
        .descendant_oids
        .iter()
        .map(|s| git2::Oid::from_str(s))
        .collect::<std::result::Result<_, _>>()
        .context("Invalid OID in descendant list")?;

    replay_and_advance(
        repo,
        squash_oid,
        &descendants,
        original_branch_oid.to_string(),
        "git-tailor: squash commits (finalize)",
    )
}

/// Resolved inputs to a squash operation: parsed commits and OIDs alongside
/// the original `&str` arguments (kept verbatim so the resulting commit IDs
/// in `ConflictState` match what the caller passed in).
struct SquashInputs<'a> {
    source_oid: &'a str,
    target_oid: &'a str,
    head_oid: &'a str,
    source_commit: git2::Commit<'a>,
    target_commit: git2::Commit<'a>,
    base_oid: git2::Oid,
    head_git_oid: git2::Oid,
}

fn parse_squash_inputs<'a>(
    repo: &'a Git2Repo,
    source_oid: &'a str,
    target_oid: &'a str,
    head_oid: &'a str,
) -> Result<SquashInputs<'a>> {
    let source_git_oid =
        git2::Oid::from_str(source_oid).context("Invalid source OID for squash")?;
    let target_git_oid =
        git2::Oid::from_str(target_oid).context("Invalid target OID for squash")?;
    let head_git_oid = git2::Oid::from_str(head_oid).context("Invalid HEAD OID for squash")?;

    let source_commit = repo.inner.find_commit(source_git_oid)?;
    let target_commit = repo.inner.find_commit(target_git_oid)?;

    if target_commit.parent_count() != 1 {
        anyhow::bail!("Cannot squash into a merge or root commit");
    }
    let base_oid = target_commit.parent_id(0)?;

    Ok(SquashInputs {
        source_oid,
        target_oid,
        head_oid,
        source_commit,
        target_commit,
        base_oid,
        head_git_oid,
    })
}

/// Persist a conflicted cherry-pick to the working tree and build the
/// `ConflictState` describing it for the user-facing resolution flow.
fn build_conflict_state(
    repo: &Git2Repo,
    cherry_index: &git2::Index,
    inputs: &SquashInputs<'_>,
    combined_message: &str,
    is_fixup: bool,
) -> Result<ConflictState> {
    let all_descendants =
        repo.collect_descendants(inputs.target_commit.id(), inputs.head_git_oid)?;
    let descendant_oids: Vec<String> = all_descendants
        .into_iter()
        .filter(|&oid| oid != inputs.source_commit.id())
        .map(|oid| oid.to_string())
        .collect();

    conflict::write_conflicts_to_workdir(repo, cherry_index, &inputs.target_commit)?;

    let operation_label = if is_fixup { "Fixup" } else { "Squash" }.to_string();

    Ok(ConflictState {
        operation_label,
        original_branch_oid: inputs.head_oid.to_string(),
        new_tip_oid: inputs.target_commit.id().to_string(),
        conflicting_commit_oid: inputs.source_commit.id().to_string(),
        remaining_oids: vec![],
        conflicting_files: conflict::collect_conflict_files(&repo.inner),
        still_unresolved: false,
        moved_commit_oid: None,
        squash_context: Some(SquashContext {
            base_oid: inputs.base_oid.to_string(),
            source_oid: inputs.source_oid.to_string(),
            target_oid: inputs.target_oid.to_string(),
            combined_message: combined_message.to_string(),
            descendant_oids,
            is_fixup,
        }),
    })
}

/// Cherry-pick descendants onto the new squash commit and either advance
/// the branch ref (success) or report the next conflict.
fn replay_and_advance(
    repo: &Git2Repo,
    squash_oid: git2::Oid,
    descendants: &[git2::Oid],
    original_branch_oid: String,
    advance_msg: &str,
) -> Result<RebaseOutcome> {
    match repo.cherry_pick_chain(squash_oid, descendants)? {
        CherryPickResult::Complete(tip) => {
            repo.advance_branch_ref(tip, advance_msg)?;
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
                operation_label: "Squash".to_string(),
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
