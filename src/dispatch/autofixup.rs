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

// Side-effect handlers for the bulk Autofixup operation.

use anyhow::Result;
use git_tailor::app::{AppMode, AppState};
use git_tailor::repo::{GitRepo, RebaseOutcome};
use git_tailor::{CommitInfo, Oid};

use crate::dispatch::{
    LoopAction, PendingAutofixupSelection, edit_message_suspended, settle_autostash,
};
use crate::{autostash_save_or_bail, get_head_oid_or_continue};

/// Compute the autofixup plan from the already-loaded commit list (no git
/// call needed) and show the confirmation dialog, or report nothing to do.
pub(crate) fn handle_prepare_autofixup_confirm(
    git_repo: &mut impl GitRepo,
    app: &mut AppState,
) -> Result<LoopAction> {
    let head_oid = get_head_oid_or_continue!(git_repo, app);
    let pairs = git_tailor::autofixup::plan_autofixup(&app.list.commits);
    if pairs.is_empty() {
        app.set_success_message("Nothing to autofixup");
        return Ok(LoopAction::Proceed);
    }
    let reference_oid = app.reference_oid.clone();
    app.enter_autofixup_confirm(pairs, head_oid, reference_oid);
    Ok(LoopAction::Proceed)
}

/// Where the cursor should land after an autofixup batch removes `pairs`'
/// source commits from the list (scattered removals, unlike Drop/Squash/Move
/// which each remove or reposition exactly one commit — plain index
/// preservation would often land on an unrelated row). If the originally
/// selected commit was itself folded away, lands on the commit it was folded
/// into; otherwise stays on the same commit, index-adjusted for how many
/// earlier commits vanished. `None` when the selection wasn't a real commit
/// (a synthetic staged/unstaged row, unaffected by autofixup either way).
pub(crate) fn autofixup_target_selection_index(
    commits: &[CommitInfo],
    selection_index: usize,
    pairs: &[git_tailor::autofixup::AutofixupPair],
) -> Option<usize> {
    let selected_oid = commits.get(selection_index)?.oid.as_oid()?;
    let reference_oid = pairs
        .iter()
        .find(|p| &p.source_oid == selected_oid)
        .map(|p| &p.target_oid)
        .unwrap_or(selected_oid);
    let index_of = |oid: &Oid| commits.iter().position(|c| c.oid.as_oid() == Some(oid));
    let reference_index = index_of(reference_oid)?;
    let removed_before = pairs
        .iter()
        .filter(|p| index_of(&p.source_oid).is_some_and(|i| i < reference_index))
        .count();
    Some(reference_index.saturating_sub(removed_before))
}

/// Open `$EDITOR` on `template` (the target's message, with the sources being
/// folded into it commented out — see `autofixup::edit_template`) and store
/// the result back onto the still-open confirmation dialog as an override for
/// `target_summary`. Does not execute anything; the batch only runs once the
/// user confirms.
pub(crate) fn handle_prepare_autofixup_edit_message(
    git_repo: &impl GitRepo,
    app: &mut AppState,
    target_summary: String,
    template: String,
    terminal_guard: &mut crate::terminal_guard::TerminalGuard,
    kb_enhanced: bool,
) -> Result<LoopAction> {
    let editor_result = edit_message_suspended(git_repo, terminal_guard, kb_enhanced, &template);
    match editor_result {
        Ok(edited) => {
            let message = git_tailor::autofixup::strip_comment_lines(&edited);
            if let AppMode::AutofixupConfirm(pending) = &mut app.mode {
                if message.is_empty() {
                    pending.message_overrides.remove(&target_summary);
                } else {
                    pending
                        .message_overrides
                        .insert(target_summary, message + "\n");
                }
            }
        }
        Err(e) => app.set_error_message(format!("Editor error: {e:#}")),
    }
    Ok(LoopAction::Proceed)
}

pub(crate) fn handle_execute_autofixup(
    git_repo: &mut impl GitRepo,
    app: &mut AppState,
    pending: &mut PendingAutofixupSelection,
    head_oid: Oid,
    reference_oid: Oid,
    pairs: Vec<git_tailor::autofixup::AutofixupPair>,
    message_overrides: std::collections::HashMap<String, String>,
) -> Result<LoopAction> {
    let target_index =
        autofixup_target_selection_index(&app.list.commits, app.list.selection_index, &pairs);
    autostash_save_or_bail!(git_repo, app);
    match git_repo.autofixup(&head_oid, &reference_oid, &message_overrides) {
        Ok(RebaseOutcome::Complete) => Ok(settle_autostash(
            app,
            git_repo.autostash_restore(),
            "Autofixup",
            "Commits autofixed up",
            match target_index {
                Some(idx) => LoopAction::ReloadSelecting(idx),
                None => LoopAction::ReloadPreserving,
            },
        )),
        Ok(RebaseOutcome::Conflict(state)) => {
            // Carry the precomputed target index across the conflict — it
            // still describes where the cursor should land once the *whole*
            // batch eventually completes, however many rounds that takes.
            pending.set(target_index);
            app.enter_rebase_conflict(*state);
            Ok(LoopAction::Continue)
        }
        Err(e) => {
            let _ = git_repo.autostash_restore();
            app.set_error_message(format!("Autofixup failed: {e:#}"));
            Ok(LoopAction::Proceed)
        }
    }
}

/// Apply a pending autofixup cursor-restoration index (see
/// [`PendingAutofixupSelection`]) to the outcome of resolving one step of a
/// conflicted batch. Only ever swaps a bare `ReloadPreserving` (the batch truly
/// completed) for `ReloadSelecting`; a `Continue` (still resolving — another
/// conflict, or a stash conflict) leaves the pending index untouched for the
/// next round, and anything else clears it so a stale index can't leak into a
/// later, unrelated reload.
pub(crate) fn apply_pending_autofixup_selection(
    pending: &mut PendingAutofixupSelection,
    is_autofixup: bool,
    action: LoopAction,
) -> LoopAction {
    if !is_autofixup {
        return action;
    }
    match action {
        LoopAction::ReloadPreserving => match pending.take() {
            Some(idx) => LoopAction::ReloadSelecting(idx),
            None => LoopAction::ReloadPreserving,
        },
        LoopAction::Continue => action,
        other => {
            pending.clear();
            other
        }
    }
}
