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

// Bulk autofixup confirmation dialog

use super::dialog::{Dialog, DialogKind, TextRole, handle_dialog_scroll, inner_width};
use crate::app::{AppAction, AppMode, AppState, KeyCommand};
use ratatui::{Frame, style::Color};

/// Handle an action while in AutofixupConfirm mode.
pub fn handle_confirm_key(action: KeyCommand, app: &mut AppState) -> AppAction {
    match action {
        KeyCommand::Confirm => {
            if let AppMode::AutofixupConfirm(pending) =
                std::mem::replace(&mut app.mode, AppMode::CommitList)
            {
                AppAction::ExecuteAutofixup {
                    head_oid: pending.head_oid,
                    reference_oid: pending.reference_oid,
                    pairs: pending.pairs,
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
            app.cancel_autofixup_confirm();
            AppAction::Handled
        }
        _ => {
            handle_dialog_scroll(action, app);
            AppAction::Handled
        }
    }
}

/// Render the autofixup confirmation dialog as a centered overlay.
pub fn render_autofixup_confirm(app: &mut AppState, frame: &mut Frame) {
    let pending = match &app.mode {
        AppMode::AutofixupConfirm(p) => p,
        _ => return,
    };

    const PREFERRED_WIDTH: u16 = 70;
    let iw = inner_width(PREFERRED_WIDTH, frame.area().width);

    let mut dialog = Dialog::new(DialogKind::Confirm, app.colors)
        .heading(
            format!("Squash {} commit(s)?", pending.pairs.len()),
            TextRole::Highlight,
        )
        .wrapped_styled(
            " Each commit is squashed into the target commit shown below it:",
            iw.saturating_sub(1),
            TextRole::Muted,
        )
        .blank();

    for pair in &pending.pairs {
        dialog = dialog
            .wrapped_styled(
                &format!(" {}", pair.source_summary),
                iw.saturating_sub(1),
                TextRole::Normal,
            )
            .styled_line(
                format!("  \u{2192} {}", pair.target_oid.short()),
                TextRole::Key,
            );
    }

    let (max_scroll, visible_height) = dialog
        .blank()
        .instructions(&[
            ("Enter", Color::Cyan, "Confirm"),
            ("Esc", Color::Cyan, "Cancel"),
        ])
        .blank()
        .render(
            frame,
            "Confirm Autofixup",
            PREFERRED_WIDTH,
            app.dialog_scroll_offset,
        );
    app.max_dialog_scroll = max_scroll;
    app.dialog_visible_height = visible_height;
}
