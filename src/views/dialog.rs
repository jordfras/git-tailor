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

// Shared dialog rendering utilities for centered overlay dialogs.

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::Line,
    widgets::{
        Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap,
    },
};

/// Render a centered dialog overlay with optional vertical scrolling.
///
/// Computes a centered rectangle from `preferred_width` and the number of
/// `lines` (clamped to the terminal height), renders a bordered paragraph
/// scrolled to `scroll_offset`, and draws a scrollbar when the content is
/// taller than the visible area.
///
/// Returns `(max_scroll, visible_height)` so the caller can clamp the stored
/// scroll offset and compute page sizes.
pub fn render_centered_dialog(
    frame: &mut Frame,
    title: &str,
    border_color: Color,
    preferred_width: u16,
    lines: Vec<Line>,
    scroll_offset: usize,
) -> (usize, usize) {
    let area = frame.area();
    let content_height = lines.len();
    let dialog_width = preferred_width.min(area.width.saturating_sub(4));
    let dialog_height = (content_height as u16 + 2).min(area.height.saturating_sub(2));
    let inner_height = dialog_height.saturating_sub(2) as usize;
    let max_scroll = content_height.saturating_sub(inner_height);
    let scroll_offset = scroll_offset.min(max_scroll);

    let dialog_x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
    let dialog_y = area.y + (area.height.saturating_sub(dialog_height)) / 2;
    let dialog_area = Rect {
        x: dialog_x,
        y: dialog_y,
        width: dialog_width,
        height: dialog_height,
    };

    frame.render_widget(Clear, dialog_area);
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(border_color))
                    .style(Style::default().bg(Color::Black)),
            )
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: false })
            .scroll((scroll_offset as u16, 0)),
        dialog_area,
    );

    if max_scroll > 0 && dialog_height > 2 {
        let scrollbar_area = Rect {
            x: dialog_area.x + dialog_area.width.saturating_sub(1),
            y: dialog_area.y + 1,
            width: 1,
            height: dialog_area.height.saturating_sub(2),
        };
        render_dialog_scrollbar(
            frame,
            scrollbar_area,
            scroll_offset,
            content_height,
            inner_height,
        );
    }

    (max_scroll, inner_height)
}

/// Draw a vertical scrollbar track+thumb inside a 1-column-wide area.
fn render_dialog_scrollbar(
    frame: &mut Frame,
    area: Rect,
    scroll_offset: usize,
    total_lines: usize,
    visible_height: usize,
) {
    if area.height == 0 || total_lines == 0 {
        return;
    }
    let max_scroll = total_lines.saturating_sub(visible_height);
    let mut state = ScrollbarState::new(max_scroll).position(scroll_offset);
    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .begin_symbol(None)
        .end_symbol(None)
        .track_symbol(Some("│"));
    frame.render_stateful_widget(scrollbar, area, &mut state);
}

/// Compute the usable inner width for content inside a dialog.
///
/// Returns the number of character columns available between the borders,
/// accounting for the terminal width constraint.
pub fn inner_width(preferred_width: u16, area_width: u16) -> usize {
    preferred_width
        .min(area_width.saturating_sub(4))
        .saturating_sub(2) as usize
}

/// Word-wrap `text` to at most `width` display columns per line.
///
/// Breaks at the last space within the allowed width; falls back to a hard
/// break at `width` characters when no space is found. Always returns at
/// least one element.
pub fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 || text.is_empty() {
        return vec![text.to_string()];
    }
    let mut result = Vec::new();
    let mut remaining = text;
    while remaining.chars().count() > width {
        let byte_limit = remaining
            .char_indices()
            .nth(width)
            .map(|(i, _)| i)
            .unwrap_or(remaining.len());
        let break_at = remaining[..byte_limit]
            .rfind(' ')
            .filter(|&p| p > 0)
            .unwrap_or(byte_limit);
        result.push(remaining[..break_at].to_string());
        remaining = remaining[break_at..].trim_start_matches(' ');
    }
    result.push(remaining.to_string());
    result
}

/// Like [`wrap_text`], but preserves the leading whitespace of the first line
/// as a hanging indent on all continuation lines so wrapped text stays
/// visually grouped under its first line.
pub fn wrap_text_indent(text: &str, width: usize) -> Vec<String> {
    let indent: String = text.chars().take_while(|c| *c == ' ').collect();
    let chunks = wrap_text(text, width);
    if chunks.len() <= 1 || indent.is_empty() {
        return chunks;
    }
    let mut result = vec![chunks[0].clone()];
    for chunk in &chunks[1..] {
        result.push(format!("{indent}{chunk}"));
    }
    result
}
