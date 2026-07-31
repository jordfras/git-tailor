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

// Shared rendering for wide, two-pane picker dialogs: a scrollable,
// multi-select list on the left, a scrollable code preview of the
// highlighted row on the right — mirroring how the main window splits the
// commit list from the detail view. Used by the "split out hunk(s)" and
// "split out file(s)" pickers; each owns its own key handling and row/preview
// content, calling in here only for the shared layout, scrolling, and
// scrollbar mechanics.

use std::collections::HashSet;

use crate::app::AppState;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
};

/// Columns consumed by the cursor marker + selection checkbox prefix
/// ("▸ [x] "), which callers must reserve when sizing their own label text.
pub(super) const LIST_ROW_PREFIX_WIDTH: usize = 6;

/// Usable text width inside a list pane's rect once its scrollbar gutter
/// (always reserved, even when not currently needed) is subtracted. Callers
/// use this — minus [`LIST_ROW_PREFIX_WIDTH`] and any suffix of their own —
/// to size their row labels before calling [`render_list`].
pub(super) fn list_text_width(area: Rect) -> usize {
    area.width.saturating_sub(1) as usize
}

/// Content areas of a two-pane picker dialog, after its border, title, and
/// instructions footer have been drawn.
pub(super) struct PickerAreas {
    pub list_area: Rect,
    pub preview_area: Rect,
}

/// Draw the outer bordered overlay (clear, border, title, instructions
/// footer) and the inner list│preview split, returning the list and preview
/// content areas for the caller to fill in via [`render_list`] and
/// [`render_preview`].
pub(super) fn render_frame(
    app: &AppState,
    frame: &mut Frame,
    title: &str,
    hint_rows: &[&[(&str, &str)]],
) -> PickerAreas {
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
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(bg_color));
    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let instructions_height = hint_rows.len() as u16;
    let [content_area, instructions_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(instructions_height)])
            .areas(inner);

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
    render_instructions(app, frame, instructions_area, hint_rows);

    PickerAreas {
        list_area,
        preview_area,
    }
}

fn render_separator(app: &AppState, frame: &mut Frame, area: Rect) {
    let sep_style = Style::default().fg(app.colors.resolve(Color::White));
    let sep_lines: Vec<Line> = (0..area.height)
        .map(|_| Line::from(Span::styled("│", sep_style)))
        .collect();
    frame.render_widget(Paragraph::new(sep_lines), area);
}

/// Render the list pane: one row per label (already sized to fit — see
/// [`list_text_width`] and [`LIST_ROW_PREFIX_WIDTH`]), prefixed with a cursor
/// marker and selection checkbox, with a vertical scrollbar once it overflows.
///
/// Uses `app.dialog` for its scroll state, same as every other list-picker
/// dialog.
pub(super) fn render_list(
    app: &mut AppState,
    frame: &mut Frame,
    area: Rect,
    labels: &[String],
    cursor_index: usize,
    selected: &HashSet<usize>,
) {
    let [text_area, scrollbar_area] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(1)]).areas(area);

    app.dialog.set_bounds(
        labels.len().saturating_sub(text_area.height as usize),
        text_area.height as usize,
    );
    app.dialog.clamp_offset();

    let lines: Vec<Line> = labels
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let cursor = i == cursor_index;
            let is_selected = selected.contains(&i);
            let marker = if cursor { "▸" } else { " " };
            let check = if is_selected { "[x]" } else { "[ ]" };
            let style = if cursor {
                Style::default()
                    .fg(app.colors.resolve(Color::Cyan))
                    .add_modifier(Modifier::BOLD)
            } else if is_selected {
                Style::default().fg(app.colors.resolve(Color::Green))
            } else {
                Style::default().fg(app.colors.resolve(Color::White))
            };
            Line::from(Span::styled(format!("{marker} {check} {label}"), style))
        })
        .collect();

    let paragraph = Paragraph::new(lines).scroll((app.dialog.offset as u16, 0));
    frame.render_widget(paragraph, text_area);

    if app.dialog.max > 0 {
        render_vertical_scrollbar(frame, scrollbar_area, app.dialog.offset, app.dialog.max);
    }
}

/// Render the preview pane, scrolled to `(h_scroll, v_scroll)` and clamped to
/// the content's actual size; returns the clamped values so the caller can
/// write them back into its own mode-specific scroll fields (repeatedly
/// scrolling past the edge shouldn't grow the stored offset unbounded).
pub(super) fn render_preview(
    frame: &mut Frame,
    area: Rect,
    lines: Vec<Line<'static>>,
    h_scroll: usize,
    v_scroll: usize,
) -> (usize, usize) {
    let [content_row, h_scrollbar_row] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);
    let [text_col, v_scrollbar_col] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(1)]).areas(content_row);

    let max_line_width = lines.iter().map(Line::width).max().unwrap_or(0);
    let max_h_scroll = max_line_width.saturating_sub(text_col.width as usize);
    let max_v_scroll = lines.len().saturating_sub(text_col.height as usize);
    let h_scroll = h_scroll.min(max_h_scroll);
    let v_scroll = v_scroll.min(max_v_scroll);

    let paragraph = Paragraph::new(lines).scroll((v_scroll as u16, h_scroll as u16));
    frame.render_widget(paragraph, text_col);

    if max_v_scroll > 0 {
        render_vertical_scrollbar(frame, v_scrollbar_col, v_scroll, max_v_scroll);
    }
    if max_h_scroll > 0 {
        render_horizontal_scrollbar(frame, h_scrollbar_row, h_scroll, max_h_scroll);
    }

    (h_scroll, v_scroll)
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

/// Render up to a few rows of centered key hints in the dialog's footer.
fn render_instructions(app: &AppState, frame: &mut Frame, area: Rect, rows: &[&[(&str, &str)]]) {
    let key_color = app.colors.resolve(Color::Cyan);
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
            Line::from(spans).alignment(Alignment::Center)
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), area);
}

/// Shorten `path` to at most `max` display columns for a list row by eliding
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
