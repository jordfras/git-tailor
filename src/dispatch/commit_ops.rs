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

// Side-effect handlers for the single-commit operations: commit staged, undo,
// redo, drop, move, reword, and squash/fixup.

use anyhow::Result;
use git_tailor::Oid;
use git_tailor::app::{AppState, SquashMode};
use git_tailor::repo::{AutostashRestore, CommitOutcome, GitRepo, UndoOutcome};

use crate::dispatch::{
    LoopAction, edit_message_suspended, handle_rebase_outcome, settle_autostash,
};
use crate::{autostash_save_or_bail, get_head_oid_or_continue};

/// Commit the staged changes: open the editor for a message (reusing the reword
/// editor flow), then create the commit. An empty message cancels.
pub(crate) fn handle_commit_staged(
    git_repo: &impl GitRepo,
    app: &mut AppState,
    terminal_guard: &mut crate::terminal_guard::TerminalGuard,
    kb_enhanced: bool,
) -> Result<LoopAction> {
    let editor_result = edit_message_suspended(git_repo, terminal_guard, kb_enhanced, "");
    match editor_result {
        Err(e) => app.set_error_message(format!("Editor error: {e}")),
        Ok(message) if message.trim().is_empty() => {
            app.set_success_message("Commit cancelled: message is empty");
        }
        Ok(message) => match git_repo.commit_staged(&message) {
            Ok(CommitOutcome::Committed) => {
                app.set_success_message("Committed staged changes");
                return Ok(LoopAction::Reload);
            }
            Ok(CommitOutcome::NothingStaged) => app.set_error_message("Nothing staged to commit"),
            Err(e) => app.set_error_message(format!("Commit failed: {e}")),
        },
    }
    Ok(LoopAction::Proceed)
}

pub(crate) fn handle_undo(git_repo: &mut impl GitRepo, app: &mut AppState) -> Result<LoopAction> {
    // A working-tree-preserving undo (stage/unstage all, or a commit's soft
    // reset) restores the very state that auto-stash would squirrel away and
    // reapply — running the stash dance would negate it — so bypass it for those.
    let skip_autostash = git_repo.pending_undo_skips_autostash().unwrap_or(false);
    if !skip_autostash && let Err(e) = git_repo.autostash_save() {
        app.set_error_message(format!("Auto-stash failed: {e}"));
        return Ok(LoopAction::Proceed);
    }
    let outcome = git_repo.undo();
    let restored = if skip_autostash {
        Ok(AutostashRestore::Done)
    } else {
        git_repo.autostash_restore()
    };
    match outcome {
        Ok(UndoOutcome::Done { label }) => Ok(settle_autostash(
            app,
            restored,
            "Undo",
            &format!("Undid {}", label.to_lowercase()),
            LoopAction::Reload,
        )),
        Ok(UndoOutcome::Empty) => {
            app.set_error_message("Nothing to undo");
            Ok(LoopAction::Proceed)
        }
        Ok(UndoOutcome::Stale) => {
            app.set_error_message("Undo history no longer matches the branch — discarded");
            Ok(LoopAction::Reload)
        }
        Err(e) => {
            app.set_error_message(format!("Undo failed: {e}"));
            Ok(LoopAction::Proceed)
        }
    }
}

pub(crate) fn handle_redo(git_repo: &mut impl GitRepo, app: &mut AppState) -> Result<LoopAction> {
    // See handle_undo: skip the auto-stash dance for a working-tree-preserving redo.
    let skip_autostash = git_repo.pending_redo_skips_autostash().unwrap_or(false);
    if !skip_autostash && let Err(e) = git_repo.autostash_save() {
        app.set_error_message(format!("Auto-stash failed: {e}"));
        return Ok(LoopAction::Proceed);
    }
    let outcome = git_repo.redo();
    let restored = if skip_autostash {
        Ok(AutostashRestore::Done)
    } else {
        git_repo.autostash_restore()
    };
    match outcome {
        Ok(UndoOutcome::Done { label }) => Ok(settle_autostash(
            app,
            restored,
            "Redo",
            &format!("Redid {}", label.to_lowercase()),
            LoopAction::Reload,
        )),
        Ok(UndoOutcome::Empty) => {
            app.set_error_message("Nothing to redo");
            Ok(LoopAction::Proceed)
        }
        Ok(UndoOutcome::Stale) => {
            app.set_error_message("Redo history no longer matches the branch — discarded");
            Ok(LoopAction::Reload)
        }
        Err(e) => {
            app.set_error_message(format!("Redo failed: {e}"));
            Ok(LoopAction::Proceed)
        }
    }
}

