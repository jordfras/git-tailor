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

// The commit list's `/` search: the query, what it compiled to, and the rows
// it matched.

use crate::app::search::SearchState;
use crate::domain::search::{CommitQuery, InvalidPattern, SearchScope};
use crate::fragmap::CommitPos;

#[derive(Debug, Default)]
pub struct ListSearch {
    pub state: SearchState<CommitPos>,
    pub scope: SearchScope,
    /// `state.query` compiled for `scope`, `None` while the query is empty.
    query: Option<Result<CommitQuery, InvalidPattern>>,
}

impl ListSearch {
    /// Open the search bar on an empty query, keeping the scope.
    pub fn activate(&mut self) {
        self.state.activate();
        self.query = None;
    }

    /// Dismiss the search, keeping the scope.
    pub fn clear(&mut self) {
        self.state.clear();
        self.query = None;
    }

    pub fn pattern_is_invalid(&self) -> bool {
        matches!(self.query, Some(Err(_)))
    }

    pub(super) fn compile(&mut self) {
        self.query =
            (!self.state.query.is_empty()).then(|| CommitQuery::new(self.scope, &self.state.query));
    }

    pub(super) fn compiled(&self) -> Option<&CommitQuery> {
        self.query.as_ref()?.as_ref().ok()
    }
}
