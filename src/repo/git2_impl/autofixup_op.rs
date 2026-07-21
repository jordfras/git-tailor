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

//! Bulk autofixup (mirrors `git rebase --autosquash`): repeatedly match and
//! squash `fixup!`/`squash!`-prefixed commits into their targets, reusing the
//! single-squash primitive in `squash_op` as the building block. The whole
//! batch runs as one undoable operation: `original_branch_oid` on any
//! `ConflictState` this produces is always the tip before the *batch* started
//! (not the current step), so the trait-level `journaled()` wrapper records a
//! single undo entry once every pair has been applied, and `rebase_abort`
//! unwinds the whole batch rather than just the in-progress step.

use std::collections::HashMap;

use anyhow::Result;

use super::super::{AutofixupContext, ConflictState, RebaseOutcome};
use super::Git2Repo;
use super::{conflict, reads, squash_op};
use crate::Oid;
use crate::app::SquashMode;
use crate::autofixup::{self, AutofixupPair};

pub(super) fn autofixup(
    repo: &Git2Repo,
    head_oid: &Oid,
    reference_oid: &Oid,
    message_overrides: &HashMap<String, String>,
) -> Result<RebaseOutcome> {
    run_batch(
        repo,
        head_oid.clone(),
        head_oid,
        reference_oid,
        message_overrides,
    )
}

/// Resume an in-progress autofixup batch through a *descendant* conflict
/// (i.e. `state.squash_context` is `None` — the squash commit itself was
/// already created, and cherry-picking one of its descendants conflicted).
/// Finishes that step via the ordinary conflict-continuation logic, unaware
/// of autofixup, then keeps going through any remaining fixup/target pairs.
pub(super) fn continue_autofixup(repo: &Git2Repo, state: &ConflictState) -> Result<RebaseOutcome> {
    let ctx = state
        .autofixup_context
        .clone()
        .expect("continue_autofixup only called for an autofixup batch");
    let batch_original_oid = state.original_branch_oid.clone();
    continue_after_step(
        repo,
        conflict::rebase_continue(repo, state),
        &batch_original_oid,
        &ctx,
    )
}

/// Resume an in-progress autofixup batch through a *squash-time* conflict
/// (`state.squash_context` was `Some` — the source/target tree merge itself
/// conflicted). Finalizes that step via `squash_finalize`, then keeps going
/// through any remaining fixup/target pairs.
pub(super) fn continue_autofixup_after_squash_finalize(
    repo: &Git2Repo,
    squash_ctx: &super::super::SquashContext,
    message: &str,
    batch_original_oid: &Oid,
    autofixup_ctx: &AutofixupContext,
) -> Result<RebaseOutcome> {
    continue_after_step(
        repo,
        squash_op::squash_finalize(repo, squash_ctx, message, batch_original_oid),
        batch_original_oid,
        autofixup_ctx,
    )
}

/// Shared continuation: if the just-finished step completed, keep going
/// through the batch; if it conflicted again, re-tag the new conflict with
/// the batch's true original tip and context so it can be resumed the
/// same way.
fn continue_after_step(
    repo: &Git2Repo,
    step_outcome: Result<RebaseOutcome>,
    batch_original_oid: &Oid,
    ctx: &AutofixupContext,
) -> Result<RebaseOutcome> {
    match step_outcome? {
        RebaseOutcome::Complete => {
            let current_tip = reads::head_oid(repo)?;
            run_batch(
                repo,
                current_tip,
                batch_original_oid,
                &ctx.reference_oid,
                &ctx.message_overrides,
            )
        }
        RebaseOutcome::Conflict(new_state) => {
            Ok(RebaseOutcome::Conflict(Box::new(ConflictState {
                original_branch_oid: batch_original_oid.clone(),
                autofixup_context: Some(ctx.clone()),
                ..*new_state
            })))
        }
    }
}

fn run_batch(
    repo: &Git2Repo,
    mut current_tip: Oid,
    batch_original_oid: &Oid,
    reference_oid: &Oid,
    message_overrides: &HashMap<String, String>,
) -> Result<RebaseOutcome> {
    loop {
        let commits = reads::list_commits(repo, &current_tip, reference_oid)?;
        let plan = autofixup::plan_autofixup(&commits);
        let Some(pair) = plan.first() else {
            return Ok(RebaseOutcome::Complete);
        };
        let more_pending_for_target = plan[1..]
            .iter()
            .any(|p| p.target_summary == pair.target_summary);
        let message = pair_message(pair, more_pending_for_target, message_overrides);
        match squash_op::squash_commits(
            repo,
            &pair.source_oid,
            &pair.target_oid,
            &message,
            &current_tip,
        )? {
            RebaseOutcome::Complete => {
                current_tip = reads::head_oid(repo)?;
            }
            RebaseOutcome::Conflict(state) => {
                return Ok(RebaseOutcome::Conflict(Box::new(ConflictState {
                    original_branch_oid: batch_original_oid.clone(),
                    autofixup_context: Some(AutofixupContext {
                        reference_oid: reference_oid.clone(),
                        message_overrides: message_overrides.clone(),
                    }),
                    ..*state
                })));
            }
        }
    }
}

/// The commit message for one autofixup pair.
///
/// If the user pinned a final message for this target in the confirmation
/// dialog, it's used — but only once `more_pending_for_target` is `false`,
/// i.e. this is the last fixup/squash still queued for that target. Applying
/// it earlier would rename the target before the remaining pairs in the same
/// group get a chance to match it (matching is by summary text, since OIDs
/// churn with every squash in the batch).
///
/// Otherwise falls back to the default: `fixup!` keeps the target's message
/// unchanged; `squash!` combines target + source with the same default text
/// the manual squash editor starts from (`src/main.rs::handle_prepare_squash`).
fn pair_message(
    pair: &AutofixupPair,
    more_pending_for_target: bool,
    message_overrides: &HashMap<String, String>,
) -> String {
    if !more_pending_for_target
        && let Some(overridden) = message_overrides.get(&pair.target_summary)
    {
        return overridden.clone();
    }
    match pair.mode {
        SquashMode::Fixup => pair.target_message.clone(),
        SquashMode::Squash => format!("{}\n\n{}", pair.target_message, pair.source_message),
    }
}
