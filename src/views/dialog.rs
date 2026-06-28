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

/// Semantic dialog kind that determines the border color.
///
/// Centralises the border-color convention so all overlay dialogs are
/// consistent and a single edit here changes every dialog of that kind.
pub enum DialogKind {
    /// Informational / status dialog — Cyan border.
    Info,
    /// Confirmation dialog before a non-destructive action — Yellow border.
    Confirm,
    /// Destructive or error dialog — Red border.
    Danger,
}

impl DialogKind {
    pub fn border_color(&self) -> Color {
        match self {
            DialogKind::Info => Color::Cyan,
            DialogKind::Confirm => Color::Yellow,
            DialogKind::Danger => Color::Red,
        }
    }
}

/// Semantic text role for dialog content lines.
///
/// All role → color mappings live here so a future theme change only needs to
/// touch `TextRole::color`.
pub enum TextRole {
    /// Default text — White.
    Normal,
    /// Highlighted / emphasized text — Yellow.
    Highlight,
    /// Muted / secondary text — DarkGray.
    Muted,
    /// Key name or short identifier — Cyan.
    Key,
    /// Error or destructive text — Red.
    Danger,
}

impl TextRole {
    pub fn color(&self) -> Color {
        match self {
            TextRole::Normal => Color::White,
            TextRole::Highlight => Color::Yellow,
            TextRole::Muted => Color::DarkGray,
            TextRole::Key => Color::Cyan,
            TextRole::Danger => Color::Red,
        }
    }
}

/// Left + right border columns (one column each side).
const BORDER_WIDTH: u16 = 2;
/// Top + bottom border rows (one row each side).
const BORDER_HEIGHT: u16 = 2;

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
        .min(area_width.saturating_sub(BORDER_WIDTH * 2))
        .saturating_sub(BORDER_WIDTH) as usize
}

/// Shorten `path` to roughly `inner_width` columns, keeping its tail behind a
/// leading ellipsis (`" …tail"`). Truncates on a character boundary, so a
/// non-ASCII path never panics; for ASCII input the result is unchanged.
pub fn truncate_path_tail(path: &str, inner_width: usize) -> String {
    if path.len() + 3 > inner_width {
        let keep = inner_width.saturating_sub(3);
        let skip = path.chars().count().saturating_sub(keep);
        let tail: String = path.chars().skip(skip).collect();
        format!(" \u{2026}{tail}")
    } else {
        path.to_string()
    }
}

/// Append the shared conflict-dialog tail to `dialog` — the list of conflicting
/// files, the "still unresolved" warning, and the `Enter`/`m`/`e`/`Esc`
/// instructions — then render it under `title`.
///
/// Shared by the rebase-conflict and auto-stash-conflict dialogs so the two look
/// and behave identically; only the heading and body (already built into
/// `dialog` by the caller) differ.
pub fn render_conflict_dialog(
    app: &mut AppState,
    frame: &mut Frame,
    mut dialog: Dialog,
    conflicting_files: &[String],
    still_unresolved: bool,
    preferred_width: u16,
    title: &str,
) {
    let iw = inner_width(preferred_width, frame.area().width);
    if !conflicting_files.is_empty() {
        dialog = dialog
            .blank()
            .styled_line("Conflicting files:", TextRole::Highlight);
        const MAX_FILES: usize = 5;
        let shown = conflicting_files.len().min(MAX_FILES);
        for path in &conflicting_files[..shown] {
            dialog = dialog.styled_line(truncate_path_tail(path, iw), TextRole::Danger);
        }
        let extra = conflicting_files.len().saturating_sub(MAX_FILES);
        if extra > 0 {
            dialog = dialog.styled_line(format!("... {extra} more"), TextRole::Muted);
        }
    }

    dialog = dialog.blank();
    if still_unresolved {
        dialog = dialog
            .wrapped_styled_bold(
                " ! Still unresolved — fix all conflicts above before continuing",
                iw,
                TextRole::Danger,
            )
            .blank();
    }
    dialog = dialog
        .instructions(&[
            ("Enter", Color::Green, "Continue"),
            ("m", Color::Cyan, "Mergetool"),
            ("e", Color::Cyan, "Editor"),
            ("Esc", Color::Red, "Abort"),
        ])
        .blank();

    let (max_scroll, visible_height) =
        dialog.render(frame, title, preferred_width, app.dialog_scroll_offset);
    app.max_dialog_scroll = max_scroll;
    app.dialog_visible_height = visible_height;
}