pub(crate) fn handle_execute_drop(
    git_repo: &mut impl GitRepo,
    app: &mut AppState,
    commit_oid: Oid,
    head_oid: Oid,
) -> Result<LoopAction> {
    autostash_save_or_bail!(git_repo, app);
    let outcome = git_repo.drop_commit(&commit_oid, &head_oid);
    Ok(handle_rebase_outcome(
        git_repo,
        app,
        outcome,
        "Drop",
        "Commit dropped",
    ))
}

pub(crate) fn handle_execute_move(
    git_repo: &mut impl GitRepo,
    app: &mut AppState,
    source_oid: Oid,
    insert_after_oid: Option<Oid>,
) -> Result<LoopAction> {
    let head_oid = get_head_oid_or_continue!(git_repo, app);
    autostash_save_or_bail!(git_repo, app);
    let outcome = git_repo.move_commit(&source_oid, insert_after_oid.as_ref(), &head_oid);
    Ok(handle_rebase_outcome(
        git_repo,
        app,
        outcome,
        "Move",
        "Commit moved",
    ))
}

pub(crate) fn handle_prepare_reword(
    git_repo: &impl GitRepo,
    app: &mut AppState,
    commit_oid: Oid,
    current_message: String,
    terminal_guard: &mut crate::terminal_guard::TerminalGuard,
    kb_enhanced: bool,
) -> Result<LoopAction> {
    let head_oid = get_head_oid_or_continue!(git_repo, app);
    let editor_result =
        edit_message_suspended(git_repo, terminal_guard, kb_enhanced, &current_message);
    match editor_result {
        Err(e) => app.set_error_message(format!("Editor error: {e}")),
        Ok(new_message) if new_message.trim().is_empty() => {
            app.set_success_message("Reword cancelled: message is empty");
        }
        Ok(new_message) if new_message == current_message => {
            app.set_success_message("No changes made");
        }
        Ok(new_message) => match git_repo.reword_commit(&commit_oid, &new_message, &head_oid) {
            Ok(()) => return Ok(LoopAction::ReloadPreserving),
            Err(e) => app.set_error_message(format!("Reword failed: {e}")),
        },
    }
    Ok(LoopAction::Proceed)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_prepare_squash(
    git_repo: &mut impl GitRepo,
    app: &mut AppState,
    source_oid: Oid,
    target_oid: Oid,
    source_message: String,
    target_message: String,
    squash_mode: SquashMode,
    terminal_guard: &mut crate::terminal_guard::TerminalGuard,
    kb_enhanced: bool,
) -> Result<LoopAction> {
    let head_oid = get_head_oid_or_continue!(git_repo, app);
    autostash_save_or_bail!(git_repo, app);
    let label = squash_mode.label();
    let message_for_context = if squash_mode.keeps_target_message() {
        target_message.clone()
    } else {
        format!("{target_message}\n\n{source_message}")
    };
    let combined = format!("{target_message}\n\n{source_message}");
    match git_repo.squash_try_combine(
        &source_oid,
        &target_oid,
        &message_for_context,
        squash_mode,
        &head_oid,
    ) {
        Ok(Some(conflict_state)) => {
            // Squash-tree conflict — defer restoring the auto-stash until the
            // user resolves and the squash finalizes (or aborts).
            app.enter_rebase_conflict(conflict_state);
            return Ok(LoopAction::Continue);
        }
        Err(e) => {
            let _ = git_repo.autostash_restore();
            app.set_error_message(format!("{label} failed: {e}"));
            return Ok(LoopAction::Continue);
        }
        Ok(None) => {}
    }
    let final_message = if squash_mode.keeps_target_message() {
        Some(target_message)
    } else {
        let editor_result =
            edit_message_suspended(git_repo, terminal_guard, kb_enhanced, &combined);
        match editor_result {
            Err(e) => {
                let _ = git_repo.autostash_restore();
                app.set_error_message(format!("Editor error: {e}"));
                return Ok(LoopAction::Continue);
            }
            Ok(msg) if msg.trim().is_empty() => {
                let _ = git_repo.autostash_restore();
                app.set_error_message(format!("{label} aborted: empty commit message"));
                return Ok(LoopAction::Continue);
            }
            Ok(msg) => Some(msg),
        }
    };
    if let Some(msg) = final_message {
        let success_msg = match squash_mode {
            SquashMode::Fixup => "Commit fixed up",
            SquashMode::Squash => "Commits squashed",
        };
        let outcome = git_repo.squash_commits(&source_oid, &target_oid, &msg, &head_oid);
        return Ok(handle_rebase_outcome(
            git_repo,
            app,
            outcome,
            label,
            success_msg,
        ));
    }
    Ok(LoopAction::Proceed)
}
