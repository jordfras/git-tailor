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

// Split strategy selection dialog

use super::dialog::render_centered_dialog;
use crate::app::{AppAction, AppMode, AppState, SplitStrategy};
use crate::event::KeyCommand;
use ratatui::{
    layout::Alignment,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    Frame,
};

/// Handle an action while in SplitSelect mode.
pub fn handle_key(action: KeyCommand, app: &mut AppState) -> AppAction {
    match action {
        KeyCommand::MoveUp => {
            app.split_select_up();
            AppAction::Handled
        }
        KeyCommand::MoveDown => {
            app.split_select_down();
            AppAction::Handled
        }
        KeyCommand::Confirm => {
            let strategy = app.selected_split_strategy();
            let commit_oid = app.commits[app.selection_index].oid.clone();
            app.mode = AppMode::CommitList;
            AppAction::PrepareSplit {
                strategy,
                commit_oid,
            }
        }
        KeyCommand::ShowHelp => {
            app.toggle_help();
            AppAction::Handled
        }
        KeyCommand::Quit => {
            app.mode = AppMode::CommitList;
            AppAction::Handled
        }
        _ => AppAction::Handled,
    }
}

/// Handle an action while in SplitConfirm mode.
pub fn handle_confirm_key(action: KeyCommand, app: &mut AppState) -> AppAction {
    match action {
        KeyCommand::Confirm => {
            if let AppMode::SplitConfirm(pending) =
                std::mem::replace(&mut app.mode, AppMode::CommitList)
            {
                AppAction::ExecuteSplit {
                    strategy: pending.strategy,
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
            app.cancel_split_confirm();
            AppAction::Handled
        }
        _ => AppAction::Handled,
    }
}

/// Render the split strategy selection dialog as a centered overlay.
pub fn render(app: &AppState, frame: &mut Frame) {
    let commit_summary = app
        .commits
        .get(app.selection_index)
        .map(|c| {
            let short_oid = if c.oid.len() > 10 {
                &c.oid[..10]
            } else {
                &c.oid
            };
            format!("{} {}", short_oid, c.summary)
        })
        .unwrap_or_default();

    // Truncate summary if too long for dialog
    let max_summary_len = 44;
    let display_summary = if commit_summary.len() > max_summary_len {
        format!("{}…", &commit_summary[..max_summary_len - 1])
    } else {
        commit_summary
    };

    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!(" {}", display_summary),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::DIM),
        )),
        Line::from(""),
        Line::from(Span::styled(
            " Choose split strategy:",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    let strategy_index = match app.mode {
        AppMode::SplitSelect { strategy_index } => strategy_index,
        _ => 0,
    };
    for (i, strategy) in SplitStrategy::ALL.iter().enumerate() {
        let selected = i == strategy_index;
        let marker = if selected { "▸ " } else { "  " };
        let style = if selected {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };

        lines.push(Line::from(Span::styled(
            format!(" {}  {}", marker, strategy.label()),
            style,
        )));

        let desc_style = Style::default().fg(Color::DarkGray);
        lines.push(Line::from(Span::styled(
            format!("        {}", strategy.description()),
            desc_style,
        )));
        lines.push(Line::from(""));
    }

    lines.push(
        Line::from(vec![
            Span::styled("Enter ", Style::default().fg(Color::Cyan)),
            Span::raw("Select   "),
            Span::styled("Esc ", Style::default().fg(Color::Cyan)),
            Span::raw("Cancel"),
        ])
        .alignment(Alignment::Center),
    );
    lines.push(Line::from(""));

    let content_width = 50;

    render_centered_dialog(frame, " Split Commit ", Color::Cyan, content_width, lines);
}

/// Render the large-split confirmation dialog as a centered overlay.
pub fn render_split_confirm(app: &AppState, frame: &mut Frame) {
    let pending = match &app.mode {
        AppMode::SplitConfirm(p) => p,
        _ => return,
    };
    let strategy_name = match pending.strategy {
        crate::app::SplitStrategy::PerFile => "per file",
        crate::app::SplitStrategy::PerHunk => "per hunk",
        crate::app::SplitStrategy::PerHunkGroup => "per hunk group",
    };

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!(
                " This will create {} commits ({}).",
                pending.count, strategy_name
            ),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::raw(" Do you want to proceed?")),
        Line::from(""),
        Line::from(vec![
            Span::styled("Enter ", Style::default().fg(Color::Cyan)),
            Span::raw("Confirm   "),
            Span::styled("Esc ", Style::default().fg(Color::Cyan)),
            Span::raw("Cancel"),
        ])
        .alignment(Alignment::Center),
        Line::from(""),
    ];

    render_centered_dialog(frame, " Confirm Split ", Color::Yellow, 52, lines);
}
