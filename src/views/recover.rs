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

// Startup crash-recovery prompt for an operation a previous run was killed in.

use super::dialog::{
    Dialog, DialogKind, TextRole, handle_dialog_scroll, inner_width, truncate_path_tail,
};
use crate::app::{AppAction, AppMode, AppState, KeyCommand};
use ratatui::{Frame, style::Color};

/// Handle an action while in RecoverConfirm mode.
pub fn handle_recover_key(action: KeyCommand, app: &mut AppState) -> AppAction {
    match action {
        KeyCommand::Confirm => {
            // Resume: hand the persisted state to the normal conflict flow, where
            // Enter=continue / Esc=abort already work.
            if let AppMode::RecoverConfirm(state) =
                std::mem::replace(&mut app.mode, AppMode::CommitList)
            {
                app.enter_rebase_conflict(*state);
            }
            AppAction::Handled
        }
        KeyCommand::Quit => {
            if let AppMode::RecoverConfirm(state) =
                std::mem::replace(&mut app.mode, AppMode::CommitList)
            {
                AppAction::RebaseAbort(*state)
            } else {
                AppAction::Handled
            }
        }
        KeyCommand::ShowHelp => {
            app.toggle_help();
            AppAction::Handled
        }
        _ => {
            handle_dialog_scroll(action, &mut app.dialog);
            AppAction::Handled
        }
    }
}

/// Render the crash-recovery prompt as a centered overlay.
pub fn render_recover(app: &mut AppState, frame: &mut Frame) {
    let state = match &app.mode {
        AppMode::RecoverConfirm(s) => s,
        _ => return,
    };

    let label = &state.operation_label;
    let label_lower = label.to_lowercase();

    const PREFERRED_WIDTH: u16 = 62;
    let iw = inner_width(PREFERRED_WIDTH, frame.area().width);

    let mut dialog = Dialog::new(DialogKind::Danger, app.colors)
        .heading(
            format!("Interrupted {label_lower} found"),
            TextRole::Highlight,
        )
        .wrapped(
            &format!(
                "git-tailor was closed while a {label_lower} was in progress. The \
                 working tree still holds the unresolved changes."
            ),
            iw.saturating_sub(1),
        );

    if !state.conflicting_files.is_empty() {
        dialog = dialog
            .blank()
            .styled_line("Conflicting files:", TextRole::Highlight);
        const MAX_FILES: usize = 5;
        let shown = state.conflicting_files.len().min(MAX_FILES);
        for path in &state.conflicting_files[..shown] {
            dialog = dialog.styled_line(truncate_path_tail(path, iw), TextRole::Danger);
        }
        let extra = state.conflicting_files.len().saturating_sub(MAX_FILES);
        if extra > 0 {
            dialog = dialog.styled_line(format!("... {extra} more"), TextRole::Muted);
        }
    }

    let (max_scroll, visible_height) = dialog
        .blank()
        .instructions(&[
            ("Enter", Color::Green, "Resume"),
            ("Esc", Color::Red, "Abort"),
        ])
        .blank()
        .render(frame, "Recover", PREFERRED_WIDTH, app.dialog.offset);
    app.dialog.set_bounds(max_scroll, visible_height);
}
