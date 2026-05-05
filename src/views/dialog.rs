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

use crate::app::{AppState, KeyCommand};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap,
    },
};

/// Handle scroll keys for a scrollable dialog overlay.
///
/// Returns `true` if the action was consumed. Maps `MoveUp`/`MoveDown` to
/// line scrolling and `PageUp`/`PageDown` to page scrolling.
pub fn handle_dialog_scroll(action: KeyCommand, app: &mut AppState) -> bool {
    match action {
        KeyCommand::MoveUp => {
            app.scroll_dialog_up();
            true
        }
        KeyCommand::MoveDown => {
            app.scroll_dialog_down();
            true
        }
        KeyCommand::PageUp => {
            app.scroll_dialog_page_up();
            true
        }
        KeyCommand::PageDown => {
            app.scroll_dialog_page_down();
            true
        }
        _ => false,
    }
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

/// Incremental builder for a centered overlay dialog.
///
/// Chain content methods, then call [`render`][Dialog::render] to draw and
/// return `(max_scroll, visible_height)` for scroll management.
///
/// # Example
///
/// ```ignore
/// Dialog::new()
///     .title(" Drop this commit?", Color::Yellow)
///     .styled_line(format!(" {short_oid}"), Color::Cyan)
///     .instructions(&[("Enter", Color::Cyan, "Confirm"), ("Esc", Color::Cyan, "Cancel")])
///     .blank()
///     .render(frame, "Confirm Drop", Color::Yellow, 60, 0);
/// ```
#[derive(Default)]
pub struct Dialog {
    lines: Vec<Line<'static>>,
}

impl Dialog {
    pub fn new() -> Self {
        Self { lines: Vec::new() }
    }

    /// Push an empty blank line.
    pub fn blank(mut self) -> Self {
        self.lines.push(Line::from(""));
        self
    }

    /// Push a bold line in `color` surrounded by blank lines above and below.
    pub fn title(mut self, text: impl Into<String>, color: Color) -> Self {
        self.lines.push(Line::from(""));
        self.lines.push(Line::from(Span::styled(
            text.into(),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        )));
        self.lines.push(Line::from(""));
        self
    }

    /// Push a bold yellow section header preceded by a blank line.
    ///
    /// Unlike [`title`][Dialog::title], no blank is added after — the
    /// content lines follow immediately.
    pub fn section(mut self, text: impl Into<String>) -> Self {
        self.lines.push(Line::from(""));
        self.lines.push(Line::from(Span::styled(
            text.into(),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )));
        self
    }

    /// Push a line styled with `color` (no bold).
    pub fn styled_line(mut self, text: impl Into<String>, color: Color) -> Self {
        self.lines.push(Line::from(Span::styled(
            text.into(),
            Style::default().fg(color),
        )));
        self
    }

    /// Push a plain unstyled line.
    pub fn plain(mut self, text: impl Into<String>) -> Self {
        self.lines.push(Line::from(Span::raw(text.into())));
        self
    }

    /// Wrap `text` and push each chunk as a raw line prefixed with a single space.
    pub fn wrapped(mut self, text: &str, width: usize) -> Self {
        for chunk in wrap_text(text, width) {
            self.lines.push(Line::from(Span::raw(format!(" {chunk}"))));
        }
        self
    }

    /// Wrap `text` with indent preservation and push each chunk as a plain
    /// raw line (leading indent preserved on continuation lines).
    pub fn wrapped_indent(mut self, text: &str, width: usize) -> Self {
        for chunk in wrap_text_indent(text, width) {
            self.lines.push(Line::from(Span::raw(chunk)));
        }
        self
    }

    /// Wrap `text` with indent preservation and push each chunk styled with
    /// `color`.
    pub fn wrapped_styled(mut self, text: &str, width: usize, color: Color) -> Self {
        for chunk in wrap_text_indent(text, width) {
            self.lines
                .push(Line::from(Span::styled(chunk, Style::default().fg(color))));
        }
        self
    }

    /// Like [`wrapped_styled`][Dialog::wrapped_styled] but also applies the `BOLD` modifier.
    pub fn wrapped_styled_bold(mut self, text: &str, width: usize, color: Color) -> Self {
        for chunk in wrap_text_indent(text, width) {
            self.lines.push(Line::from(Span::styled(
                chunk,
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            )));
        }
        self
    }

    /// Push a two-span key-binding line: `key` rendered in Cyan, `desc` in
    /// the default color.
    pub fn key_binding(mut self, key: &str, desc: &str) -> Self {
        self.lines.push(Line::from(vec![
            Span::styled(key.to_string(), Style::default().fg(Color::Cyan)),
            Span::raw(desc.to_string()),
        ]));
        self
    }

    /// Push a centered line of key-hint pairs `(key, color, desc)`.
    ///
    /// Each hint is rendered as a colored `key ` span followed by a raw `desc`
    /// span; consecutive hints are separated by three spaces.
    pub fn instructions(mut self, hints: &[(&str, Color, &str)]) -> Self {
        let mut spans = Vec::new();
        for (i, &(key, color, desc)) in hints.iter().enumerate() {
            spans.push(Span::styled(format!("{key} "), Style::default().fg(color)));
            if i + 1 < hints.len() {
                spans.push(Span::raw(format!("{desc}   ")));
            } else {
                spans.push(Span::raw(desc.to_string()));
            }
        }
        self.lines
            .push(Line::from(spans).alignment(Alignment::Center));
        self
    }

    /// Push an arbitrary pre-built line (escape hatch for complex multi-span
    /// lines not covered by the other methods).
    pub fn push_line(mut self, line: Line<'static>) -> Self {
        self.lines.push(line);
        self
    }

    /// Render the dialog as a centered overlay and return `(max_scroll,
    /// visible_height)` for scroll management.
    pub fn render(
        self,
        frame: &mut Frame,
        title: &str,
        border_color: Color,
        preferred_width: u16,
        scroll_offset: usize,
    ) -> (usize, usize) {
        let area = frame.area();
        let content_height = self.lines.len();
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
        let padded_title = format!(" {title} ");
        frame.render_widget(
            Paragraph::new(self.lines)
                .block(
                    Block::default()
                        .title(padded_title.as_str())
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

/// Word-wrap `text` to at most `width` display columns per line.
///
/// Breaks at the last space within the allowed width; falls back to a hard
/// break at `width` characters when no space is found. Always returns at
/// least one element.
fn wrap_text(text: &str, width: usize) -> Vec<String> {
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

/// Like `wrap_text`, but preserves the leading whitespace of the first line
/// as a hanging indent on all continuation lines so wrapped text stays
/// visually grouped under its first line.
fn wrap_text_indent(text: &str, width: usize) -> Vec<String> {
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
