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

//! Resolution dialog for conflicts left by reapplying the auto-stash after an
//! operation completed. It mirrors the rebase-conflict dialog — same mergetool
//! (`m`) / editor (`e`) keys and the same Esc-aborts-the-whole-operation
//! semantics — but "continue" drops the stash instead of resuming a cherry-pick.

use super::dialog::{
    Dialog, DialogKind, TextRole, handle_dialog_scroll, inner_width, render_conflict_dialog,
};
use crate::app::{AppAction, AppMode, AppState, KeyCommand};
use ratatui::Frame;

/// Handle an action while in StashConflict mode.
pub fn handle_stash_conflict_key(action: KeyCommand, app: &mut AppState) -> AppAction {
    match action {
        KeyCommand::Confirm => AppAction::AutostashContinue,
        KeyCommand::Mergetool => match &app.mode {
            AppMode::StashConflict(state) => AppAction::RunMergetoolForStash {
                files: state.conflicting_files.clone(),
            },
            _ => AppAction::Handled,
        },
        KeyCommand::OpenEditor => match &app.mode {
            AppMode::StashConflict(state) => AppAction::RunEditorForStash {
                files: state.conflicting_files.clone(),
            },
            _ => AppAction::Handled,
        },
        KeyCommand::ShowHelp => {
            app.toggle_help();
            AppAction::Handled
        }
        KeyCommand::Quit => AppAction::AutostashAbort,
        _ => {
            handle_dialog_scroll(action, app);
            AppAction::Handled
        }
    }
}

/// Render the auto-stash conflict resolution dialog as a centered overlay.
pub fn render_stash_conflict(app: &mut AppState, frame: &mut Frame) {
    let state = match &app.mode {
        AppMode::StashConflict(s) => s,
        _ => return,
    };

    let label_lower = state.operation_label.to_lowercase();

    const PREFERRED_WIDTH: u16 = 62;
    let iw = inner_width(PREFERRED_WIDTH, frame.area().width);

    let dialog = Dialog::new(DialogKind::Danger, app.colors)
        .heading(
            format!("Auto-stash conflict after {label_lower}"),
            TextRole::Danger,
        )
        .wrapped(
            "Your staged/unstaged changes conflict with the result. Resolve the \
             markers below, then continue to keep them — or abort to undo the \
             whole operation and get your changes back unchanged.",
            iw,
        );

    // Copy the bits the shared renderer needs so the immutable borrow of
    // `app.mode` (via `state`) ends before it takes `app` mutably.
    let files = state.conflicting_files.clone();
    let still_unresolved = state.still_unresolved;
    render_conflict_dialog(
        app,
        frame,
        dialog,
        &files,
        still_unresolved,
        PREFERRED_WIDTH,
        "Auto-stash Conflict",
    );
}
