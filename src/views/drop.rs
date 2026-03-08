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

use super::dialog::{inner_width, render_centered_dialog, wrap_text};
use crate::app::{AppAction, AppMode, AppState, KeyCommand};
use ratatui::{
    Frame,
    layout::Alignment,
    style::{Color, Modifier, Style},
    text::{Line, Span},
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
