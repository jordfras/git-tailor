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

use super::dialog::Dialog;
use super::list_nav::{self, ListNav};
use crate::app::{AppAction, AppMode, AppState, KeyCommand, SplitStrategy};
use ratatui::{
    Frame,
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

/// Handle an action while in SplitSelect mode.
pub fn handle_key(action: KeyCommand, app: &mut AppState) -> AppAction {
    let strategy_index = match app.mode {
        AppMode::SplitSelect { strategy_index } => strategy_index,
        _ => return AppAction::Handled,
    };

    let len = SplitStrategy::ALL.len();
    let mut cursor = strategy_index;

    match list_nav::handle_list_navigation(action, &mut cursor, len, len, false) {
        ListNav::Moved => {
            app.mode = AppMode::SplitSelect {
                strategy_index: cursor,
            };
            AppAction::Handled
        }
        ListNav::Confirmed => {
            let strategy = SplitStrategy::ALL[strategy_index];
            let commit_oid = app.commits[app.selection_index]
                .oid
                .as_oid()
                .unwrap()
                .clone();
            app.mode = AppMode::CommitList;
            AppAction::PrepareSplit {
                strategy,
                commit_oid,
            }
        }
        ListNav::Cancelled => {
            app.mode = AppMode::CommitList;
            AppAction::Handled
        }
        ListNav::Help => {
            app.toggle_help();
            AppAction::Handled
        }
        ListNav::Unhandled => AppAction::Handled,
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
        .map(|c| format!("{} {}", c.oid.short(), c.summary))
        .unwrap_or_default();

    // Truncate summary if too long for dialog
    let max_summary_len = 44;
    let display_summary = if commit_summary.len() > max_summary_len {
        format!("{}…", &commit_summary[..max_summary_len - 1])
    } else {
        commit_summary
    };

    let strategy_index = match app.mode {
        AppMode::SplitSelect { strategy_index } => strategy_index,
        _ => 0,
    };

    let mut dialog = Dialog::new()
        .blank()
        .push_line(Line::from(Span::styled(
            format!(" {display_summary}"),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::DIM),
        )))
        .title(" Choose split strategy:", Color::Yellow);

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

        dialog = dialog
            .push_line(Line::from(Span::styled(
                format!(" {}  {}", marker, strategy.label()),
                style,
            )))
            .styled_line(
                format!("        {}", strategy.description()),
                Color::DarkGray,
            )
            .blank();
    }

    dialog = dialog
        .instructions(&[
            ("Enter", Color::Cyan, "Select"),
            ("Esc", Color::Cyan, "Cancel"),
        ])
        .blank();

    let content_width = 50;
    dialog.render(frame, "Split Commit", Color::Cyan, content_width, 0);
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

    Dialog::new()
        .title(
            format!(
                " This will create {} commits ({}).",
                pending.count, strategy_name
            ),
            Color::Yellow,
        )
        .plain(" Do you want to proceed?")
        .blank()
        .instructions(&[
            ("Enter", Color::Cyan, "Confirm"),
            ("Esc", Color::Cyan, "Cancel"),
        ])
        .blank()
        .render(frame, "Confirm Split", Color::Yellow, 52, 0);
}
