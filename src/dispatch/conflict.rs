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

// Side-effect handlers for resolving rebase and auto-stash conflicts: running
// the merge tool or editor, and continuing or aborting the paused operation.

use anyhow::Result;
use git_tailor::app::{AppMode, AppState, SquashMode};
use git_tailor::repo::{AutostashContinue, ConflictState, GitRepo, StashConflictState};
use git_tailor::{editor, mergetool};

use crate::dispatch::autofixup::apply_pending_autofixup_selection;
use crate::dispatch::{LoopAction, handle_rebase_outcome, settle_autostash};
use crate::external_tool::with_tui_suspended;

pub(crate) fn handle_rebase_abort(
    git_repo: &mut impl GitRepo,
    app: &mut AppState,
    state: ConflictState,
) -> Result<LoopAction> {
    // Aborting unwinds the whole (possibly autofixup) operation back to its
    // original tip, so any pending cursor-restoration index no longer
    // applies — clear it rather than risk it leaking into a later reload.
    app.pending_autofixup_selection = None;
    match git_repo.rebase_abort(&state) {
        Ok(()) => {
            let restored = git_repo.autostash_restore();
            let label = state.operation_label.to_lowercase();
            Ok(settle_autostash(
                app,
                restored,
                &state.operation_label,
                &format!("{} aborted", label.trim()),
                LoopAction::Reload,
            ))
        }
        Err(e) => {
            app.set_error_message(format!("Abort failed: {e}"));
            Ok(LoopAction::Proceed)
        }
    }
}

pub(crate) fn handle_rebase_continue(
    git_repo: &mut impl GitRepo,
    app: &mut AppState,
    state: ConflictState,
    terminal_guard: &mut crate::terminal_guard::TerminalGuard,
    kb_enhanced: bool,
) -> Result<LoopAction> {
    let is_autofixup = state.autofixup_context.is_some();
    let _ = git_repo.auto_stage_resolved_conflicts(&state.conflicting_files);
    if let Some(ref ctx) = state.squash_context {
        let original_oid = state.original_branch_oid.clone();
        let ctx_clone = ctx.clone();
        let conflict_files = git_repo.read_conflicting_files();
        if !conflict_files.is_empty() {
            app.enter_rebase_conflict(git_tailor::repo::ConflictState {
                conflicting_files: conflict_files,
                still_unresolved: true,
                ..state
            });
            return Ok(LoopAction::Continue);
        }
        // An autofixup batch never pauses for interactive editing per pair
        // (the confirmation dialog is the only prompt) — squash!-mode
        // conflicts keep the non-interactive combined message computed up
        // front, the same as fixup!-mode already does.
        let final_msg = if ctx.squash_mode.keeps_target_message() || is_autofixup {
            ctx.combined_message.clone()
        } else {
            let combined = ctx.combined_message.clone();
            let terminal_bg = terminal_guard.background();
            let editor_result =
                with_tui_suspended(terminal_guard.terminal(), kb_enhanced, terminal_bg, || {
                    editor::edit_message_in_editor(git_repo, &combined)
                })?;
            match editor_result {
                Err(e) => {
                    let _ = git_repo.rebase_abort(&state);
                    let _ = git_repo.autostash_restore();
                    app.set_error_message(format!("Editor error: {e}"));
                    return Ok(LoopAction::Reload);
                }
                Ok(msg) if msg.trim().is_empty() => {
                    let _ = git_repo.rebase_abort(&state);
                    let _ = git_repo.autostash_restore();
                    let label = &state.operation_label;
                    app.set_error_message(format!("{label} aborted: empty commit message"));
                    return Ok(LoopAction::Continue);
                }
                Ok(msg) => msg,
            }
        };
        let success_msg = match ctx_clone.squash_mode {
            SquashMode::Fixup => "Commit fixed up",
            SquashMode::Squash => "Commits squashed",
        };
        let outcome = git_repo.squash_finalize(
            &ctx_clone,
            &final_msg,
            &original_oid,
            state.autofixup_context.as_ref(),
        );
        let result = handle_rebase_outcome(git_repo, app, outcome, "Squash", success_msg);
        return Ok(apply_pending_autofixup_selection(app, is_autofixup, result));
    }
    let success_msg = format!("Commit {} complete", state.operation_label.to_lowercase());
    let outcome = git_repo.rebase_continue(&state);
    let result = handle_rebase_outcome(git_repo, app, outcome, "Continue", &success_msg);
    Ok(apply_pending_autofixup_selection(app, is_autofixup, result))
}

pub(crate) fn handle_run_mergetool(
    git_repo: &impl GitRepo,
    app: &mut AppState,
    files: Vec<String>,
    conflict_state: ConflictState,
    terminal_guard: &mut crate::terminal_guard::TerminalGuard,
    kb_enhanced: bool,
) -> Result<LoopAction> {
    let terminal_bg = terminal_guard.background();
    let result = with_tui_suspended(terminal_guard.terminal(), kb_enhanced, terminal_bg, || {
        mergetool::run_mergetool(git_repo, &files)
    })?;
    match result {
        Ok(true) => {
            let new_files = git_repo.read_conflicting_files();
            app.mode = AppMode::RebaseConflict(Box::new(git_tailor::repo::ConflictState {
                conflicting_files: new_files,
                still_unresolved: false,
                ..conflict_state
            }));
            app.set_success_message(
                "Merge tool finished \u{2014} press Enter when done or Esc to abort",
            );
        }
        Ok(false) => {
            app.set_error_message("No merge tool configured (set merge.tool in git config)");
        }
        Err(e) => {
            app.set_error_message(format!("Merge tool failed: {e}"));
        }
    }
    Ok(LoopAction::Proceed)
}

