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

// File picker dialog for the "split out file(s)" split strategy — a two-pane
// picker (see `two_pane_picker`) listing the commit's changed files on the
// left, with a diff preview of the highlighted one on the right.

use super::list_nav::{self, ListNav};
use super::two_pane_picker::{self, LIST_ROW_PREFIX_WIDTH};
use crate::app::{AppAction, AppMode, AppState, KeyCommand};
use crate::{DeltaStatus, DiffLineKind, FileDiff};
use ratatui::{
    Frame,
    style::{Color, Style},
    text::{Line, Span},
};

/// Resolve a `FileDiff`'s display/identity path: the new path, falling back
/// to the old path for a deleted file.
fn file_path(file: &FileDiff) -> String {
    file.new_path
        .clone()
        .or_else(|| file.old_path.clone())
        .unwrap_or_default()
}

/// Single-character status indicator, matching the one used elsewhere in the
/// app for the same `DeltaStatus` values.
fn status_char(status: DeltaStatus) -> &'static str {
    match status {
        DeltaStatus::Added => "A",
        DeltaStatus::Deleted => "D",
        DeltaStatus::Renamed => "R",
        DeltaStatus::Copied => "C",
        DeltaStatus::Typechange => "T",
        _ => "M",
    }
}

/// Handle an action while in SplitFilesSelect mode.
pub fn handle_key(action: KeyCommand, app: &mut AppState) -> AppAction {
    let (commit_oid, file_count, file_index) = match &app.mode {
        AppMode::SplitFilesSelect {
            commit_oid,
            files,
            file_index,
            ..
        } => (commit_oid.clone(), files.len(), *file_index),
        _ => return AppAction::Handled,
    };

    let mut cursor = file_index;
    // Items are one row each, so the list pane's height is exactly how many
    // fit — a page steps by one screenful rather than to the end of the list.
    match list_nav::handle_list_navigation(
        action,
        &mut cursor,
        file_count,
        app.dialog.visible_height,
        false,
    ) {
        ListNav::Moved => {
            if let AppMode::SplitFilesSelect {
                file_index,
                preview_h_scroll,
                preview_v_scroll,
                ..
            } = &mut app.mode
            {
                *file_index = cursor;
                *preview_h_scroll = 0;
                *preview_v_scroll = 0;
            }
            app.dialog.ensure_visible(cursor, 1);
            AppAction::Handled
        }
        ListNav::Confirmed => {
            let AppMode::SplitFilesSelect {
                files, selected, ..
            } = std::mem::replace(&mut app.mode, AppMode::CommitList)
            else {
                return AppAction::Handled;
            };
            // Nothing explicitly marked: fall back to just the file under the
            // cursor, so a single file can still be split out in one keypress.
            let chosen: Vec<String> = if selected.is_empty() {
                files.get(file_index).map(file_path).into_iter().collect()
            } else {
                selected
                    .iter()
                    .filter_map(|&i| files.get(i).map(file_path))
                    .collect()
            };
            if chosen.is_empty() {
                return AppAction::Handled;
            }
            AppAction::ExecuteSplitOutFiles {
                commit_oid,
                file_paths: chosen,
            }
        }
        ListNav::Cancelled => {
            app.cancel_split_files_select();
            AppAction::Handled
        }
        ListNav::Help => {
            app.toggle_help();
            AppAction::Handled
        }
        ListNav::Unhandled => match action {
            KeyCommand::TogglePickerItem => {
                if let AppMode::SplitFilesSelect { selected, .. } = &mut app.mode
                    && !selected.remove(&file_index)
                {
                    selected.insert(file_index);
                }
                AppAction::Handled
            }
            KeyCommand::ScrollLeft => {
                if let AppMode::SplitFilesSelect {
                    preview_h_scroll, ..
                } = &mut app.mode
                {
                    *preview_h_scroll = preview_h_scroll.saturating_sub(1);
                }
                AppAction::Handled
            }
            KeyCommand::ScrollRight => {
                if let AppMode::SplitFilesSelect {
                    preview_h_scroll, ..
                } = &mut app.mode
                {
                    *preview_h_scroll = preview_h_scroll.saturating_add(1);
                }
                AppAction::Handled
            }
            KeyCommand::ScrollListUp => {
                if let AppMode::SplitFilesSelect {
                    preview_v_scroll, ..
                } = &mut app.mode
                {
                    *preview_v_scroll = preview_v_scroll.saturating_sub(1);
                }
                AppAction::Handled
            }
            KeyCommand::ScrollListDown => {
                if let AppMode::SplitFilesSelect {
                    preview_v_scroll, ..
                } = &mut app.mode
                {
                    *preview_v_scroll = preview_v_scroll.saturating_add(1);
                }
                AppAction::Handled
            }
            _ => AppAction::Handled,
        },
    }
}

