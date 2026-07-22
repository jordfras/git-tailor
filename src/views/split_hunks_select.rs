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

// Hunk picker dialog for the "split out hunk(s)" split strategy: a wide
// two-pane overlay listing the commit's hunks on the left, with a diff
// preview of the highlighted one on the right — mirroring how the main
// window splits the commit list from the detail view.

use super::list_nav::{self, ListNav};
use super::split_file_select::elide_path;
use crate::DiffLineKind;
use crate::app::{AppAction, AppMode, AppState, KeyCommand};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
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
            KeyCommand::ToggleHunkSelect => {
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

/// Scroll `dialog_scroll_offset` so the hunk row at `index` is visible in the
/// list pane (one line per hunk, no header offset — the list starts at the
/// top of its own pane).
fn scroll_to_hunk(app: &mut AppState, index: usize) {
    let vh = app.dialog_visible_height;
    if vh == 0 {
        return;
    }
    if index < app.dialog_scroll_offset {
        app.dialog_scroll_offset = index;
    } else if index >= app.dialog_scroll_offset + vh {
        app.dialog_scroll_offset = index + 1 - vh;
    }
    app.dialog_scroll_offset = app.dialog_scroll_offset.min(app.max_dialog_scroll);
}

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

    let area = frame.area();
    let border_color = app.colors.resolve(Color::Cyan);
    let bg_color = app.colors.resolve(Color::Black);

    let width = area.width.saturating_sub(4).max(20).min(area.width);
    let height = area.height.saturating_sub(4).max(10).min(area.height);
    let dialog_area = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };

    frame.render_widget(Clear, dialog_area);
    let block = Block::default()
        .title(format!(" Split Out Hunk(s) — context: {context_lines} "))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(bg_color));
    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    // Two rows: the hint set is too wide to fit legibly on one line at
    // typical terminal widths.
    let [content_area, instructions_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(2)]).areas(inner);

    let list_width = (content_area.width / 3)
        .clamp(24, 42)
        .min(content_area.width);
    let [list_area, sep_area, preview_area] = Layout::horizontal([
        Constraint::Length(list_width),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .areas(content_area);

    render_separator(app, frame, sep_area);
    render_hunk_list(app, frame, list_area, &hunks, hunk_index, &selected);
    render_preview(
        app,
        frame,
        preview_area,
        hunks.get(hunk_index),
        preview_h_scroll,
        preview_v_scroll,
    );
    render_instructions(app, frame, instructions_area);
}

fn render_separator(app: &AppState, frame: &mut Frame, area: Rect) {
    let sep_style = Style::default().fg(app.colors.resolve(Color::White));
    let sep_lines: Vec<Line> = (0..area.height)
        .map(|_| Line::from(Span::styled("│", sep_style)))
        .collect();
    frame.render_widget(Paragraph::new(sep_lines), area);
}

fn render_hunk_list(
    app: &mut AppState,
    frame: &mut Frame,
    area: Rect,
    hunks: &[crate::app::HunkPickerEntry],
    hunk_index: usize,
    selected: &std::collections::HashSet<usize>,
) {
    let [text_area, scrollbar_area] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(1)]).areas(area);

    app.dialog_visible_height = text_area.height as usize;
    app.max_dialog_scroll = hunks.len().saturating_sub(text_area.height as usize);
    app.dialog_scroll_offset = app.dialog_scroll_offset.min(app.max_dialog_scroll);

    let inner_width = text_area.width as usize;
    let mut lines = Vec::with_capacity(hunks.len());
    for (i, entry) in hunks.iter().enumerate() {
        let cursor = i == hunk_index;
        let is_selected = selected.contains(&i);
        let marker = if cursor { "▸" } else { " " };
        let check = if is_selected { "[x]" } else { "[ ]" };
        let suffix = format!(" -{},{}", entry.hunk.old_start, entry.hunk.old_lines);
        let budget = inner_width.saturating_sub(
            marker.chars().count() + 1 + check.chars().count() + 1 + suffix.chars().count(),
        );
        let path = elide_path(&entry.file_path, budget);
        let style = if cursor {
            Style::default()
                .fg(app.colors.resolve(Color::Cyan))
                .add_modifier(Modifier::BOLD)
        } else if is_selected {
            Style::default().fg(app.colors.resolve(Color::Green))
        } else {
            Style::default().fg(app.colors.resolve(Color::White))
        };
        lines.push(Line::from(Span::styled(
            format!("{marker} {check} {path}{suffix}"),
            style,
        )));
    }

    let paragraph = Paragraph::new(lines).scroll((app.dialog_scroll_offset as u16, 0));
    frame.render_widget(paragraph, text_area);

    if app.max_dialog_scroll > 0 {
        render_vertical_scrollbar(
            frame,
            scrollbar_area,
            app.dialog_scroll_offset,
            app.max_dialog_scroll,
        );
    }
}

