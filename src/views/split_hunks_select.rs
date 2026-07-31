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

// Hunk picker dialog for the "split out hunk(s)" split strategy — a two-pane
// picker (see `two_pane_picker`) listing the commit's hunks on the left, with
// a diff preview of the highlighted one on the right.

use super::list_nav::{self, ListNav};
use super::two_pane_picker::{self, LIST_ROW_PREFIX_WIDTH};
use crate::DiffLineKind;
use crate::app::{AppAction, AppMode, AppState, KeyCommand};
use ratatui::{
    Frame,
    style::{Color, Style},
    text::{Line, Span},
};

/// Handle an action while in SplitHunksSelect mode.
pub fn handle_key(action: KeyCommand, app: &mut AppState) -> AppAction {
    let (commit_oid, hunk_count, hunk_index, context_lines) = match &app.mode {
        AppMode::SplitHunksSelect {
            commit_oid,
            hunks,
            hunk_index,
            context_lines,
            ..
        } => (commit_oid.clone(), hunks.len(), *hunk_index, *context_lines),
        _ => return AppAction::Handled,
    };

    let mut cursor = hunk_index;
    match list_nav::handle_list_navigation(action, &mut cursor, hunk_count, hunk_count, false) {
        ListNav::Moved => {
            if let AppMode::SplitHunksSelect {
                hunk_index,
                preview_h_scroll,
                preview_v_scroll,
                ..
            } = &mut app.mode
            {
                *hunk_index = cursor;
                *preview_h_scroll = 0;
                *preview_v_scroll = 0;
            }
            scroll_to_hunk(app, cursor);
            AppAction::Handled
        }
        ListNav::Confirmed => {
            let AppMode::SplitHunksSelect {
                hunks, selected, ..
            } = std::mem::replace(&mut app.mode, AppMode::CommitList)
            else {
                return AppAction::Handled;
            };
            // Nothing explicitly marked: fall back to just the hunk under the
            // cursor, so a single hunk can still be split out in one keypress.
            let chosen: Vec<(usize, usize)> = if selected.is_empty() {
                hunks
                    .get(hunk_index)
                    .map(|h| vec![(h.delta_idx, h.hunk_idx)])
                    .unwrap_or_default()
            } else {
                selected
                    .iter()
                    .filter_map(|&i| hunks.get(i).map(|h| (h.delta_idx, h.hunk_idx)))
                    .collect()
            };
            if chosen.is_empty() {
                return AppAction::Handled;
            }
            AppAction::ExecuteSplitOutHunks {
                commit_oid,
                hunks: chosen,
                context_lines,
            }
        }
        ListNav::Cancelled => {
            app.cancel_split_hunks_select();
            AppAction::Handled
        }
        ListNav::Help => {
            app.toggle_help();
            AppAction::Handled
        }
        ListNav::Unhandled => match action {
            KeyCommand::TogglePickerItem => {
                if let AppMode::SplitHunksSelect { selected, .. } = &mut app.mode
                    && !selected.remove(&hunk_index)
                {
                    selected.insert(hunk_index);
                }
                AppAction::Handled
            }
            // Horizontal scroll of the diff preview pane. Plain arrows already
            // move the hunk list cursor, so this reuses Left/Right (the same
            // keys the commit detail view uses for the same purpose); vertical
            // scroll below uses Ctrl-Up/Down instead for the same reason.
            // Both are clamped to content size at render time.
            KeyCommand::ScrollLeft => {
                if let AppMode::SplitHunksSelect {
                    preview_h_scroll, ..
                } = &mut app.mode
                {
                    *preview_h_scroll = preview_h_scroll.saturating_sub(1);
                }
                AppAction::Handled
            }
            KeyCommand::ScrollRight => {
                if let AppMode::SplitHunksSelect {
                    preview_h_scroll, ..
                } = &mut app.mode
                {
                    *preview_h_scroll = preview_h_scroll.saturating_add(1);
                }
                AppAction::Handled
            }
            KeyCommand::ScrollListUp => {
                if let AppMode::SplitHunksSelect {
                    preview_v_scroll, ..
                } = &mut app.mode
                {
                    *preview_v_scroll = preview_v_scroll.saturating_sub(1);
                }
                AppAction::Handled
            }
            KeyCommand::ScrollListDown => {
                if let AppMode::SplitHunksSelect {
                    preview_v_scroll, ..
                } = &mut app.mode
                {
                    *preview_v_scroll = preview_v_scroll.saturating_add(1);
                }
                AppAction::Handled
            }
            // Re-fetch the commit's diff at the new context level and rebuild
            // the hunk list from scratch — hunks merging or splitting apart
            // means row indices (and any selection) can't carry over safely.
            KeyCommand::IncreaseContext => AppAction::PrepareSplitOutHunks {
                commit_oid,
                context_lines: context_lines.saturating_add(1),
            },
            KeyCommand::DecreaseContext => AppAction::PrepareSplitOutHunks {
                commit_oid,
                context_lines: context_lines.saturating_sub(1),
            },
            _ => AppAction::Handled,
        },
    }
}

