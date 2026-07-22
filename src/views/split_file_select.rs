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

/// Dialog content width passed to [`Dialog::render`].
const CONTENT_WIDTH: u16 = 50;

/// Display columns available for a file path on one row: the dialog content
/// width minus its borders (2), the " ▸   " marker/indent (5), and one column
/// reserved for the scrollbar.
const PATH_WIDTH: usize = CONTENT_WIDTH as usize - 8;

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

    let mut dialog = Dialog::new(DialogKind::Info, app.colors)
        .blank()
        .heading("Choose file to split out:", TextRole::Highlight)
        .blank();

    for (i, file) in files.iter().enumerate() {
        let selected = i == file_index;
        let marker = if selected { "▸ " } else { "  " };
        let style = if selected {
            Style::default()
                .fg(app.colors.resolve(Color::Cyan))
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(app.colors.resolve(Color::White))
        };
        dialog = dialog.push_line(Line::from(Span::styled(
            format!(" {}  {}", marker, elide_path(file, PATH_WIDTH)),
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

    let (max_scroll, visible_height) = dialog.render(
        frame,
        "Split Out File",
        CONTENT_WIDTH,
        app.dialog_scroll_offset,
    );
    app.max_dialog_scroll = max_scroll;
    app.dialog_visible_height = visible_height;
}

/// Shorten `path` to at most `max` display columns for the picker by eliding
/// leading directory components (front ellipsis), so the filename and its
/// nearest parents — the parts that identify the file — stay visible.
///
/// Elision happens on `/` boundaries: the result is `…/<tail>` where `<tail>`
/// is the longest run of whole trailing components that fits. If even the
/// basename is too wide, the path's trailing characters are kept with a
/// leading ellipsis as a last resort.
pub(super) fn elide_path(path: &str, max: usize) -> String {
    let total = path.chars().count();
    if total <= max {
        return path.to_string();
    }

    // Byte indices where each component starts (just after every '/').
    let mut starts = vec![0usize];
    for (i, c) in path.char_indices() {
        if c == '/' {
            starts.push(i + c.len_utf8());
        }
    }

    // From the longest tail to the shortest, return the first `…/<tail>` that
    // fits. `starts[0]` (the whole path) is skipped — it already doesn't fit.
    for &start in starts.iter().skip(1) {
        let suffix = &path[start..];
        // 2 columns for the leading "…/".
        if 2 + suffix.chars().count() <= max {
            return format!("…/{suffix}");
        }
    }

    // Even the basename does not fit: keep the trailing chars after one '…'.
    let keep = max.saturating_sub(1);
    let tail: String = path.chars().skip(total - keep).collect();
    format!("…{tail}")
}

#[cfg(test)]
mod tests {
    use super::elide_path;

    // "aa/bb/cc/dd.rs" is 14 columns; components start at 0, 3, 6, 9.

    #[test]
    fn keeps_path_that_fits() {
        assert_eq!(elide_path("aa/bb/cc/dd.rs", 14), "aa/bb/cc/dd.rs");
        assert_eq!(elide_path("src/a.rs", 20), "src/a.rs");
    }

    #[test]
    fn elides_leading_components_on_boundary() {
        assert_eq!(elide_path("aa/bb/cc/dd.rs", 13), "…/bb/cc/dd.rs");
        assert_eq!(elide_path("aa/bb/cc/dd.rs", 10), "…/cc/dd.rs");
    }

    #[test]
    fn keeps_basename_when_middle_too_long() {
        assert_eq!(elide_path("aa/bb/cc/dd.rs", 9), "…/dd.rs");
    }

    #[test]
    fn truncates_basename_as_last_resort() {
        // Budget too small even for "…/dd.rs": keep trailing chars after '…'.
        let out = elide_path("aa/bb/cc/dd.rs", 6);
        assert_eq!(out, "…dd.rs");
        assert!(out.chars().count() <= 6);
    }

    #[test]
    fn handles_path_without_separators() {
        let out = elide_path("verylongfilename.rs", 10);
        assert!(out.starts_with('…'));
        assert!(out.ends_with(".rs"));
        assert!(out.chars().count() <= 10);
    }

    #[test]
    fn result_never_exceeds_budget() {
        let p = "src/views/git2_impl/reads/extract_files.rs";
        for max in 4..=p.chars().count() {
            assert!(elide_path(p, max).chars().count() <= max, "max={max}");
        }
    }
}
