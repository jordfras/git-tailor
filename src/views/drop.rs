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

// Drop commit confirmation dialog

use super::dialog::{Dialog, DialogKind, TextRole, handle_dialog_scroll, inner_width};
use crate::app::{AppAction, AppMode, AppState, KeyCommand};
use ratatui::{Frame, style::Color};

/// Handle an action while in DropConfirm mode.
pub fn handle_confirm_key(action: KeyCommand, app: &mut AppState) -> AppAction {
    match action {
        KeyCommand::Confirm => {
            if let AppMode::DropConfirm(pending) =
                std::mem::replace(&mut app.mode, AppMode::CommitList)
            {
                AppAction::ExecuteDrop {
                    commit_oid: pending.commit_oid,
                    head_oid: pending.head_oid,
                }
            } else {
                AppAction::Handled
            }
        }
        KeyCommand::ShowHelp => {
            app.toggle_help();
            AppAction::Handled
        }
        KeyCommand::Quit => {
            app.cancel_drop_confirm();
            AppAction::Handled
        }
        _ => {
            handle_dialog_scroll(action, app);
            AppAction::Handled
        }
    }
}

/// Render the drop confirmation dialog as a centered overlay.
pub fn render_drop_confirm(app: &mut AppState, frame: &mut Frame) {
    let pending = match &app.mode {
        AppMode::DropConfirm(p) => p,
        _ => return,
    };

    let short_oid = pending.commit_oid.short();

    const PREFERRED_WIDTH: u16 = 60;
    let iw = inner_width(PREFERRED_WIDTH, frame.area().width);

    let (max_scroll, visible_height) = Dialog::new(DialogKind::Confirm, app.colors)
        .heading("Drop this commit?", TextRole::Highlight)
        .styled_line(short_oid.to_string(), TextRole::Key)
        .wrapped(&pending.commit_summary, iw.saturating_sub(1))
        .blank()
        .instructions(&[
            ("Enter", Color::Cyan, "Confirm"),
            ("Esc", Color::Cyan, "Cancel"),
        ])
        .blank()
        .render(frame, "Confirm Drop", PREFERRED_WIDTH, app.dialog.offset);
    app.dialog.set_bounds(max_scroll, visible_height);
}
