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

// Regex search within the commit detail view: matching the content,
// highlighting matches, and scrolling to them.

use crossterm::event::Event;
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};
use regex::RegexBuilder;

use crate::app::AppState;
use crate::app::search::{QueryEdit, SearchDirection};
use crate::views::palette::Colors;
use crate::views::search_bar::SearchBarInfo;

/// Handle raw keyboard events while the search input bar is active.
///
/// Called from main.rs before `parse_key` so that character keys are
/// captured as search-query input rather than dispatched as commands.
pub fn handle_search_event(event: Event, app: &mut AppState) {
    if app.detail.search.edit(&event) != QueryEdit::Confirmed {
        return;
    }
    // Jump to the first match at or after the current scroll position,
    // wrapping to match 0 when all matches lie above — the same behavior as
    // `less`.
    let search = &mut app.detail.search;
    if !search.matches.is_empty() {
        let next_idx = search
            .matches
            .iter()
            .position(|&line| line >= app.detail.v.offset)
            .unwrap_or(0);
        search.match_index = Some(next_idx);
        scroll_to_current_match(app);
    }
}

/// Advance to the next search match, wrapping around.
pub(super) fn next_match(app: &mut AppState) {
    advance_search_match(app, SearchDirection::Next);
}

/// Advance to the previous search match, wrapping around.
pub(super) fn prev_match(app: &mut AppState) {
    advance_search_match(app, SearchDirection::Prev);
}

fn advance_search_match(app: &mut AppState, dir: SearchDirection) {
    if let Some(&target_line) = app.detail.search.advance(dir) {
        center_on_line(app, target_line);
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
    if let Some(match_idx) = app.detail.search.match_index
        && let Some(&target_line) = app.detail.search.matches.get(match_idx)
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
    if app.detail.search.active && !app.detail.search.query.is_empty() {
        match RegexBuilder::new(&app.detail.search.query).build() {
            Ok(regex) => {
                let (info, prev_match_index) = compute_search_matches(app, &content, &regex);
                search_info = info;
                if !app.detail.search.matches.is_empty() {
                    if !app.detail.search.input_active {
                        auto_scroll_to_new_match(app, prev_match_index);
                    }
                    content = apply_search_highlighting(content, app, &regex, app.colors);
                }
            }
            Err(_) => {
                app.detail.search.matches.clear();
                app.detail.search.match_index = None;
                search_info = Some(SearchBarInfo::InvalidPattern);
            }
        }
    } else if !app.detail.search.active {
        app.detail.search.matches.clear();
        app.detail.search.match_index = None;
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

/// Find all content lines matching `regex`, update `app.detail.search.matches` and
/// `app.detail.search.match_index`, and return the corresponding `SearchBarInfo` plus
/// the previous match index (so callers can detect whether it changed).
fn compute_search_matches(
    app: &mut AppState,
    content: &[Line<'_>],
    regex: &regex::Regex,
) -> (Option<SearchBarInfo>, Option<usize>) {
    let prev_match_index = app.detail.search.match_index;

    app.detail.search.matches = content
        .iter()
        .enumerate()
        .filter(|(_, line)| regex.is_match(&line_text(line)))
        .map(|(i, _)| i)
        .collect();

    if app.detail.search.matches.is_empty() {
        app.detail.search.match_index = None;
        (Some(SearchBarInfo::NoMatches), prev_match_index)
    } else {
        match app.detail.search.match_index {
            None => app.detail.search.match_index = Some(0),
            Some(idx) if idx >= app.detail.search.matches.len() => {
                app.detail.search.match_index = Some(app.detail.search.matches.len() - 1);
            }
            _ => {}
        }
        (
            Some(SearchBarInfo::Matches {
                current: app.detail.search.match_index.unwrap(),
                total: app.detail.search.matches.len(),
            }),
            prev_match_index,
        )
    }
}

/// Scroll the detail view so the current match is visible, but only when the
/// match index actually changed (first match found or index clamped). This
/// avoids overriding manual arrow/PageUp/PageDown scrolling on every render.
fn auto_scroll_to_new_match(app: &mut AppState, prev_match_index: Option<usize>) {
    if app.detail.search.match_index != prev_match_index
        && let Some(mi) = app.detail.search.match_index
    {
        center_on_line(app, app.detail.search.matches[mi]);
    }
}

/// Apply search-match highlighting to all content lines.
fn apply_search_highlighting(
    content: Vec<Line<'_>>,
    app: &AppState,
    regex: &regex::Regex,
    colors: Colors,
) -> Vec<Line<'static>> {
    let current_match_line = app
        .detail
        .search
        .match_index
        .map(|i| app.detail.search.matches[i]);
    content
        .into_iter()
        .enumerate()
        .map(|(i, line)| {
            let is_current = current_match_line == Some(i);
            highlight_line_matches(line, regex, is_current, colors)
        })
        .collect()
}
