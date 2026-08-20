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

// Action dispatch: turn the [`AppAction`] a view module requested into the git
// side effect that carries it out. Each operation's handler lives in a
// per-operation submodule; the shared control-flow helpers live here.

mod autofixup;
mod conflict;
mod edit;
mod rewrite;
mod split;
mod staging;
mod undo;

#[cfg(test)]
mod tests;

use anyhow::Result;
use git_tailor::app::{AppAction, AppState};
use git_tailor::editor;
use git_tailor::repo::{
    AutostashRestore, GitRepo, RebaseOutcome, StageOutcome, StashConflictState,
};
use git_tailor::views;

use crate::external_tool::with_tui_suspended;

use autofixup::{
    handle_execute_autofixup, handle_prepare_autofixup_confirm,
    handle_prepare_autofixup_edit_message,
};
use conflict::{
    handle_autostash_abort, handle_autostash_continue, handle_rebase_abort, handle_rebase_continue,
    handle_run_conflict_tool, handle_run_stash_tool, run_editor_suspended, run_mergetool_suspended,
};
use edit::handle_execute_edit;
use rewrite::{
    handle_execute_drop, handle_execute_move, handle_prepare_reword, handle_prepare_squash,
};
use split::{
    execute_split, handle_execute_split_out_files, handle_execute_split_out_hunks,
    handle_prepare_split, handle_prepare_split_out_files, handle_prepare_split_out_hunks,
};
use staging::{handle_commit_staged, handle_stage_all, handle_unstage_all};
use undo::{handle_redo, handle_undo};

/// Loop-control signal returned by [`dispatch_action`].
pub(crate) enum LoopAction {
    /// Skip the rest of this iteration (equivalent to `continue` in the loop).
    Continue,
    /// Fall through to the post-dispatch checks (e.g. `should_quit`).
    Proceed,
    /// Reload commits from HEAD, preserving the current selection index.
    ReloadPreserving,
    /// Reload commits from HEAD, resetting selection to the natural position.
    Reload,
    /// Reload commits from HEAD, selecting a specific (clamped) index —
    /// for operations where "the same index" isn't a good proxy for "the
    /// same commit" (e.g. autofixup, which can remove several commits
    /// scattered through the list in one batch).
    ReloadSelecting(usize),
}

/// Precomputed target selection index for an in-progress autofixup batch that
/// hit a conflict, carried across the conflict-resolution round trip (possibly
/// several, if more than one pair conflicts) so the cursor still lands on the
/// right commit once the whole batch finally completes.
///
/// Owned by the event loop rather than `AppState`: it is meaningful only
/// between one dispatch handler and another, and it must not reach the on-disk
/// journal — an index into the in-memory commit list would be nonsense after a
/// crash rebuilt that list.
#[derive(Default)]
pub(crate) struct PendingAutofixupSelection(Option<usize>);

impl PendingAutofixupSelection {
    /// Remember where the cursor should land, or `None` when there is nothing
    /// sensible to restore (e.g. the selection was a synthetic row).
    pub(crate) fn set(&mut self, index: Option<usize>) {
        self.0 = index;
    }

    pub(crate) fn take(&mut self) -> Option<usize> {
        self.0.take()
    }

    pub(crate) fn clear(&mut self) {
        self.0 = None;
    }
}

/// Fetch HEAD OID from the repo, setting an error message and returning
/// `LoopAction::Continue` from the enclosing function if the call fails.
///
/// Used by the dispatch handlers; `LoopAction` must be in scope at the call
/// site.
#[macro_export]
macro_rules! get_head_oid_or_continue {
    ($git_repo:expr, $app:expr) => {
        match $git_repo.head_oid() {
            Ok(oid) => oid,
            Err(e) => {
                $app.set_error_message(format!("Failed to get HEAD: {e:#}"));
                return Ok(LoopAction::Continue);
            }
        }
    };
}

/// Auto-stash the working tree before a history-rewriting operation, setting an
/// error message and returning `LoopAction::Proceed` from the enclosing function
/// if the stash fails. Used by the dispatch handlers; `LoopAction` must be in
/// scope at the call site.
#[macro_export]
macro_rules! autostash_save_or_bail {
    ($git_repo:expr, $app:expr) => {
        if let Err(e) = $git_repo.autostash_save() {
            $app.set_error_message(format!("Auto-stash failed: {e:#}"));
            return Ok(LoopAction::Proceed);
        }
    };
}

