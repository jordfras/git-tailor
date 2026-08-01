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

use super::dialog::{Dialog, DialogKind, TextRole, handle_dialog_scroll};
use super::list_nav::{self, ListNav};
use crate::app::{AppAction, AppMode, AppState, KeyCommand, SplitStrategy};
use crate::repo::DEFAULT_CONTEXT_LINES;
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
            let commit_oid = app.list.commits[app.list.selection_index]
                .oid
                .expect_real_oid();
            // "Split out file(s)" and "split out hunk(s)" each need a second
            // dialog to choose what to peel out, so they take their own flow
            // rather than the count/confirm split path.
            app.mode = AppMode::CommitList;
            match strategy {
                SplitStrategy::OutFiles => AppAction::PrepareSplitOutFiles { commit_oid },
                SplitStrategy::OutHunks => AppAction::PrepareSplitOutHunks {
                    commit_oid,
                    context_lines: DEFAULT_CONTEXT_LINES,
                },
                _ => AppAction::PrepareSplit {
                    strategy,
                    commit_oid,
                },
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

/// Scroll `dialog.offset` so that the strategy row at `index` is
/// visible. The dialog layout is: 5 header lines, then 3 lines per strategy
/// (label + description + blank). Uses the visible height from the previous
/// render frame; does nothing if that hasn't been computed yet.
fn scroll_to_strategy(app: &mut AppState, index: usize) {
    const HEADER_LINES: usize = 5;
    const ITEM_HEIGHT: usize = 3;
    let item_top = HEADER_LINES + index * ITEM_HEIGHT;
    let item_bottom = item_top + ITEM_HEIGHT - 1;
    let vh = app.dialog.visible_height;
    if vh == 0 {
        return;
    }
    if item_top < app.dialog.offset {
        app.dialog.offset = item_top;
    } else if item_bottom >= app.dialog.offset + vh {
        app.dialog.offset = item_bottom + 1 - vh;
    }
    app.dialog.clamp_offset();
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
            handle_dialog_scroll(action, &mut app.dialog);
            AppAction::Handled
        }
    }
}

/// Render the split strategy selection dialog as a centered overlay.
pub fn render(app: &mut AppState, frame: &mut Frame) {
    let commit_summary = app
        .list
        .selected()
        .map(|c| format!("{} {}", c.oid.short(), c.summary))
        .unwrap_or_default();

    // Truncate summary if too long for dialog. Count/cut by characters, not
    // bytes, so a non-ASCII summary never slices mid-codepoint (which panics).
    let max_summary_len = 44;
    let display_summary = if commit_summary.chars().count() > max_summary_len {
        let prefix: String = commit_summary.chars().take(max_summary_len - 1).collect();
        format!("{prefix}…")
    } else {
        commit_summary
    };

    let strategy_index = match app.mode {
        AppMode::SplitSelect { strategy_index } => strategy_index,
        _ => 0,
    };

    let mut dialog = Dialog::new(DialogKind::Info, app.colors)
        .blank()
        .push_line(Line::from(Span::styled(
            format!(" {display_summary}"),
            Style::default()
                .fg(app.colors.resolve(Color::White))
                .add_modifier(Modifier::DIM),
        )))
        .heading("Choose split strategy:", TextRole::Highlight);

    for (i, strategy) in SplitStrategy::ALL.iter().enumerate() {
        let selected = i == strategy_index;
        let marker = if selected { "▸ " } else { "  " };
        let style = if selected {
            Style::default()
                .fg(app.colors.resolve(Color::Cyan))
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(app.colors.resolve(Color::White))
        };

        dialog = dialog
            .push_line(Line::from(Span::styled(
                format!(" {}  {}", marker, strategy.label()),
                style,
            )))
            .styled_line(
                format!("       {}", strategy.description()),
                TextRole::Muted,
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
    let (max_scroll, visible_height) =
        dialog.render(frame, "Split Commit", content_width, app.dialog.offset);
    app.dialog.set_bounds(max_scroll, visible_height);
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
        // Neither ever reaches the large-split confirmation dialog ("split out
        // file(s)"/"split out hunks" always produce exactly two commits, and
        // neither goes through PrepareSplit/SplitConfirm at all), but the
        // match must be total.
        crate::app::SplitStrategy::OutFiles => "split out files",
        crate::app::SplitStrategy::OutHunks => "split out hunks",
    };

    let (max_scroll, visible_height) = Dialog::new(DialogKind::Confirm, app.colors)
        .heading(
            format!(
                "This will create {} commits ({}).",
                pending.count, strategy_name
            ),
            TextRole::Highlight,
        )
        .plain(" Do you want to proceed?")
        .blank()
        .instructions(&[
            ("Enter", Color::Cyan, "Confirm"),
            ("Esc", Color::Cyan, "Cancel"),
        ])
        .blank()
        .render(frame, "Confirm Split", 52, app.dialog.offset);
    app.dialog.set_bounds(max_scroll, visible_height);
}
