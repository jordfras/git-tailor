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

//! Bulk autofixup (mirrors `git rebase --autosquash`): squash each
//! `fixup!`/`squash!`-prefixed commit into its target, one step at a time,
//! following the [`BatchPlan`] made when the batch starts and reusing the
//! single-squash primitive in `squash_op` for each step. The whole batch runs
//! as one undoable operation: `original_branch_oid` on any
//! `ConflictState` this produces is always the tip before the *batch* started
//! (not the current step), so the trait-level `journaled()` wrapper
//! records a single undo entry for the whole batch — once every pair has been
//! applied, or as soon as an error stops it with some landed — and
//! `rebase_abort` unwinds the whole batch rather than just the in-progress step.

use anyhow::Result;
use bstr::{BStr, BString, ByteSlice};

use super::super::{AutofixupContext, ConflictState, RebaseOutcome, RepoRead};
use super::Git2Repo;
use super::{conflict, reads, squash_op};
use crate::Oid;
use crate::app::SquashMode;
use crate::autofixup::{AutofixupPair, BatchPlan, MessageOverrides, PlannedPos};

/// What the batch is called in its undo entry, its conflicts and its reflog.
pub(super) const LABEL: &str = "Autofixup";

pub(super) fn autofixup(
    repo: &mut Git2Repo,
    head_oid: &Oid,
    reference_oid: &Oid,
    pairs: &[AutofixupPair],
    message_overrides: &MessageOverrides,
) -> Result<RebaseOutcome> {
    let plan = BatchPlan::new(reads::list_oids(repo, head_oid, reference_oid)?, pairs)?;
    // The pairs are squashed one at a time, so a refusal part-way would stop
    // the batch with the earlier ones landed. Every pair rewrites from its
    // target up, so the oldest target covers the whole batch.
    if let Some(oldest) = plan.steps.iter().map(|step| step.target.0).min() {
        let oldest = plan.planned_oid(PlannedPos(oldest));
        repo.refuse_rewriting_from(oldest, head_oid)?;
        // A step finds its commits by their place in the branch, which only a
        // single line of history defines.
        if repo.range_has_merge(Some(git2::Oid::from(oldest)), git2::Oid::from(head_oid))? {
            anyhow::bail!("Cannot autofixup: a merge commit lies between a target and HEAD");
        }
    }
    let ctx = AutofixupContext {
        reference_oid: reference_oid.clone(),
        message_overrides: message_overrides.clone(),
        plan: Some(plan),
        landed: 0,
    };
    run_batch(repo, head_oid, ctx)
}

/// Resume an in-progress autofixup batch through a *descendant* conflict
/// (i.e. `state.squash_context` is `None` — the squash commit itself was
/// already created, and cherry-picking one of its descendants conflicted).
/// Finishes that step via the ordinary conflict-continuation logic, unaware
/// of autofixup, then keeps going through any remaining fixup/target pairs.
pub(super) fn continue_autofixup(
    repo: &mut Git2Repo,
    state: &ConflictState,
) -> Result<RebaseOutcome> {
    let ctx = state
        .autofixup_context
        .clone()
        .expect("continue_autofixup only called for an autofixup batch");
    plan_of(&ctx)?;
    let batch_original_oid = state.original_branch_oid.clone();
    let step = conflict::rebase_continue(repo, state);
    continue_after_step(repo, step, &batch_original_oid, &ctx)
}

/// Resume an in-progress autofixup batch through a *squash-time* conflict
/// (`state.squash_context` was `Some` — the source/target tree merge itself
/// conflicted). Finalizes that step via `squash_finalize`, then keeps going
/// through any remaining fixup/target pairs.
pub(super) fn continue_autofixup_after_squash_finalize(
    repo: &mut Git2Repo,
    squash_ctx: &super::super::SquashContext,
    message: &BStr,
    batch_original_oid: &Oid,
    autofixup_ctx: &AutofixupContext,
) -> Result<RebaseOutcome> {
    plan_of(autofixup_ctx)?;
    let step = squash_op::squash_finalize(repo, squash_ctx, message, batch_original_oid);
    continue_after_step(repo, step, batch_original_oid, autofixup_ctx)
}

