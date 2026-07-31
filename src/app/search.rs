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

// Regex search state for the commit detail view.

/// Search query, results and cursor for the detail view's regex search.
#[derive(Debug, Default)]
pub struct SearchState {
    /// Current search query string (regex pattern).
    pub query: String,
    /// Whether the user is actively typing in the search bar.
    pub input_active: bool,
    /// Whether search results (highlights, match navigation) are active.
    pub active: bool,
    /// Line indices in the detail content that match the search regex.
    pub matches: Vec<usize>,
    /// Index into `matches` for the current match.
    pub match_index: Option<usize>,
}

impl SearchState {
    /// Clear all search state.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Activate search mode: clear the query and show the search bar.
    pub fn activate(&mut self) {
        *self = Self {
            input_active: true,
            active: true,
            ..Self::default()
        };
    }
}
