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

// The commit list's search bar: typing the query and drawing the bar.

use crossterm::event::Event;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
};

use crate::app::AppState;
use crate::app::ListSearch;
use crate::app::search::QueryEdit;
use crate::fragmap::CommitPos;
use crate::views::search_bar::{self, SearchBarInfo};
use crate::views::search_match::{highlight_matches, match_style};

/// Handle raw keyboard events while the search bar takes input.
///
/// Called from main.rs before `parse_key` so that character keys are
/// captured as search-query input rather than dispatched as commands.
pub fn handle_search_event(event: Event, app: &mut AppState) {
    match app.list.search.state.edit(&event) {
        QueryEdit::Edited => app.list.search_as_typed(),
        QueryEdit::Dismissed => app.list.cancel_search(),
        QueryEdit::Confirmed | QueryEdit::Ignored => {}
    }
}

pub(super) fn render_bar(frame: &mut Frame, app: &AppState, area: Rect) {
    let search = &app.list.search;
    search_bar::render(frame, &search.state, bar_info(search), app.colors, area);
}

fn bar_info(search: &ListSearch) -> Option<SearchBarInfo> {
    let state = &search.state;
    if state.query.is_empty() {
        None
    } else if search.pattern_is_invalid() {
        Some(SearchBarInfo::InvalidPattern)
    } else if state.matches.is_empty() {
        Some(SearchBarInfo::NoMatches)
    } else {
        Some(SearchBarInfo::Matches {
            current: state.match_index.unwrap_or(0),
            total: state.matches.len(),
        })
    }
}

/// The style marking row `pos` as a match, `None` when it is not one.
pub(super) fn row_match_style(app: &AppState, pos: CommitPos) -> Option<Style> {
    let state = &app.list.search.state;
    let found = state.matches.binary_search(&pos).ok()?;
    Some(match_style(state.match_index == Some(found), app.colors))
}

/// `summary` in `style`, with the text the search matched highlighted when
/// the row is a match and the search looks at messages.
pub(super) fn summary_line(
    app: &AppState,
    summary: &str,
    style: Style,
    row_match: Option<Style>,
) -> Line<'static> {
    let span = Span::styled(summary.to_string(), style);
    let regex = app
        .list
        .search
        .compiled()
        .and_then(|query| query.message_regex());
    match (regex, row_match) {
        (Some(regex), Some(highlight)) => Line::from(highlight_matches(&span, regex, highlight)),
        _ => Line::from(span),
    }
}