const HINT_ROWS: [&[(&str, &str)]; 2] = [
    &[("↑/↓", "Move"), ("Space", "Toggle")],
    &[
        ("←/→ Ctrl-↑/↓", "Scroll"),
        ("Enter", "Split"),
        ("Esc", "Cancel"),
    ],
];

/// Render the file picker as a wide, centered two-pane overlay.
pub fn render(app: &mut AppState, frame: &mut Frame) {
    let (files, file_index, selected, preview_h_scroll, preview_v_scroll) = match &app.mode {
        AppMode::SplitFilesSelect {
            files,
            file_index,
            selected,
            preview_h_scroll,
            preview_v_scroll,
            ..
        } => (
            files.clone(),
            *file_index,
            selected.clone(),
            *preview_h_scroll,
            *preview_v_scroll,
        ),
        _ => return,
    };

    let areas = two_pane_picker::render_frame(app, frame, "Split Out File(s)", &HINT_ROWS);

    let budget =
        two_pane_picker::list_text_width(areas.list_area).saturating_sub(LIST_ROW_PREFIX_WIDTH);
    let labels: Vec<String> = files
        .iter()
        .map(|file| {
            let prefix = format!("{} ", status_char(file.status));
            let path_budget = budget.saturating_sub(prefix.chars().count());
            format!(
                "{prefix}{}",
                two_pane_picker::elide_path(&file_path(file), path_budget)
            )
        })
        .collect();
    two_pane_picker::render_list(app, frame, areas.list_area, &labels, file_index, &selected);

    let preview_lines = build_file_preview(app, files.get(file_index));
    let (h, v) = two_pane_picker::render_preview(
        frame,
        areas.preview_area,
        preview_lines,
        preview_h_scroll,
        preview_v_scroll,
    );
    if let AppMode::SplitFilesSelect {
        preview_h_scroll,
        preview_v_scroll,
        ..
    } = &mut app.mode
    {
        *preview_h_scroll = h;
        *preview_v_scroll = v;
    }
}

fn build_file_preview(app: &AppState, file: Option<&FileDiff>) -> Vec<Line<'static>> {
    let white = app.colors.resolve(Color::White);
    let mut lines = Vec::new();
    let Some(file) = file else {
        return lines;
    };
    lines.push(Line::from(Span::styled(
        format!("{}:", file_path(file)),
        Style::default().fg(app.colors.resolve(Color::Yellow)),
    )));
    lines.push(Line::from(""));
    if file.hunks.is_empty() {
        lines.push(Line::from(Span::styled(
            super::commit_detail::hunkless_marker(file),
            Style::default().fg(app.colors.resolve(Color::DarkGray)),
        )));
    }
    for hunk in &file.hunks {
        lines.push(Line::from(Span::styled(
            format!(
                "@@ -{},{} +{},{} @@",
                hunk.old_start, hunk.old_lines, hunk.new_start, hunk.new_lines
            ),
            Style::default().fg(app.colors.resolve(Color::Cyan)),
        )));
        for diff_line in &hunk.lines {
            let (prefix, style) = match diff_line.kind {
                DiffLineKind::Addition => {
                    ("+", Style::default().fg(app.colors.resolve(Color::Green)))
                }
                DiffLineKind::Deletion => {
                    ("-", Style::default().fg(app.colors.resolve(Color::Red)))
                }
                DiffLineKind::Context => (" ", Style::default().fg(white)),
            };
            let content = diff_line.content.trim_end_matches(['\n', '\r']);
            lines.push(Line::from(Span::styled(
                format!("{prefix}{content}"),
                style,
            )));
        }
    }
    lines
}