/// Render the diff preview, scrolled to `(v_scroll, h_scroll)` and clamped to
/// its actual content size — writing the clamped values back to `app.mode` so
/// repeatedly scrolling past the edge doesn't grow the stored offset unbounded.
fn render_preview(
    app: &mut AppState,
    frame: &mut Frame,
    area: Rect,
    entry: Option<&crate::app::HunkPickerEntry>,
    h_scroll: usize,
    v_scroll: usize,
) {
    let white = app.colors.resolve(Color::White);
    let mut lines = Vec::new();
    if let Some(entry) = entry {
        lines.push(Line::from(Span::styled(
            format!("{}:", entry.file_path),
            Style::default().fg(app.colors.resolve(Color::Yellow)),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!(
                "@@ -{},{} +{},{} @@",
                entry.hunk.old_start,
                entry.hunk.old_lines,
                entry.hunk.new_start,
                entry.hunk.new_lines
            ),
            Style::default().fg(app.colors.resolve(Color::Cyan)),
        )));
        for diff_line in &entry.hunk.lines {
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

    let [content_row, h_scrollbar_row] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);
    let [text_col, v_scrollbar_col] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(1)]).areas(content_row);

    let max_line_width = lines.iter().map(Line::width).max().unwrap_or(0);
    let max_h_scroll = max_line_width.saturating_sub(text_col.width as usize);
    let max_v_scroll = lines.len().saturating_sub(text_col.height as usize);
    let h_scroll = h_scroll.min(max_h_scroll);
    let v_scroll = v_scroll.min(max_v_scroll);
    if let AppMode::SplitHunksSelect {
        preview_h_scroll,
        preview_v_scroll,
        ..
    } = &mut app.mode
    {
        *preview_h_scroll = h_scroll;
        *preview_v_scroll = v_scroll;
    }

    let paragraph = Paragraph::new(lines).scroll((v_scroll as u16, h_scroll as u16));
    frame.render_widget(paragraph, text_col);

    if max_v_scroll > 0 {
        render_vertical_scrollbar(frame, v_scrollbar_col, v_scroll, max_v_scroll);
    }
    if max_h_scroll > 0 {
        render_horizontal_scrollbar(frame, h_scrollbar_row, h_scroll, max_h_scroll);
    }
}

fn render_vertical_scrollbar(
    frame: &mut Frame,
    area: Rect,
    scroll_offset: usize,
    max_scroll: usize,
) {
    if area.height == 0 {
        return;
    }
    let mut state = ScrollbarState::new(max_scroll).position(scroll_offset);
    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .begin_symbol(None)
        .end_symbol(None)
        .track_symbol(Some("│"));
    frame.render_stateful_widget(scrollbar, area, &mut state);
}

fn render_horizontal_scrollbar(
    frame: &mut Frame,
    area: Rect,
    scroll_offset: usize,
    max_scroll: usize,
) {
    if area.width == 0 {
        return;
    }
    let mut state = ScrollbarState::new(max_scroll).position(scroll_offset);
    let scrollbar = Scrollbar::new(ScrollbarOrientation::HorizontalBottom)
        .begin_symbol(None)
        .end_symbol(None)
        .track_symbol(Some("─"));
    frame.render_stateful_widget(scrollbar, area, &mut state);
}

/// Two rows of key hints — the full set doesn't fit legibly on one line at
/// typical terminal widths.
fn render_instructions(app: &AppState, frame: &mut Frame, area: Rect) {
    let key_color = app.colors.resolve(Color::Cyan);
    let rows: [&[(&str, &str)]; 2] = [
        &[("↑/↓", "Move"), ("Space", "Toggle"), ("+/-", "Context")],
        &[
            ("←/→ Ctrl-↑/↓", "Scroll"),
            ("Enter", "Split"),
            ("Esc", "Cancel"),
        ],
    ];
    let lines: Vec<Line> = rows
        .iter()
        .map(|hints| {
            let mut spans = Vec::new();
            for (i, &(key, desc)) in hints.iter().enumerate() {
                spans.push(Span::styled(
                    format!("{key} "),
                    Style::default().fg(key_color),
                ));
                if i + 1 < hints.len() {
                    spans.push(Span::raw(format!("{desc}   ")));
                } else {
                    spans.push(Span::raw(desc.to_string()));
                }
            }
            Line::from(spans).alignment(ratatui::layout::Alignment::Center)
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), area);
}