pub(crate) fn handle_run_editor(
    git_repo: &impl GitRepo,
    app: &mut AppState,
    files: Vec<String>,
    conflict_state: ConflictState,
    terminal_guard: &mut crate::terminal_guard::TerminalGuard,
    kb_enhanced: bool,
) -> Result<LoopAction> {
    let workdir = git_repo.workdir();
    let terminal_bg = terminal_guard.background();
    let result: anyhow::Result<()> =
        with_tui_suspended(terminal_guard.terminal(), kb_enhanced, terminal_bg, || {
            let workdir =
                workdir.ok_or_else(|| anyhow::anyhow!("repository has no working directory"))?;
            for file_path in &files {
                editor::open_file_in_editor(git_repo, &workdir.join(file_path))?;
            }
            Ok(())
        })?;
    match result {
        Ok(()) => {
            let new_files = git_repo.read_conflicting_files();
            app.mode = AppMode::RebaseConflict(Box::new(git_tailor::repo::ConflictState {
                conflicting_files: new_files,
                still_unresolved: false,
                ..conflict_state
            }));
            app.set_success_message(
                "Editor finished \u{2014} press Enter when done or Esc to abort",
            );
        }
        Err(e) => {
            app.set_error_message(format!("Editor failed: {e}"));
        }
    }
    Ok(LoopAction::Proceed)
}

/// Run the merge tool (`use_mergetool`) or editor over the files conflicting in
/// an auto-stash reapply, then refresh the stash-conflict dialog.
pub(crate) fn handle_run_stash_tool(
    git_repo: &impl GitRepo,
    app: &mut AppState,
    files: Vec<String>,
    terminal_guard: &mut crate::terminal_guard::TerminalGuard,
    kb_enhanced: bool,
    use_mergetool: bool,
) -> Result<LoopAction> {
    let operation_label = match &app.mode {
        AppMode::StashConflict(s) => s.operation_label.clone(),
        _ => String::new(),
    };
    let tool = if use_mergetool {
        "Merge tool"
    } else {
        "Editor"
    };
    let workdir = git_repo.workdir();
    let terminal_bg = terminal_guard.background();
    let result: anyhow::Result<bool> =
        with_tui_suspended(terminal_guard.terminal(), kb_enhanced, terminal_bg, || {
            if use_mergetool {
                mergetool::run_mergetool(git_repo, &files)
            } else {
                let workdir = workdir
                    .ok_or_else(|| anyhow::anyhow!("repository has no working directory"))?;
                for file_path in &files {
                    editor::open_file_in_editor(git_repo, &workdir.join(file_path))?;
                }
                Ok(true)
            }
        })?;
    match result {
        Ok(true) => {
            let new_files = git_repo.read_conflicting_files();
            app.mode = AppMode::StashConflict(Box::new(StashConflictState {
                operation_label,
                conflicting_files: new_files,
                still_unresolved: false,
            }));
            app.set_success_message(format!(
                "{tool} finished \u{2014} press Enter when done or Esc to abort"
            ));
        }
        Ok(false) => {
            app.set_error_message("No merge tool configured (set merge.tool in git config)");
        }
        Err(e) => {
            app.set_error_message(format!("{tool} failed: {e}"));
        }
    }
    Ok(LoopAction::Proceed)
}

/// Finish a conflicting auto-stash reapply: drop the stash if everything is
/// resolved, otherwise refresh the dialog with the remaining conflicts.
pub(crate) fn handle_autostash_continue(
    git_repo: &mut impl GitRepo,
    app: &mut AppState,
) -> Result<LoopAction> {
    let operation_label = match &app.mode {
        AppMode::StashConflict(s) => s.operation_label.clone(),
        _ => String::new(),
    };
    match git_repo.autostash_conflict_continue() {
        Ok(AutostashContinue::Resolved) => {
            app.mode = AppMode::CommitList;
            app.set_success_message("Auto-stashed changes restored");
            Ok(LoopAction::ReloadPreserving)
        }
        Ok(AutostashContinue::StillUnresolved { files }) => {
            app.mode = AppMode::StashConflict(Box::new(StashConflictState {
                operation_label,
                conflicting_files: files,
                still_unresolved: true,
            }));
            Ok(LoopAction::Continue)
        }
        Err(e) => {
            app.set_error_message(format!("Failed to finish auto-stash: {e}"));
            Ok(LoopAction::Continue)
        }
    }
}

/// Abort a conflicting auto-stash reapply: rewind the whole operation and put
/// the user's original dirty changes back.
pub(crate) fn handle_autostash_abort(
    git_repo: &mut impl GitRepo,
    app: &mut AppState,
) -> Result<LoopAction> {
    let label = match &app.mode {
        AppMode::StashConflict(s) => s.operation_label.to_lowercase(),
        _ => String::new(),
    };
    match git_repo.autostash_conflict_abort() {
        Ok(()) => {
            app.mode = AppMode::CommitList;
            app.set_success_message(format!(
                "{} aborted \u{2014} changes restored",
                label.trim()
            ));
            Ok(LoopAction::Reload)
        }
        Err(e) => {
            app.set_error_message(format!("Abort failed: {e}"));
            Ok(LoopAction::Continue)
        }
    }
}
