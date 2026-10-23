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
use ratatui::{Frame, layout::Rect};

use crate::app::AppState;
use crate::app::ListSearch;
use crate::app::search::QueryEdit;
use crate::views::search_bar::{self, SearchBarInfo};

/// Handle raw keyboard events while the search bar takes input.
///
/// Called from main.rs before `parse_key` so that character keys are
/// captured as search-query input rather than dispatched as commands.
pub fn handle_search_event(event: Event, app: &mut AppState) {
    match app.list.search.state.edit(&event) {
        QueryEdit::Edited => app.list.rerun_search(),
        QueryEdit::Confirmed => app.list.confirm_search(),
        QueryEdit::Dismissed => app.list.search.clear(),
        QueryEdit::Ignored => {}
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
