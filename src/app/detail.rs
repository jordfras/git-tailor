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

// State of the commit detail view: scroll position, diff context, file offsets.

use crate::app::ScrollState;
use crate::repo::DEFAULT_CONTEXT_LINES;

/// Number of diff context lines shown in the commit detail view. A newtype so
/// its default (git's 3) is preserved under `DetailState`'s derived `Default`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DetailContextLines(pub u32);

impl Default for DetailContextLines {
    fn default() -> Self {
        Self(DEFAULT_CONTEXT_LINES)
    }
}

/// Everything the commit detail view scrolls or measures.
#[derive(Debug, Default)]
pub struct DetailState {
    /// Vertical scroll state.
    pub v: ScrollState,
    /// Horizontal scroll state. Its `visible_height` holds the text area
    /// *width*, since that is what horizontal paging steps by.
    pub h: ScrollState,
    /// Diff context lines shown in the detail view, adjusted with `+` / `-`.
    pub context_lines: DetailContextLines,
    /// Line indices of file-diff headers in the content (updated during render).
    pub file_start_lines: Vec<usize>,
}

impl DetailState {
    /// Jump to the next file header in the detail view (wraps cyclically).
    pub fn jump_to_next_file(&mut self) {
        if self.file_start_lines.is_empty() {
            return;
        }
        let target = self
            .file_start_lines
            .iter()
            .find(|&&l| l > self.v.offset)
            .copied()
            .unwrap_or(self.file_start_lines[0]);
        let clamped = target.min(self.v.max);
        // If clamping produces no forward movement (last file is beyond max
        // scroll and we're already there), wrap to the first file.
        if clamped <= self.v.offset {
            self.v.offset = self.file_start_lines[0];
        } else {
            self.v.offset = clamped;
        }
    }

    /// Jump to the previous file header in the detail view (wraps cyclically).
    pub fn jump_to_prev_file(&mut self) {
        if self.file_start_lines.is_empty() {
            return;
        }
        let target = self
            .file_start_lines
            .iter()
            .rev()
            .find(|&&l| l < self.v.offset)
            .copied()
            .unwrap_or_else(|| *self.file_start_lines.last().unwrap());
        self.v.scroll_to(target);
    }

    /// Show one more diff context line (`+`). git re-computes the diff, so
    /// hunks whose context regions now overlap render as one.
    pub fn increase_context_lines(&mut self) {
        self.context_lines.0 = self.context_lines.0.saturating_add(1);
    }

    /// Show one fewer diff context line (`-`), with 0 as the floor. Hunks that
    /// were merged split apart again as context shrinks.
    pub fn decrease_context_lines(&mut self) {
        self.context_lines.0 = self.context_lines.0.saturating_sub(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AppState;

    #[test]
    fn detail_context_lines_default_is_git_default() {
        assert_eq!(
            AppState::default().detail.context_lines.0,
            DEFAULT_CONTEXT_LINES
        );
    }

    #[test]
    fn increase_and_decrease_detail_context_lines() {
        let mut app = AppState::default();
        app.detail.increase_context_lines();
        assert_eq!(app.detail.context_lines.0, DEFAULT_CONTEXT_LINES + 1);
        app.detail.decrease_context_lines();
        app.detail.decrease_context_lines();
        assert_eq!(app.detail.context_lines.0, DEFAULT_CONTEXT_LINES - 1);
    }

    #[test]
    fn decrease_detail_context_lines_floors_at_zero() {
        let mut app = AppState::default();
        for _ in 0..(DEFAULT_CONTEXT_LINES + 2) {
            app.detail.decrease_context_lines();
        }
        assert_eq!(app.detail.context_lines.0, 0);
    }
}