/// Incremental builder for a centered overlay dialog.
///
/// Chain content methods, then call [`render`][Dialog::render] to draw and
/// return `(max_scroll, visible_height)` for scroll management.
///
/// # Example
///
/// ```ignore
/// Dialog::new(DialogKind::Confirm)
///     .heading("Drop this commit?", TextRole::Highlight)
///     .styled_line(format!("{short_oid}"), TextRole::Key)
///     .instructions(&[("Enter", Color::Cyan, "Confirm"), ("Esc", Color::Cyan, "Cancel")])
///     .blank()
///     .render(frame, "Confirm Drop", 60, 0);
/// ```
pub struct Dialog {
    kind: DialogKind,
    lines: Vec<Line<'static>>,
}

impl Dialog {
    pub fn new(kind: DialogKind) -> Self {
        Self {
            kind,
            lines: Vec::new(),
        }
    }

    /// Push an empty blank line.
    pub fn blank(mut self) -> Self {
        self.lines.push(Line::from(""));
        self
    }

    /// Push a bold heading line colored by `role`, surrounded by blank lines above and below.
    ///
    /// The leading space is added automatically — pass the text without it.
    pub fn heading(mut self, text: impl Into<String>, role: TextRole) -> Self {
        self.lines.push(Line::from(""));
        self.lines.push(Line::from(Span::styled(
            format!(" {}", text.into()),
            Style::default()
                .fg(role.color())
                .add_modifier(Modifier::BOLD),
        )));
        self.lines.push(Line::from(""));
        self
    }

    /// Push a bold yellow section header preceded by a blank line.
    ///
    /// Unlike [`heading`][Dialog::heading], no blank is added after — the
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

    /// Push a line with the color resolved from `role` (no bold).
    ///
    /// A single leading space is added automatically as a left margin.
    pub fn styled_line(mut self, text: impl Into<String>, role: TextRole) -> Self {
        self.lines.push(Line::from(Span::styled(
            format!(" {}", text.into()),
            Style::default().fg(role.color()),
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

    /// Wrap `text` with indent preservation and push each chunk with the color resolved from `role`.
    pub fn wrapped_styled(mut self, text: &str, width: usize, role: TextRole) -> Self {
        for chunk in wrap_text_indent(text, width) {
            self.lines.push(Line::from(Span::styled(
                chunk,
                Style::default().fg(role.color()),
            )));
        }
        self
    }

    /// Like [`wrapped_styled`][Dialog::wrapped_styled] but also applies the `BOLD` modifier.
    pub fn wrapped_styled_bold(mut self, text: &str, width: usize, role: TextRole) -> Self {
        for chunk in wrap_text_indent(text, width) {
            self.lines.push(Line::from(Span::styled(
                chunk,
                Style::default()
                    .fg(role.color())
                    .add_modifier(Modifier::BOLD),
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
        preferred_width: u16,
        scroll_offset: usize,
    ) -> (usize, usize) {
        let border_color = self.kind.border_color();
        let area = frame.area();
        let dialog_width = preferred_width.min(area.width.saturating_sub(BORDER_WIDTH * 2));

        let padded_title = format!(" {title} ");
        let paragraph = Paragraph::new(self.lines)
            .block(
                Block::default()
                    .title(padded_title.as_str())
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(border_color))
                    .style(Style::default().bg(Color::Black)),
            )
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: false });

        // Measure the content *after* wrapping at the inner width. Using the
        // logical line count would undercount any line wider than the dialog
        // (it occupies several rows once wrapped), which previously left the
        // lower rows clipped by the border and unreachable by scrolling.
        // `line_count` shares the renderer's word-wrapper and includes the
        // block's top/bottom border rows, so subtract them for the bare height.
        let inner_width = dialog_width.saturating_sub(BORDER_WIDTH);
        let content_height = paragraph
            .line_count(inner_width)
            .saturating_sub(BORDER_HEIGHT as usize);

        let dialog_height =
            (content_height as u16 + BORDER_HEIGHT).min(area.height.saturating_sub(BORDER_HEIGHT));
        let inner_height = dialog_height.saturating_sub(BORDER_HEIGHT) as usize;
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
        frame.render_widget(paragraph.scroll((scroll_offset as u16, 0)), dialog_area);

        if max_scroll > 0 && dialog_height > BORDER_HEIGHT {
            let scrollbar_area = Rect {
                x: dialog_area.x + dialog_area.width.saturating_sub(1),
                y: dialog_area.y + 1,
                width: 1,
                height: dialog_area.height.saturating_sub(BORDER_HEIGHT),
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