/// Handle the side effects requested by a view-module action. Returns whether
/// the caller should `continue` the event loop or fall through to the
/// post-dispatch checks.
pub(crate) fn dispatch_action(
    result: AppAction,
    app: &mut AppState,
    git_repo: &mut impl GitRepo,
    pending_autofixup: &mut PendingAutofixupSelection,
    terminal_guard: &mut crate::terminal_guard::TerminalGuard,
    kb_enhanced: bool,
) -> Result<LoopAction> {
    match result {
        AppAction::Handled => {}
        AppAction::Quit => app.should_quit = true,
        AppAction::ReloadCommits => return Ok(LoopAction::Reload),
        AppAction::LoadDetailDiff => views::commit_detail::load_diff(git_repo, app),
        AppAction::PrepareSplit {
            strategy,
            commit_oid,
        } => return handle_prepare_split(git_repo, app, strategy, commit_oid),
        AppAction::ExecuteSplit {
            strategy,
            commit_oid,
            head_oid,
        } => {
            return Ok(execute_split(
                git_repo,
                app,
                strategy,
                &commit_oid,
                &head_oid,
            ));
        }
        AppAction::PrepareSplitOutFiles { commit_oid } => {
            return handle_prepare_split_out_files(git_repo, app, commit_oid);
        }
        AppAction::ExecuteSplitOutFiles {
            commit_oid,
            file_paths,
        } => return handle_execute_split_out_files(git_repo, app, commit_oid, file_paths),
        AppAction::PrepareSplitOutHunks {
            commit_oid,
            context_lines,
        } => {
            return handle_prepare_split_out_hunks(git_repo, app, commit_oid, context_lines);
        }
        AppAction::ExecuteSplitOutHunks {
            commit_oid,
            hunks,
            context_lines,
        } => {
            return handle_execute_split_out_hunks(git_repo, app, commit_oid, hunks, context_lines);
        }
        AppAction::PrepareDropConfirm {
            commit_oid,
            commit_summary,
        } => {
            let head_oid = get_head_oid_or_continue!(git_repo, app);
            app.enter_drop_confirm(commit_oid, commit_summary, head_oid);
        }
        AppAction::ExecuteDrop {
            commit_oid,
            head_oid,
        } => return handle_execute_drop(git_repo, app, commit_oid, head_oid),
        AppAction::ExecuteEdit {
            commit_oid,
            commit_summary,
        } => {
            return handle_execute_edit(
                git_repo,
                app,
                commit_oid,
                commit_summary,
                terminal_guard,
                kb_enhanced,
            );
        }
        AppAction::RebaseContinue(state) => {
            return handle_rebase_continue(
                git_repo,
                app,
                pending_autofixup,
                state,
                terminal_guard,
                kb_enhanced,
            );
        }
        AppAction::RebaseAbort(state) => {
            return handle_rebase_abort(git_repo, app, pending_autofixup, state);
        }
        AppAction::RunMergetool {
            files,
            conflict_state,
        } => {
            let outcome = run_mergetool_suspended(git_repo, &files, terminal_guard, kb_enhanced);
            return Ok(handle_run_conflict_tool(
                git_repo,
                app,
                conflict_state,
                "Merge tool",
                outcome,
            ));
        }
        AppAction::RunEditor {
            files,
            conflict_state,
        } => {
            let outcome = run_editor_suspended(git_repo, &files, terminal_guard, kb_enhanced);
            return Ok(handle_run_conflict_tool(
                git_repo,
                app,
                conflict_state,
                "Editor",
                outcome,
            ));
        }
        AppAction::AutostashContinue => return handle_autostash_continue(git_repo, app),
        AppAction::AutostashAbort => return handle_autostash_abort(git_repo, app),
        AppAction::RunMergetoolForStash { files } => {
            let outcome = run_mergetool_suspended(git_repo, &files, terminal_guard, kb_enhanced);
            return Ok(handle_run_stash_tool(git_repo, app, "Merge tool", outcome));
        }
        AppAction::RunEditorForStash { files } => {
            let outcome = run_editor_suspended(git_repo, &files, terminal_guard, kb_enhanced);
            return Ok(handle_run_stash_tool(git_repo, app, "Editor", outcome));
        }
        AppAction::PrepareReword {
            commit_oid,
            current_message,
        } => {
            return handle_prepare_reword(
                git_repo,
                app,
                commit_oid,
                current_message,
                terminal_guard,
                kb_enhanced,
            );
        }
        AppAction::PrepareSquash {
            source_oid,
            target_oid,
            source_message,
            target_message,
            squash_mode,
        } => {
            return handle_prepare_squash(
                git_repo,
                app,
                source_oid,
                target_oid,
                source_message,
                target_message,
                squash_mode,
                terminal_guard,
                kb_enhanced,
            );
        }
        AppAction::ExecuteMove {
            source_oid,
            insert_after_oid,
        } => return handle_execute_move(git_repo, app, source_oid, insert_after_oid),
        AppAction::PrepareAutofixupConfirm => {
            return handle_prepare_autofixup_confirm(git_repo, app);
        }
        AppAction::PrepareAutofixupEditMessage {
            target_summary,
            template,
        } => {
            return handle_prepare_autofixup_edit_message(
                git_repo,
                app,
                target_summary,
                template,
                terminal_guard,
                kb_enhanced,
            );
        }
        AppAction::ExecuteAutofixup {
            head_oid,
            reference_oid,
            pairs,
            message_overrides,
        } => {
            return handle_execute_autofixup(
                git_repo,
                app,
                pending_autofixup,
                head_oid,
                reference_oid,
                pairs,
                message_overrides,
            );
        }
        AppAction::StageAll => return handle_stage_all(git_repo, app),
        AppAction::UnstageAll => return handle_unstage_all(git_repo, app),
        AppAction::PrepareCommitStaged => {
            return handle_commit_staged(git_repo, app, terminal_guard, kb_enhanced);
        }
        AppAction::Undo => return handle_undo(git_repo, app),
        AppAction::Redo => return handle_redo(git_repo, app),
    }
    Ok(LoopAction::Proceed)
}