/// Scroll `dialog.offset` so the hunk row at `index` is visible in the
/// list pane (one line per hunk, no header offset — the list starts at the
/// top of its own pane).
fn scroll_to_hunk(app: &mut AppState, index: usize) {
    let vh = app.dialog.visible_height;
    if vh == 0 {
        return;
    }
    if index < app.dialog.offset {
        app.dialog.offset = index;
    } else if index >= app.dialog.offset + vh {
        app.dialog.offset = index + 1 - vh;
    }
    app.dialog.clamp_offset();
}

const HINT_ROWS: [&[(&str, &str)]; 2] = [
    &[("↑/↓", "Move"), ("Space", "Toggle"), ("+/-", "Context")],
    &[
        ("←/→ Ctrl-↑/↓", "Scroll"),
        ("Enter", "Split"),
        ("Esc", "Cancel"),
    ],
];

/// Render the hunk picker as a wide, centered two-pane overlay.
pub fn render(app: &mut AppState, frame: &mut Frame) {
    let (hunks, hunk_index, selected, context_lines, preview_h_scroll, preview_v_scroll) =
        match &app.mode {
            AppMode::SplitHunksSelect {
                hunks,
                hunk_index,
                selected,
                context_lines,
                preview_h_scroll,
                preview_v_scroll,
                ..
            } => (
                hunks.clone(),
                *hunk_index,
                selected.clone(),
                *context_lines,
                *preview_h_scroll,
                *preview_v_scroll,
            ),
            _ => return,
        };

    let title = format!("Split Out Hunk(s) — context: {context_lines}");
    let areas = two_pane_picker::render_frame(app, frame, &title, &HINT_ROWS);

    let budget =
        two_pane_picker::list_text_width(areas.list_area).saturating_sub(LIST_ROW_PREFIX_WIDTH);
    let labels: Vec<String> = hunks
        .iter()
        .map(|entry| {
            let suffix = format!(" -{},{}", entry.hunk.old_start, entry.hunk.old_lines);
            let path_budget = budget.saturating_sub(suffix.chars().count());
            format!(
                "{}{suffix}",
                two_pane_picker::elide_path(&entry.file_path, path_budget)
            )
        })
        .collect();
    two_pane_picker::render_list(app, frame, areas.list_area, &labels, hunk_index, &selected);

    let preview_lines = build_hunk_preview(app, hunks.get(hunk_index));
    let (h, v) = two_pane_picker::render_preview(
        frame,
        areas.preview_area,
        preview_lines,
        preview_h_scroll,
        preview_v_scroll,
    );
    if let AppMode::SplitHunksSelect {
        preview_h_scroll,
        preview_v_scroll,
        ..
    } = &mut app.mode
    {
        *preview_h_scroll = h;
        *preview_v_scroll = v;
    }
}

fn build_hunk_preview(
    app: &AppState,
    entry: Option<&crate::app::HunkPickerEntry>,
) -> Vec<Line<'static>> {
    let white = app.colors.resolve(Color::White);
    let mut lines = Vec::new();
    let Some(entry) = entry else {
        return lines;
    };
    lines.push(Line::from(Span::styled(
        format!("{}:", entry.file_path),
        Style::default().fg(app.colors.resolve(Color::Yellow)),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!(
            "@@ -{},{} +{},{} @@",
            entry.hunk.old_start, entry.hunk.old_lines, entry.hunk.new_start, entry.hunk.new_lines
        ),
        Style::default().fg(app.colors.resolve(Color::Cyan)),
    )));
    for diff_line in &entry.hunk.lines {
        let (prefix, style) = match diff_line.kind {
            DiffLineKind::Addition => ("+", Style::default().fg(app.colors.resolve(Color::Green))),
            DiffLineKind::Deletion => ("-", Style::default().fg(app.colors.resolve(Color::Red))),
            DiffLineKind::Context => (" ", Style::default().fg(white)),
        };
        let content = diff_line.content.trim_end_matches(['\n', '\r']);
        lines.push(Line::from(Span::styled(
            format!("{prefix}{content}"),
            style,
        )));
    }
    lines
}
