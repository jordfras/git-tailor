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

// Squash target selection dialog

use super::dialog::{inner_width, render_centered_dialog, wrap_text};
use crate::app::{AppAction, AppMode, AppState};
use crate::event::KeyCommand;
use ratatui::{
    layout::Alignment,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    Frame,
};

/// Handle an action while in SquashSelect mode.
///
/// The user navigates the commit list to pick a squash target. The source
/// commit (from `source_index`) will be squashed *into* the chosen target.
pub fn handle_key(action: KeyCommand, app: &mut AppState) -> AppAction {
    match action {
        KeyCommand::MoveUp => {
            if app.reverse {
                app.move_down();
            } else {
                app.move_up();
            }
            AppAction::Handled
        }
        KeyCommand::MoveDown => {
            if app.reverse {
                app.move_up();
            } else {
                app.move_down();
            }
            AppAction::Handled
        }
        KeyCommand::PageUp => {
            let h = app.commit_list_visible_height;
            if app.reverse {
                app.page_down(h);
            } else {
                app.page_up(h);
            }
            AppAction::Handled
        }
        KeyCommand::PageDown => {
            let h = app.commit_list_visible_height;
            if app.reverse {
                app.page_up(h);
            } else {
                app.page_down(h);
            }
            AppAction::Handled
        }
        KeyCommand::Confirm => {
            let source_index = match app.mode {
                AppMode::SquashSelect { source_index } => source_index,
                _ => return AppAction::Handled,
            };
            let target_index = app.selection_index;

            // Cannot squash onto itself
            if target_index == source_index {
                app.set_error_message("Cannot squash a commit into itself");
                return AppAction::Handled;
            }

            // Cannot squash onto staged/unstaged
            let target = &app.commits[target_index];
            if target.oid == "staged" || target.oid == "unstaged" {
                app.set_error_message("Cannot squash into staged/unstaged changes");
                return AppAction::Handled;
            }

            let source = &app.commits[source_index];
            let result = AppAction::PrepareSquash {
                source_oid: source.oid.clone(),
                target_oid: target.oid.clone(),
                source_message: source.message.clone(),
                target_message: target.message.clone(),
            };

            app.mode = AppMode::CommitList;
            result
        }
        KeyCommand::ShowHelp => {
            app.toggle_help();
            AppAction::Handled
        }
        KeyCommand::Quit => {
            app.cancel_squash_select();
            AppAction::Handled
        }
        _ => AppAction::Handled,
    }
}

/// Render the squash target selection as a banner at the top of the screen.
///
/// This overlays a small info box showing which commit is being squashed and
/// instructions. The commit list underneath is rendered by the overlay system
/// (background mode), so the user can navigate it normally.
pub fn render(app: &AppState, frame: &mut Frame) {
    let source_index = match app.mode {
        AppMode::SquashSelect { source_index } => source_index,
        _ => return,
    };

    let source = match app.commits.get(source_index) {
        Some(c) => c,
        None => return,
    };

    let short_oid = if source.oid.len() >= 10 {
        &source.oid[..10]
    } else {
        &source.oid
    };

    const PREFERRED_WIDTH: u16 = 60;
    let iw = inner_width(PREFERRED_WIDTH, frame.area().width);

    let summary_chunks = wrap_text(&source.summary, iw.saturating_sub(1));

    let mut lines: Vec<Line> = vec![
        Line::from(""),
        Line::from(Span::styled(
            " Squash this commit into…",
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
    lines.push(Line::from(Span::styled(
        " Navigate to a target commit and press Enter.",
        Style::default().fg(Color::DarkGray),
    )));
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
        " Squash Commit ",
        Color::Magenta,
        PREFERRED_WIDTH,
        lines,
    );
}
