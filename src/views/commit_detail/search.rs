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

// Regex search within the commit detail view — query entry, match navigation,
// highlighting, and the search bar.

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use regex::RegexBuilder;

use crate::app::{AppAction, AppState};
use crate::views::palette::Colors;

#[derive(Clone, Copy, PartialEq, Eq)]
enum SearchDirection {
    Next,
    Prev,
}

/// Transient info about the search bar, computed during render.
pub(super) enum SearchBarInfo {
    NoMatches,
    InvalidPattern,
    Matches { current: usize, total: usize },
}

/// Handle raw keyboard events while the search input bar is active.
///
/// Called from main.rs before `parse_key` so that character keys are
/// captured as search-query input rather than dispatched as commands.
pub fn handle_search_event(event: Event, app: &mut AppState) -> AppAction {
    if let Event::Key(KeyEvent {
        code,
        kind,
        modifiers,
        ..
    }) = event
        && kind == event::KeyEventKind::Press
    {
        if code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
            return AppAction::Quit;
        }
        match code {
            KeyCode::Esc => {
                app.search.clear();
            }
            KeyCode::Enter => {
                app.search.input_active = false;
                // Jump to the first match at or after the current scroll
                // position, wrapping to match 0 when all matches lie above —
                // the same behavior as `less`.
                if !app.search.matches.is_empty() {
                    let next_idx = app
                        .search
                        .matches
                        .iter()
                        .position(|&line| line >= app.detail.v.offset)
                        .unwrap_or(0);
                    app.search.match_index = Some(next_idx);
                    scroll_to_current_match(app);
                }
            }
            KeyCode::Backspace => {
                app.search.query.pop();
            }
            KeyCode::Char(c) => {
                app.search.query.push(c);
            }
            _ => {}
        }
    }
    AppAction::Handled
}

/// Advance to the next search match, wrapping around.
pub(super) fn next_match(app: &mut AppState) {
    advance_search_match(app, SearchDirection::Next);
}

/// Advance to the previous search match, wrapping around.
pub(super) fn prev_match(app: &mut AppState) {
    advance_search_match(app, SearchDirection::Prev);
}

/// Advance to the next or previous search match, wrapping around.
fn advance_search_match(app: &mut AppState, dir: SearchDirection) {
    if !app.search.active || app.search.matches.is_empty() {
        return;
    }
    let len = app.search.matches.len();
    app.search.match_index = Some(next_match_index(app.search.match_index, len, dir));
    scroll_to_current_match(app);
}

/// Return the next match index given the current index, list length, and direction.
/// Wraps around at both ends.
fn next_match_index(current: Option<usize>, len: usize, dir: SearchDirection) -> usize {
    match (current, dir) {
        (Some(idx), SearchDirection::Next) => (idx + 1) % len,
        (Some(0), SearchDirection::Prev) => len - 1,
        (Some(idx), SearchDirection::Prev) => idx - 1,
        (None, SearchDirection::Next) => 0,
        (None, SearchDirection::Prev) => len - 1,
    }
}

/// Center the detail view on `target_line`, but only when it is off screen —
/// recentring an already-visible match would fight the user's own scrolling.
fn center_on_line(app: &mut AppState, target_line: usize) {
    let vh = app.detail.v.visible_height;
    if vh == 0 {
        return;
    }
    let offset = app.detail.v.offset;
    if target_line < offset || target_line >= offset + vh {
        app.detail.v.scroll_to(target_line.saturating_sub(vh / 2));
    }
}

/// Scroll the detail view so the current search match line is visible.
fn scroll_to_current_match(app: &mut AppState) {
    if let Some(match_idx) = app.search.match_index
        && let Some(&target_line) = app.search.matches.get(match_idx)
    {
        center_on_line(app, target_line);
    }
}

/// Compute matches, auto-scroll to a newly selected one, and highlight the
/// content lines. Returns the (possibly highlighted) content plus the info the
/// search bar should display.
pub(super) fn apply(
    app: &mut AppState,
    mut content: Vec<Line<'static>>,
) -> (Vec<Line<'static>>, Option<SearchBarInfo>) {
    let mut search_info = None;
    if app.search.active && !app.search.query.is_empty() {
        match RegexBuilder::new(&app.search.query).build() {
            Ok(regex) => {
                let (info, prev_match_index) = compute_search_matches(app, &content, &regex);
                search_info = info;
                if !app.search.matches.is_empty() {
                    if !app.search.input_active {
                        auto_scroll_to_new_match(app, prev_match_index);
                    }
                    content = apply_search_highlighting(content, app, &regex, app.colors);
                }
            }
            Err(_) => {
                app.search.matches.clear();
                app.search.match_index = None;
                search_info = Some(SearchBarInfo::InvalidPattern);
            }
        }
    } else if !app.search.active {
        app.search.matches.clear();
        app.search.match_index = None;
    }
    (content, search_info)
}

/// Highlight regex matches within a single styled Span, returning one or more
/// replacement Spans.
fn highlight_span_matches(
    span: &Span<'_>,
    regex: &regex::Regex,
    highlight_style: Style,
) -> Vec<Span<'static>> {
    let text = span.content.as_ref();
    let style = span.style;
    let mut result = Vec::new();
    let mut last_end = 0;

    for m in regex.find_iter(text) {
        if m.start() > last_end {
            result.push(Span::styled(text[last_end..m.start()].to_string(), style));
        }
        result.push(Span::styled(
            text[m.start()..m.end()].to_string(),
            highlight_style,
        ));
        last_end = m.end();
    }
    if last_end < text.len() {
        result.push(Span::styled(text[last_end..].to_string(), style));
    }
    if result.is_empty() {
        result.push(Span::styled(text.to_string(), style));
    }
    result
}

