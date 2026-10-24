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

// Regex search state shared by the views that search: query entry and the
// cursor over whatever the view matched.

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SearchDirection {
    Next,
    Prev,
}

/// What a key typed into the search bar did to the query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryEdit {
    /// Enter: typing is over and the search stays active.
    Confirmed,
    /// Esc: the search is gone.
    Dismissed,
    Edited,
    Ignored,
}

/// Search query, results and cursor. `M` is what one match points at in the
/// view searched.
#[derive(Debug)]
pub struct SearchState<M> {
    /// Current search query string (regex pattern).
    pub query: String,
    /// Whether the user is actively typing in the search bar.
    pub input_active: bool,
    /// Whether search results (highlights, match navigation) are active.
    pub active: bool,
    pub matches: Vec<M>,
    /// Index into `matches` for the current match.
    pub match_index: Option<usize>,
}

impl<M> Default for SearchState<M> {
    fn default() -> Self {
        Self {
            query: String::new(),
            input_active: false,
            active: false,
            matches: Vec::new(),
            match_index: None,
        }
    }
}

impl<M> SearchState<M> {
    /// Clear all search state.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// A search for `query` that is already confirmed, as if typed and then
    /// entered.
    pub fn confirmed(query: String) -> Self {
        Self {
            query,
            active: true,
            ..Self::default()
        }
    }

    /// Activate search mode: clear the query and show the search bar.
    pub fn activate(&mut self) {
        *self = Self {
            input_active: true,
            active: true,
            ..Self::default()
        };
    }

    /// Apply a key typed while the search bar takes input.
    pub fn edit(&mut self, event: &Event) -> QueryEdit {
        let Event::Key(KeyEvent {
            code,
            modifiers,
            kind,
            ..
        }) = event
        else {
            return QueryEdit::Ignored;
        };
        if *kind != event::KeyEventKind::Press {
            return QueryEdit::Ignored;
        }
        match code {
            KeyCode::Esc => {
                self.clear();
                QueryEdit::Dismissed
            }
            KeyCode::Enter => {
                self.input_active = false;
                QueryEdit::Confirmed
            }
            KeyCode::Backspace => {
                self.query.pop();
                QueryEdit::Edited
            }
            // AltGr arrives as Ctrl and Alt together on Windows, and types.
            KeyCode::Char(_)
                if modifiers.contains(KeyModifiers::CONTROL)
                    != modifiers.contains(KeyModifiers::ALT) =>
            {
                QueryEdit::Ignored
            }
            KeyCode::Char(c) => {
                self.query.push(*c);
                QueryEdit::Edited
            }
            _ => QueryEdit::Ignored,
        }
    }

    /// Step to the next or previous match, wrapping around, and return it.
    /// `None`, leaving the cursor alone, when there is nothing to step over.
    pub fn advance(&mut self, dir: SearchDirection) -> Option<&M> {
        if !self.active || self.matches.is_empty() {
            return None;
        }
        let index = next_match_index(self.match_index, self.matches.len(), dir);
        self.match_index = Some(index);
        self.matches.get(index)
    }
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

    #[test]
    fn ctrl_and_alt_chords_are_not_typed_into_the_query() {
        let mut search = SearchState::<usize>::default();
        search.activate();
        for modifiers in [
            KeyModifiers::NONE,
            KeyModifiers::SHIFT,
            KeyModifiers::CONTROL,
            KeyModifiers::ALT,
            // AltGr, as Windows reports it, types a character.
            KeyModifiers::CONTROL | KeyModifiers::ALT,
        ] {
            search.edit(&Event::Key(KeyEvent::new(KeyCode::Char('x'), modifiers)));
        }
        assert_eq!(search.query, "xxx");
    }
}
