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

use super::dialog::{Dialog, handle_dialog_scroll};
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
            scroll_to_strategy(app, cursor);
            AppAction::Handled
        }
        ListNav::Confirmed => {
            let strategy = SplitStrategy::ALL[strategy_index];
            let commit_oid = app.commits[app.selection_index].oid.expect_real_oid();
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

/// Scroll `dialog_scroll_offset` so that the strategy row at `index` is
/// visible. The dialog layout is: 5 header lines, then 3 lines per strategy
/// (label + description + blank). Uses the visible height from the previous
/// render frame; does nothing if that hasn't been computed yet.
fn scroll_to_strategy(app: &mut AppState, index: usize) {
    const HEADER_LINES: usize = 5;
    const ITEM_HEIGHT: usize = 3;
    let item_top = HEADER_LINES + index * ITEM_HEIGHT;
    let item_bottom = item_top + ITEM_HEIGHT - 1;
    let vh = app.dialog_visible_height;
    if vh == 0 {
        return;
    }
    if item_top < app.dialog_scroll_offset {
        app.dialog_scroll_offset = item_top;
    } else if item_bottom >= app.dialog_scroll_offset + vh {
        app.dialog_scroll_offset = item_bottom + 1 - vh;
    }
    app.dialog_scroll_offset = app.dialog_scroll_offset.min(app.max_dialog_scroll);
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
        _ => {
            handle_dialog_scroll(action, app);
            AppAction::Handled
        }
    }
}

/// Render the split strategy selection dialog as a centered overlay.
pub fn render(app: &mut AppState, frame: &mut Frame) {
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
    let (max_scroll, visible_height) = dialog.render(
        frame,
        "Split Commit",
        Color::Cyan,
        content_width,
        app.dialog_scroll_offset,
    );
    app.max_dialog_scroll = max_scroll;
    app.dialog_visible_height = visible_height;
}

/// Render the large-split confirmation dialog as a centered overlay.
pub fn render_split_confirm(app: &mut AppState, frame: &mut Frame) {
    let pending = match &app.mode {
        AppMode::SplitConfirm(p) => p,
        _ => return,
    };
    let strategy_name = match pending.strategy {
        crate::app::SplitStrategy::PerFile => "per file",
        crate::app::SplitStrategy::PerHunk => "per hunk",
        crate::app::SplitStrategy::PerHunkGroup => "per hunk group",
    };

    let (max_scroll, visible_height) = Dialog::new()
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
        .render(
            frame,
            "Confirm Split",
            Color::Yellow,
            52,
            app.dialog_scroll_offset,
        );
    app.max_dialog_scroll = max_scroll;
    app.dialog_visible_height = visible_height;
}