/// The plan the batch was started with. A context without one cannot say how
/// far its batch got, so resuming it is refused — before the paused step is
/// finished, so aborting it still puts the branch back where it started.
fn plan_of(ctx: &AutofixupContext) -> Result<&BatchPlan> {
    ctx.plan.as_ref().ok_or_else(|| {
        anyhow::anyhow!(
            "This autofixup was paused by an older git-tailor and cannot be \
             resumed here. Abort it, then run the autofixup again."
        )
    })
}

/// Shared continuation: if the just-finished step completed, keep going
/// through the batch; if it conflicted again, re-tag the new conflict with
/// the batch's true original tip and context so it can be resumed the
/// same way.
fn continue_after_step(
    repo: &mut Git2Repo,
    step_outcome: Result<RebaseOutcome>,
    batch_original_oid: &Oid,
    ctx: &AutofixupContext,
) -> Result<RebaseOutcome> {
    match step_outcome? {
        RebaseOutcome::Complete => run_batch(repo, batch_original_oid, ctx.clone()),
        RebaseOutcome::Conflict(new_state) => {
            Ok(RebaseOutcome::Conflict(Box::new(ConflictState {
                operation_label: LABEL.to_string(),
                original_branch_oid: batch_original_oid.clone(),
                autofixup_context: Some(ctx.clone()),
                ..*new_state
            })))
        }
    }
}

/// Apply `ctx.plan`'s steps from `ctx.landed` on, pausing on a conflict with
/// the context that resumes the batch where it stopped.
fn run_batch(
    repo: &mut Git2Repo,
    batch_original_oid: &Oid,
    mut ctx: AutofixupContext,
) -> Result<RebaseOutcome> {
    let plan = plan_of(&ctx)?.clone();
    while ctx.landed < plan.steps.len() {
        let current_tip = reads::head_oid(repo)?;
        let current = reads::list_oids(repo, &current_tip, &ctx.reference_oid)?;
        if current.len() + ctx.landed != plan.commits.len() {
            anyhow::bail!("The branch no longer matches the autofixup that was planned for it.");
        }
        let step = &plan.steps[ctx.landed];
        let source_oid = plan.current_oid(step.source, ctx.landed, &current).clone();
        let target_oid = plan.current_oid(step.target, ctx.landed, &current).clone();
        let more_pending_for_target = plan.steps[ctx.landed + 1..]
            .iter()
            .any(|later| later.target == step.target);
        let overridden = if more_pending_for_target {
            None
        } else {
            ctx.message_overrides
                .for_target(plan.planned_oid(step.target))
                .cloned()
        };
        let message = match overridden {
            Some(message) => message,
            None => step_message(repo, &source_oid, &target_oid, step.mode)?,
        };
        let outcome = squash_op::squash_commits(
            repo,
            &source_oid,
            &target_oid,
            message.as_bstr(),
            &current_tip,
        )?;
        ctx.landed += 1;
        if let RebaseOutcome::Conflict(state) = outcome {
            return Ok(RebaseOutcome::Conflict(Box::new(ConflictState {
                operation_label: LABEL.to_string(),
                original_branch_oid: batch_original_oid.clone(),
                autofixup_context: Some(ctx),
                ..*state
            })));
        }
    }
    Ok(RebaseOutcome::Complete)
}

/// The default message for one step: `fixup!` keeps the target's message
/// unchanged; `squash!` combines target + source with the same default text
/// the manual squash editor starts from (`src/main.rs::handle_prepare_squash`).
///
/// Read from the repository, not from the commit list: that holds the lossy
/// display rendering, and writing it back would replace a message git-tailor
/// cannot read with one it can.
fn step_message(
    repo: &Git2Repo,
    source_oid: &Oid,
    target_oid: &Oid,
    mode: SquashMode,
) -> Result<BString> {
    let target_bytes = repo.commit_message_bytes(target_oid)?;
    match mode {
        SquashMode::Fixup => Ok(target_bytes),
        SquashMode::Squash => {
            let source_bytes = repo.commit_message_bytes(source_oid)?;
            Ok(crate::domain::combine_messages(
                target_bytes.as_bstr(),
                Some(source_bytes.as_bstr()),
            ))
        }
    }
}
