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

// Rebase conflict resolution dialog, shared by drop/squash/etc.

use super::dialog::{Dialog, DialogKind, TextRole, handle_dialog_scroll, inner_width};
use crate::app::{AppAction, AppMode, AppState, KeyCommand};
use ratatui::{
    Frame,
    style::{Color, Style},
    text::{Line, Span},
};

/// Handle an action while in RebaseConflict mode.
pub fn handle_conflict_key(action: KeyCommand, app: &mut AppState) -> AppAction {
    match action {
        KeyCommand::Confirm => {
            if let AppMode::RebaseConflict(state) =
                std::mem::replace(&mut app.mode, AppMode::CommitList)
            {
                AppAction::RebaseContinue(*state)
            } else {
                AppAction::Handled
            }
        }
        KeyCommand::Mergetool => {
            if let AppMode::RebaseConflict(ref state) = app.mode {
                AppAction::RunMergetool {
                    files: state.conflicting_files.clone(),
                    conflict_state: state.as_ref().clone(),
                }
            } else {
                AppAction::Handled
            }
        }
        KeyCommand::OpenEditor => {
            if let AppMode::RebaseConflict(ref state) = app.mode {
                AppAction::RunEditor {
                    files: state.conflicting_files.clone(),
                    conflict_state: state.as_ref().clone(),
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
            if let AppMode::RebaseConflict(state) =
                std::mem::replace(&mut app.mode, AppMode::CommitList)
            {
                AppAction::RebaseAbort(*state)
            } else {
                AppAction::Handled
            }
        }
        _ => {
            handle_dialog_scroll(action, app);
            AppAction::Handled
        }
    }
}

/// Render the conflict resolution dialog as a centered overlay.
///
/// Used by any operation (drop, squash, etc.) that may hit a merge conflict
/// during cherry-pick. The dialog title and body text adapt to the
/// `operation_label` stored in `ConflictState`.
pub fn render_conflict(app: &mut AppState, frame: &mut Frame) {
    let state = match &app.mode {
        AppMode::RebaseConflict(s) => s,
        _ => return,
    };

    let short_oid = state.conflicting_commit_oid.short();

    let label = &state.operation_label;
    let label_lower = label.to_lowercase();

    // Look up the commit summary from the loaded commit list so the user can
    // see which commit is conflicting without having to remember the OID.
    let commit_summary = app
        .commits
        .iter()
        .find(|c| c.oid.as_oid() == Some(&state.conflicting_commit_oid))
        .map(|c| c.summary.as_str())
        .unwrap_or("");

    const PREFERRED_WIDTH: u16 = 62;
    let iw = inner_width(PREFERRED_WIDTH, frame.area().width);

    let remaining = state.remaining_oids.len();

    let mut dialog = Dialog::new(DialogKind::Danger)
        .heading(
            format!("Merge conflict during {label_lower}"),
            TextRole::Danger,
        )
        .push_line(Line::from(vec![
            Span::raw(" Conflict in "),
            Span::styled(short_oid.to_string(), Style::default().fg(Color::Cyan)),
        ]));

    if !commit_summary.is_empty() {
        dialog = dialog.wrapped(commit_summary, iw.saturating_sub(1));
    }

    // For move operations, clarify whether the moved commit itself conflicted
    // or a commit being rebased on top of it.
    if let Some(ref moved_oid) = state.moved_commit_oid {
        let note = if state.conflicting_commit_oid == *moved_oid {
            " The moved commit itself caused the conflict."
        } else {
            " A commit being rebased on top of the moved commit conflicted."
        };
        dialog = dialog.wrapped_styled(note, iw, TextRole::Highlight);
    } else if state.operation_label == "Squash" {
        let note = if state.squash_context.is_some() {
            " The squash itself caused the conflict."
        } else {
            " A commit being rebased after the squash conflicted."
        };
        dialog = dialog.wrapped_styled(note, iw, TextRole::Highlight);
    }

    if remaining > 0 {
        let note = format!(" ({remaining} commit(s) still to rebase after this)");
        dialog = dialog.wrapped_indent(&note, iw);
    }

    if !state.conflicting_files.is_empty() {
        dialog = dialog
            .blank()
            .styled_line("Conflicting files:", TextRole::Highlight);
        const MAX_FILES: usize = 5;
        let shown = state.conflicting_files.len().min(MAX_FILES);
        for path in &state.conflicting_files[..shown] {
            let truncated = if path.len() + 3 > iw {
                format!(" \u{2026}{}", &path[path.len().saturating_sub(iw - 3)..])
            } else {
                path.to_string()
            };
            dialog = dialog.styled_line(truncated, TextRole::Danger);
        }
        let extra = state.conflicting_files.len().saturating_sub(MAX_FILES);
        if extra > 0 {
            dialog = dialog.styled_line(format!("... {extra} more"), TextRole::Muted);
        }
    }

    dialog = dialog.blank();
    if state.still_unresolved {
        dialog = dialog
            .wrapped_styled_bold(
                " ! Still unresolved — fix all conflicts above before continuing",
                iw,
                TextRole::Danger,
            )
            .blank();
    }
    dialog = dialog
        .instructions(&[
            ("Enter", Color::Green, "Continue"),
            ("m", Color::Cyan, "Mergetool"),
            ("e", Color::Cyan, "Editor"),
            ("Esc", Color::Red, "Abort"),
        ])
        .blank();

    let title = format!("{label} Conflict");
    let (max_scroll, visible_height) =
        dialog.render(frame, &title, PREFERRED_WIDTH, app.dialog_scroll_offset);
    app.max_dialog_scroll = max_scroll;
    app.dialog_visible_height = visible_height;
}
