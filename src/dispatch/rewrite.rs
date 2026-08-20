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

// Side-effect handlers for the single-commit history rewrites: drop, move,
// reword, and squash/fixup. (Split and Edit are richer multi-step flows and
// live in their own modules.)

use anyhow::Result;
use git_tailor::Oid;
use git_tailor::app::{AppState, SquashMode};
use git_tailor::repo::GitRepo;

use crate::dispatch::{LoopAction, edit_message_suspended, handle_rebase_outcome};
use crate::{autostash_save_or_bail, get_head_oid_or_continue};

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
        Err(e) => app.set_error_message(format!("Editor error: {e:#}")),
        Ok(new_message) if new_message.trim().is_empty() => {
            app.set_success_message("Reword cancelled: message is empty");
        }
        Ok(new_message) if new_message == current_message => {
            app.set_success_message("No changes made");
        }
        Ok(new_message) => match git_repo.reword_commit(&commit_oid, &new_message, &head_oid) {
            Ok(()) => return Ok(LoopAction::ReloadPreserving),
            Err(e) => app.set_error_message(format!("Reword failed: {e:#}")),
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
            app.set_error_message(format!("{label} failed: {e:#}"));
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
                app.set_error_message(format!("Editor error: {e:#}"));
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
