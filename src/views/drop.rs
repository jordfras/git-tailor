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

// Drop commit dialogs: confirmation and conflict resolution

use super::dialog::{inner_width, render_centered_dialog, wrap_text};
use crate::app::{AppAction, AppMode, AppState};
use crate::event::KeyCommand;
use ratatui::{
    layout::Alignment,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    Frame,
};

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
        _ => AppAction::Handled,
    }
}

/// Handle an action while in DropConflict mode.
pub fn handle_conflict_key(action: KeyCommand, app: &mut AppState) -> AppAction {
    match action {
        KeyCommand::Confirm => {
            if let AppMode::DropConflict(state) =
                std::mem::replace(&mut app.mode, AppMode::CommitList)
            {
                AppAction::ContinueDrop(state)
            } else {
                AppAction::Handled
            }
        }
        KeyCommand::Mergetool => {
            if let AppMode::DropConflict(ref state) = app.mode {
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
            if let AppMode::DropConflict(state) =
                std::mem::replace(&mut app.mode, AppMode::CommitList)
            {
                AppAction::AbortDrop(state)
            } else {
                AppAction::Handled
            }
        }
        _ => AppAction::Handled,
    }
}

/// Render the drop confirmation dialog as a centered overlay.
pub fn render_drop_confirm(app: &AppState, frame: &mut Frame) {
    let pending = match &app.mode {
        AppMode::DropConfirm(p) => p,
        _ => return,
    };

    let short_oid = if pending.commit_oid.len() >= 10 {
        &pending.commit_oid[..10]
    } else {
        &pending.commit_oid
    };

    const PREFERRED_WIDTH: u16 = 60;
    let iw = inner_width(PREFERRED_WIDTH, frame.area().width);

    // Pre-wrap the summary so that lines.len() reflects the real rendered
    // height and dialog_height is computed accurately.
    let summary_chunks = wrap_text(&pending.commit_summary, iw.saturating_sub(1));

    let mut lines: Vec<Line> = vec![
        Line::from(""),
        Line::from(Span::styled(
            " Drop this commit?",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            format!(" {short_oid}"),
            Style::default().fg(Color::Cyan),
        )),
    ];
    for chunk in &summary_chunks {
        lines.push(Line::from(Span::raw(format!(" {chunk}"))));
    }
    lines.push(Line::from(""));
    lines.push(
        Line::from(vec![
            Span::styled("Enter ", Style::default().fg(Color::Cyan)),
            Span::raw("Confirm   "),
            Span::styled("Esc ", Style::default().fg(Color::Cyan)),
            Span::raw("Cancel"),
        ])
        .alignment(Alignment::Center),
    );
    lines.push(Line::from(""));

    render_centered_dialog(
        frame,
        " Confirm Drop ",
        Color::Yellow,
        PREFERRED_WIDTH,
        lines,
    );
}

/// Render the drop-conflict resolution dialog as a centered overlay.
///
/// Shown when a drop operation hits a merge conflict. The user must resolve
/// the conflict in the working tree, then press Enter to continue or Esc to
/// abort the entire drop operation (even if this is not the first conflict).
pub fn render_drop_conflict(app: &AppState, frame: &mut Frame) {
    let state = match &app.mode {
        AppMode::DropConflict(s) => s,
        _ => return,
    };

    let short_oid = if state.conflicting_commit_oid.len() >= 10 {
        &state.conflicting_commit_oid[..10]
    } else {
        &state.conflicting_commit_oid
    };

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
            " Merge conflict during drop",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::raw(" Conflict in "),
            Span::styled(short_oid, Style::default().fg(Color::Cyan)),
        ]),
    ];

    // Show the summary of the conflicting commit, wrapped to fit the dialog.
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

    // List conflicting files so the user knows what to resolve.
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
            Span::raw("Abort entire drop"),
        ])
        .alignment(Alignment::Center),
    );
    lines.push(Line::from(""));

    render_centered_dialog(frame, " Drop Conflict ", Color::Red, PREFERRED_WIDTH, lines);
}
