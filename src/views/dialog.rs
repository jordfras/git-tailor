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
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

/// Render a centered dialog overlay.
///
/// Computes a centered rectangle from `preferred_width` and the number of
/// `lines`, clears the background, and draws a bordered paragraph.
pub fn render_centered_dialog(
    frame: &mut Frame,
    title: &str,
    border_color: Color,
    preferred_width: u16,
    lines: Vec<Line>,
) {
    let area = frame.area();
    let dialog_width = preferred_width.min(area.width.saturating_sub(4));
    let dialog_height = (lines.len() as u16 + 2).min(area.height.saturating_sub(2));
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
            .wrap(Wrap { trim: false }),
        dialog_area,
    );
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
