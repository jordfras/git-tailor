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

// File picker dialog for the "split out file" split strategy.

use super::dialog::{Dialog, DialogKind, TextRole};
use super::list_nav::{self, ListNav};
use crate::app::{AppAction, AppMode, AppState, KeyCommand};
use ratatui::{
    Frame,
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

/// Handle an action while in SplitFileSelect mode.
pub fn handle_key(action: KeyCommand, app: &mut AppState) -> AppAction {
    let (commit_oid, file_count, file_index) = match &app.mode {
        AppMode::SplitFileSelect {
            commit_oid,
            files,
            file_index,
        } => (commit_oid.clone(), files.len(), *file_index),
        _ => return AppAction::Handled,
    };

    let mut cursor = file_index;
    match list_nav::handle_list_navigation(action, &mut cursor, file_count, file_count, false) {
        ListNav::Moved => {
            if let AppMode::SplitFileSelect { file_index, .. } = &mut app.mode {
                *file_index = cursor;
            }
            scroll_to_file(app, cursor);
            AppAction::Handled
        }
        ListNav::Confirmed => {
            let file_path = match &app.mode {
                AppMode::SplitFileSelect { files, .. } => files.get(file_index).cloned(),
                _ => None,
            };
            let Some(file_path) = file_path else {
                return AppAction::Handled;
            };
            app.mode = AppMode::CommitList;
            AppAction::ExecuteSplitOutFile {
                commit_oid,
                file_path,
            }
        }
        ListNav::Cancelled => {
            app.cancel_split_file_select();
            AppAction::Handled
        }
        ListNav::Help => {
            app.toggle_help();
            AppAction::Handled
        }
        ListNav::Unhandled => AppAction::Handled,
    }
}

/// Scroll `dialog_scroll_offset` so that the file row at `index` is visible.
/// The layout is: 4 header lines, then 1 line per file.
fn scroll_to_file(app: &mut AppState, index: usize) {
    const HEADER_LINES: usize = 4;
    let item = HEADER_LINES + index;
    let vh = app.dialog_visible_height;
    if vh == 0 {
        return;
    }
    if item < app.dialog_scroll_offset {
        app.dialog_scroll_offset = item;
    } else if item >= app.dialog_scroll_offset + vh {
        app.dialog_scroll_offset = item + 1 - vh;
    }
    app.dialog_scroll_offset = app.dialog_scroll_offset.min(app.max_dialog_scroll);
}

/// Render the "split out file" file picker as a centered overlay.
pub fn render(app: &mut AppState, frame: &mut Frame) {
    let (files, file_index) = match &app.mode {
        AppMode::SplitFileSelect {
            files, file_index, ..
        } => (files.clone(), *file_index),
        _ => return,
    };

    let mut dialog = Dialog::new(DialogKind::Info)
        .blank()
        .heading("Choose file to split out:", TextRole::Highlight)
        .blank();

    for (i, file) in files.iter().enumerate() {
        let selected = i == file_index;
        let marker = if selected { "▸ " } else { "  " };
        let style = if selected {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        dialog = dialog.push_line(Line::from(Span::styled(
            format!(" {}  {}", marker, file),
            style,
        )));
    }

    dialog = dialog
        .blank()
        .instructions(&[
            ("Enter", Color::Cyan, "Select"),
            ("Esc", Color::Cyan, "Cancel"),
        ])
        .blank();

    let content_width = 50;
    let (max_scroll, visible_height) = dialog.render(
        frame,
        "Split Out File",
        content_width,
        app.dialog_scroll_offset,
    );
    app.max_dialog_scroll = max_scroll;
    app.dialog_visible_height = visible_height;
}