/// Apply search-match highlighting to an entire Line.
fn highlight_line_matches(
    line: Line<'_>,
    regex: &regex::Regex,
    is_current_match: bool,
    colors: Colors,
) -> Line<'static> {
    let highlight_style = if is_current_match {
        colors.resolve_style(Style::default().fg(Color::Black).bg(Color::LightCyan))
    } else {
        colors.resolve_style(Style::default().fg(Color::Black).bg(Color::Yellow))
    };
    let new_spans: Vec<Span<'static>> = line
        .spans
        .iter()
        .flat_map(|span| highlight_span_matches(span, regex, highlight_style))
        .collect();
    Line::from(new_spans)
}

/// Return the plain-text content of a Line (all spans concatenated).
fn line_text(line: &Line<'_>) -> String {
    line.spans.iter().map(|s| s.content.as_ref()).collect()
}

/// Find all content lines matching `regex`, update `app.search.matches` and
/// `app.search.match_index`, and return the corresponding `SearchBarInfo` plus
/// the previous match index (so callers can detect whether it changed).
fn compute_search_matches(
    app: &mut AppState,
    content: &[Line<'_>],
    regex: &regex::Regex,
) -> (Option<SearchBarInfo>, Option<usize>) {
    let prev_match_index = app.search.match_index;

    app.search.matches = content
        .iter()
        .enumerate()
        .filter(|(_, line)| regex.is_match(&line_text(line)))
        .map(|(i, _)| i)
        .collect();

    if app.search.matches.is_empty() {
        app.search.match_index = None;
        (Some(SearchBarInfo::NoMatches), prev_match_index)
    } else {
        match app.search.match_index {
            None => app.search.match_index = Some(0),
            Some(idx) if idx >= app.search.matches.len() => {
                app.search.match_index = Some(app.search.matches.len() - 1);
            }
            _ => {}
        }
        (
            Some(SearchBarInfo::Matches {
                current: app.search.match_index.unwrap(),
                total: app.search.matches.len(),
            }),
            prev_match_index,
        )
    }
}

/// Scroll the detail view so the current match is visible, but only when the
/// match index actually changed (first match found or index clamped). This
/// avoids overriding manual arrow/PageUp/PageDown scrolling on every render.
fn auto_scroll_to_new_match(app: &mut AppState, prev_match_index: Option<usize>) {
    if app.search.match_index != prev_match_index
        && let Some(mi) = app.search.match_index
    {
        center_on_line(app, app.search.matches[mi]);
    }
}

/// Apply search-match highlighting to all content lines.
fn apply_search_highlighting(
    content: Vec<Line<'_>>,
    app: &AppState,
    regex: &regex::Regex,
    colors: Colors,
) -> Vec<Line<'static>> {
    let current_match_line = app.search.match_index.map(|i| app.search.matches[i]);
    content
        .into_iter()
        .enumerate()
        .map(|(i, line)| {
            let is_current = current_match_line == Some(i);
            highlight_line_matches(line, regex, is_current, colors)
        })
        .collect()
}

/// Render the search bar widget into the given area.
pub(super) fn render_bar(
    frame: &mut Frame,
    app: &AppState,
    search_info: Option<SearchBarInfo>,
    area: Rect,
) {
    let colors = app.colors;
    let dark_gray = colors.resolve(Color::DarkGray);
    let red = colors.resolve(Color::Red);
    let mut bar_spans = vec![
        Span::styled("/", Style::default().fg(dark_gray)),
        Span::styled(
            app.search.query.clone(),
            Style::default().fg(colors.resolve(Color::White)),
        ),
    ];
    if app.search.input_active {
        bar_spans.push(Span::styled("█", Style::default().fg(dark_gray)));
    }
    match search_info {
        Some(SearchBarInfo::NoMatches) => {
            bar_spans.push(Span::styled("  [no matches]", Style::default().fg(red)));
        }
        Some(SearchBarInfo::InvalidPattern) => {
            bar_spans.push(Span::styled(
                "  [invalid pattern]",
                Style::default().fg(red),
            ));
        }
        Some(SearchBarInfo::Matches { current, total }) => {
            bar_spans.push(Span::styled(
                format!("  [{}/{}]", current + 1, total),
                Style::default().fg(dark_gray),
            ));
        }
        None => {}
    }
    frame.render_widget(Paragraph::new(Line::from(bar_spans)), area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_from_none_forward() {
        assert_eq!(next_match_index(None, 5, SearchDirection::Next), 0);
    }

    #[test]
    fn next_from_none_backward() {
        assert_eq!(next_match_index(None, 5, SearchDirection::Prev), 4);
    }

    #[test]
    fn next_wraps_forward() {
        assert_eq!(next_match_index(Some(4), 5, SearchDirection::Next), 0);
    }

    #[test]
    fn next_wraps_backward() {
        assert_eq!(next_match_index(Some(0), 5, SearchDirection::Prev), 4);
    }

    #[test]
    fn next_advances_forward() {
        assert_eq!(next_match_index(Some(2), 5, SearchDirection::Next), 3);
    }

    #[test]
    fn next_advances_backward() {
        assert_eq!(next_match_index(Some(3), 5, SearchDirection::Prev), 2);
    }
}
