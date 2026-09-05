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

// Operation picker dialog: a menu of the operations valid for the selected row.

use super::commit_list;
use super::dialog::{Dialog, DialogKind, TextRole};
use super::list_nav::{self, ListNav};
use crate::app::{AppAction, AppMode, AppState, KeyCommand, Operation};
use ratatui::{
    Frame,
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

/// Operations offered for the currently selected row. Empty only if there is no
/// selection at all (in practice every row yields at least undo/redo).
fn available(app: &AppState) -> Vec<Operation> {
    let is_oldest = app.list.selected_is_oldest_commit();
    app.list
        .selected_virtual_oid()
        .map(|oid| Operation::available_for(oid, is_oldest))
        .unwrap_or_default()
}

/// Rows above the first operation: blank, heading (blank/content/blank), hint.
const HEADER_LINES: usize = 5;

/// Handle an action while in OperationSelect mode.
pub fn handle_key(action: KeyCommand, app: &mut AppState) -> AppAction {
    let operation = match app.mode {
        AppMode::OperationSelect { operation } => operation,
        _ => return AppAction::Handled,
    };

    let ops = available(app);
    let len = ops.len();
    // Position of the highlighted operation in the (stable-while-modal) menu.
    let mut cursor = ops.iter().position(|op| *op == operation).unwrap_or(0);

    match list_nav::handle_list_navigation(action, &mut cursor, len, len, false) {
        ListNav::Moved => {
            if let Some(&operation) = ops.get(cursor) {
                app.mode = AppMode::OperationSelect { operation };
                app.dialog.ensure_visible(HEADER_LINES + cursor, 1);
            }
            AppAction::Handled
        }
        ListNav::Confirmed => {
            // Close the picker, then dispatch through the commit list's handler
            // so the operation reuses its existing entry point (some open a
            // follow-up dialog, others return an action for main.rs to execute).
            app.mode = AppMode::CommitList;
            commit_list::handle_key(operation.key_command(), app)
        }
        ListNav::Canceled => {
            app.mode = AppMode::CommitList;
            AppAction::Handled
        }
        ListNav::Help => {
            app.toggle_help();
            AppAction::Handled
        }
        ListNav::Unhandled => {
            // A shown shortcut runs its operation directly, as if it were
            // highlighted and confirmed. Only operations offered for this row
            // react; any other key is ignored (the dialog stays open).
            if ops.iter().any(|op| op.key_command() == action) {
                app.mode = AppMode::CommitList;
                commit_list::handle_key(action, app)
            } else {
                AppAction::Handled
            }
        }
    }
}

/// Render the operation picker as a centered overlay.
pub fn render(app: &mut AppState, frame: &mut Frame) {
    // Header naming the row the operations apply to.
    let header = app
        .list
        .selected()
        .map(|c| {
            if c.oid.is_synthetic() {
                format!("{} changes", c.oid.short())
            } else {
                format!("{} {}", c.oid.short(), c.summary)
            }
        })
        .unwrap_or_default();

    // Truncate by characters, not bytes, so a non-ASCII summary never slices
    // mid-codepoint (which panics).
    let max_header_len = 44;
    let display_header = if header.chars().count() > max_header_len {
        let prefix: String = header.chars().take(max_header_len - 1).collect();
        format!("{prefix}…")
    } else {
        header
    };

    let selected_operation = match app.mode {
        AppMode::OperationSelect { operation } => Some(operation),
        _ => None,
    };

    let ops = available(app);

    let mut dialog = Dialog::new(DialogKind::Info, app.colors)
        .blank()
        .push_line(Line::from(Span::styled(
            format!(" {display_header}"),
            Style::default()
                .fg(app.colors.resolve(Color::White))
                .add_modifier(Modifier::DIM),
        )))
        .heading("Choose operation:", TextRole::Highlight);

    // One line per operation: marker + label column, the keyboard shortcut (so
    // users learn the direct keys), then a muted description. Compact so all
    // operations fit without scrolling.
    for op in ops.iter() {
        let selected = selected_operation == Some(*op);
        let marker = if selected { "▸" } else { " " };
        let label_style = if selected {
            Style::default()
                .fg(app.colors.resolve(Color::Cyan))
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(app.colors.resolve(Color::White))
        };

        dialog = dialog.push_line(Line::from(vec![
            Span::styled(format!(" {} {:<14}", marker, op.label()), label_style),
            Span::styled(
                format!("{:<7}", op.shortcut()),
                Style::default().fg(app.colors.resolve(TextRole::Key.color())),
            ),
            Span::styled(
                op.description(),
                Style::default().fg(app.colors.resolve(TextRole::Muted.color())),
            ),
        ]));
    }

    dialog = dialog
        .blank()
        .instructions(&[
            ("Enter", Color::Cyan, "Select"),
            ("Esc", Color::Cyan, "Cancel"),
        ])
        .blank();

    let content_width = 54;
    let (max_scroll, visible_height) =
        dialog.render(frame, "Operations", content_width, app.dialog.offset);
    app.dialog.set_bounds(max_scroll, visible_height);
}
