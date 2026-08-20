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

// Side-effect handlers for the working-tree/index staging operations: stage all,
// unstage all, and committing the staged changes.

use anyhow::Result;
use git_tailor::app::AppState;
use git_tailor::repo::{CommitOutcome, GitRepo};

use crate::dispatch::{LoopAction, edit_message_suspended, report_stage_outcome};

pub(crate) fn handle_stage_all(git_repo: &impl GitRepo, app: &mut AppState) -> Result<LoopAction> {
    let outcome = git_repo.stage_all();
    Ok(report_stage_outcome(
        app,
        outcome,
        "Staged all changes",
        "Nothing to stage",
    ))
}

pub(crate) fn handle_unstage_all(
    git_repo: &impl GitRepo,
    app: &mut AppState,
) -> Result<LoopAction> {
    let outcome = git_repo.unstage_all();
    Ok(report_stage_outcome(
        app,
        outcome,
        "Unstaged all changes",
        "Nothing to unstage",
    ))
}

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
        Err(e) => app.set_error_message(format!("Editor error: {e:#}")),
        Ok(message) if message.trim().is_empty() => {
            app.set_success_message("Commit cancelled: message is empty");
        }
        Ok(message) => match git_repo.commit_staged(&message) {
            Ok(CommitOutcome::Committed) => {
                app.set_success_message("Committed staged changes");
                return Ok(LoopAction::Reload);
            }
            Ok(CommitOutcome::NothingStaged) => app.set_error_message("Nothing staged to commit"),
            Err(e) => app.set_error_message(format!("Commit failed: {e:#}")),
        },
    }
    Ok(LoopAction::Proceed)
}