/// Report a stage-all / unstage-all outcome. A change triggers a reload so the
/// synthetic Staged/Unstaged rows refresh; a no-op leaves the view untouched.
pub(crate) fn report_stage_outcome(
    app: &mut AppState,
    outcome: Result<StageOutcome>,
    changed_msg: &str,
    noop_msg: &str,
) -> LoopAction {
    match outcome {
        Ok(StageOutcome::Changed) => {
            app.set_success_message(changed_msg.to_string());
            LoopAction::Reload
        }
        Ok(StageOutcome::NoOp) => {
            app.set_success_message(noop_msg.to_string());
            LoopAction::Proceed
        }
        Err(e) => {
            app.set_error_message(format!("{e:#}"));
            LoopAction::Proceed
        }
    }
}

/// Settle the auto-stash after an operation completed. On a clean reapply, show
/// `success` and return `done`. On a conflict, open the stash-conflict dialog
/// (Esc there aborts the whole operation). On an unexpected restore error, warn
/// the user that the stash was not restored.
///
/// `op_label` titles the conflict dialog (e.g. "Drop" → "after drop").
pub(crate) fn settle_autostash(
    app: &mut AppState,
    restored: Result<AutostashRestore>,
    op_label: &str,
    success: &str,
    done: LoopAction,
) -> LoopAction {
    match restored {
        Ok(AutostashRestore::Done) => {
            app.set_success_message(success.to_string());
            done
        }
        Ok(AutostashRestore::Conflict { files }) => {
            app.enter_stash_conflict(StashConflictState {
                operation_label: op_label.to_string(),
                conflicting_files: files,
                still_unresolved: false,
            });
            LoopAction::Continue
        }
        Err(e) => {
            app.set_error_message(format!("{success}; auto-stash NOT restored: {e:#}"));
            done
        }
    }
}

/// Suspend the TUI and run the user's `$EDITOR` seeded with `seed`, returning the
/// edited message. Either failure — suspending/restoring the TUI, or the editor
/// process itself — comes back as `Err` for the caller to show; neither is
/// fatal.
pub(crate) fn edit_message_suspended(
    git_repo: &impl GitRepo,
    terminal_guard: &mut crate::terminal_guard::TerminalGuard,
    kb_enhanced: bool,
    seed: &str,
) -> Result<String> {
    let terminal_bg = terminal_guard.background();
    with_tui_suspended(terminal_guard.terminal(), kb_enhanced, terminal_bg, || {
        editor::edit_message_in_editor(git_repo, seed)
    })?
}

/// Reduce a rebase result to its UI side effect.
///
/// On success, sets a status message and returns `LoopAction::ReloadPreserving`
/// so the caller can trigger a reload with the selection preserved. On conflict,
/// enters conflict mode. On error, sets an error message.
pub(crate) fn handle_rebase_outcome(
    git_repo: &mut impl GitRepo,
    app: &mut AppState,
    outcome: anyhow::Result<RebaseOutcome>,
    op_label: &str,
    success_msg: &str,
) -> LoopAction {
    match outcome {
        Ok(RebaseOutcome::Complete) => {
            // Operation finished — reapply any auto-stash, opening the
            // resolution dialog if it conflicts.
            settle_autostash(
                app,
                git_repo.autostash_restore(),
                op_label,
                success_msg,
                LoopAction::ReloadPreserving,
            )
        }
        Ok(RebaseOutcome::Conflict(state)) => {
            // Defer the auto-stash restore until the conflict is resolved/aborted.
            app.enter_rebase_conflict(*state);
            LoopAction::Continue
        }
        Err(e) => {
            // The operation did not complete — restore the working tree.
            let _ = git_repo.autostash_restore();
            app.set_error_message(format!("{op_label} failed: {e:#}"));
            LoopAction::Proceed
        }
    }
}
