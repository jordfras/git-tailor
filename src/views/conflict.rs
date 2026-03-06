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

use super::dialog::{inner_width, render_centered_dialog, wrap_text};
use crate::app::{AppAction, AppMode, AppState};
use crate::event::KeyCommand;
use ratatui::{
    layout::Alignment,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    Frame,
};

/// Handle an action while in RebaseConflict mode.
pub fn handle_conflict_key(action: KeyCommand, app: &mut AppState) -> AppAction {
    match action {
        KeyCommand::Confirm => {
            if let AppMode::RebaseConflict(state) =
                std::mem::replace(&mut app.mode, AppMode::CommitList)
            {
                AppAction::RebaseContinue(state)
            } else {
                AppAction::Handled
            }
        }
        KeyCommand::Mergetool => {
            if let AppMode::RebaseConflict(ref state) = app.mode {
                AppAction::RunMergetool {
                    files: state.conflicting_files.clone(),
                    conflict_state: state.clone(),
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
                AppAction::RebaseAbort(state)
            } else {
                AppAction::Handled
            }
        }
        _ => AppAction::Handled,
    }
}

/// Render the conflict resolution dialog as a centered overlay.
///
/// Used by any operation (drop, squash, etc.) that may hit a merge conflict
/// during cherry-pick. The dialog title and body text adapt to the
/// `operation_label` stored in `ConflictState`.
pub fn render_conflict(app: &AppState, frame: &mut Frame) {
    let state = match &app.mode {
        AppMode::RebaseConflict(s) => s,
        _ => return,
    };

    let short_oid = if state.conflicting_commit_oid.len() >= 10 {
        &state.conflicting_commit_oid[..10]
    } else {
        &state.conflicting_commit_oid
    };

    let label = &state.operation_label;
    let label_lower = label.to_lowercase();

    // Look up the commit summary from the loaded commit list so the user can
    // see which commit is conflicting without having to remember the OID.
    let commit_summary = app
        .commits
        .iter()
        .find(|c| c.oid == state.conflicting_commit_oid)
        .map(|c| c.summary.as_str())
        .unwrap_or("");

    const PREFERRED_WIDTH: u16 = 62;
    let iw = inner_width(PREFERRED_WIDTH, frame.area().width);

    let remaining = state.remaining_oids.len();

    let mut lines: Vec<Line> = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!(" Merge conflict during {label_lower}"),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::raw(" Conflict in "),
            Span::styled(short_oid, Style::default().fg(Color::Cyan)),
        ]),
    ];

    if !commit_summary.is_empty() {
        for chunk in wrap_text(commit_summary, iw.saturating_sub(1)) {
            lines.push(Line::from(Span::raw(format!(" {chunk}"))));
        }
    }

    if remaining > 0 {
        let note = format!(" ({remaining} commit(s) still to rebase after this)");
        for chunk in wrap_text(&note, iw) {
            lines.push(Line::from(Span::raw(chunk)));
        }
    }

    if !state.conflicting_files.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            " Conflicting files:",
            Style::default().fg(Color::Yellow),
        )));
        const MAX_FILES: usize = 5;
        let shown = state.conflicting_files.len().min(MAX_FILES);
        for path in &state.conflicting_files[..shown] {
            let truncated = if path.len() + 3 > iw {
                format!(" \u{2026}{}", &path[path.len().saturating_sub(iw - 3)..])
            } else {
                format!(" {path}")
            };
            lines.push(Line::from(Span::styled(
                truncated,
                Style::default().fg(Color::Red),
            )));
        }
        let extra = state.conflicting_files.len().saturating_sub(MAX_FILES);
        if extra > 0 {
            lines.push(Line::from(Span::styled(
                format!(" ... {extra} more"),
                Style::default().fg(Color::DarkGray),
            )));
        }
    }

    lines.push(Line::from(""));
    if state.still_unresolved {
        lines.push(Line::from(Span::styled(
            " ! Still unresolved — fix all conflicts above before continuing",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));
    }
    lines.push(Line::from(Span::raw(
        " Resolve conflicts in your working tree, then:",
    )));
    lines.push(Line::from(""));
    lines.push(
        Line::from(vec![
            Span::styled("Enter ", Style::default().fg(Color::Green)),
            Span::raw("Continue   "),
            Span::styled("m ", Style::default().fg(Color::Cyan)),
            Span::raw("Mergetool   "),
            Span::styled("Esc ", Style::default().fg(Color::Red)),
            Span::raw(format!("Abort entire {label_lower}")),
        ])
        .alignment(Alignment::Center),
    );
    lines.push(Line::from(""));

    let title = format!(" {label} Conflict ");
    render_centered_dialog(frame, &title, Color::Red, PREFERRED_WIDTH, lines);
}
