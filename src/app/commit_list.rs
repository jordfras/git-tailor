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

// The commit list: its rows, which one is selected, and where it is scrolled.

use crate::app::scroll::{half_page_size, page_size};
use crate::{CommitInfo, VirtualOid};

/// The browsable commit list and its cursor.
///
/// Selection and scrolling are separate concerns here: `selection_index` is the
/// cursor, while `scroll_override` is an optional viewport position that only
/// `Ctrl-Up`/`Ctrl-Down` set. Rendering derives the actual offset from both via
/// [`effective_offset`][Self::effective_offset], which is why this type has no
/// plain `max` — the bound comes from the row count.
#[derive(Debug, Default)]
pub struct CommitListState {
    /// Rows, oldest first, with the synthetic staged/unstaged rows appended.
    pub commits: Vec<CommitInfo>,
    /// Index of the selected row.
    pub selection_index: usize,
    /// Draw newest-first instead of oldest-first.
    pub reverse: bool,
    /// Visible height of the list area (updated during render).
    pub visible_height: usize,
    /// Explicit scroll offset (display space) set by `Ctrl-Up`/`Down`.
    /// `None` follows the selection (the default); `Some` is clamped each render
    /// so the selection stays visible.
    pub scroll_override: Option<usize>,
}

impl CommitListState {
    /// The selected row, if the index is in range.
    pub fn selected(&self) -> Option<&CommitInfo> {
        self.commits.get(self.selection_index)
    }

    /// The `VirtualOid` of the selected row, if any.
    pub fn selected_virtual_oid(&self) -> Option<&VirtualOid> {
        self.selected().map(|c| &c.oid)
    }

    /// Whether the selected row is the oldest real commit on the branch.
    /// Commits are stored oldest-first with the synthetic working-tree rows
    /// appended, so the oldest is simply the first real commit.
    pub fn selected_is_oldest_commit(&self) -> bool {
        self.commits
            .iter()
            .position(|c| !c.oid.is_synthetic())
            .is_some_and(|first_real| first_real == self.selection_index)
    }

    /// Number of real (non-synthetic) commits. Operations like squash and move
    /// need at least two.
    pub fn real_commit_count(&self) -> usize {
        self.commits
            .iter()
            .filter(|c| !c.oid.is_synthetic())
            .count()
    }

    /// Pull the selection back into range. `MoveSelect` navigation can leave it
    /// as a scroll anchor pointing past the last commit.
    pub fn clamp_selection(&mut self) {
        self.selection_index = self
            .selection_index
            .min(self.commits.len().saturating_sub(1));
    }

    /// Move selection up (decrement index) with lower bound check.
    /// Does nothing if already at top or commits list is empty.
    pub fn move_up(&mut self) {
        if self.selection_index > 0 {
            self.selection_index -= 1;
        }
    }

    /// Move selection down (increment index) with upper bound check.
    /// Does nothing if already at bottom or commits list is empty.
    pub fn move_down(&mut self) {
        if !self.commits.is_empty() && self.selection_index < self.commits.len() - 1 {
            self.selection_index += 1;
        }
    }

    /// Move the selection up by one page.
    pub fn select_page_up(&mut self) {
        self.move_selection_back(page_size(self.visible_height));
    }

    /// Move the selection down by one page.
    pub fn select_page_down(&mut self) {
        self.move_selection_forward(page_size(self.visible_height));
    }

    /// Move the selection up by half a page.
    pub fn select_half_page_up(&mut self) {
        self.move_selection_back(half_page_size(self.visible_height));
    }

    /// Move the selection down by half a page.
    pub fn select_half_page_down(&mut self) {
        self.move_selection_forward(half_page_size(self.visible_height));
    }

    fn move_selection_back(&mut self, step: usize) {
        self.selection_index = self.selection_index.saturating_sub(step);
    }

    fn move_selection_forward(&mut self, step: usize) {
        if self.commits.is_empty() {
            return;
        }
        let new_index = self.selection_index.saturating_add(step);
        self.selection_index = new_index.min(self.commits.len() - 1);
    }

    /// Jump to the first commit in the list.
    pub fn jump_to_first(&mut self) {
        self.selection_index = 0;
    }

    /// Jump to the last commit in the list.
    pub fn jump_to_last(&mut self) {
        self.selection_index = self.commits.len().saturating_sub(1);
    }

    /// The scroll offset (in display space) to render, given the visible
    /// height. Without an override it follows the selection (pinned to the
    /// bottom once scrolled, the historical behavior); with one it honors the
    /// override but always clamps so the selected row stays visible.
    pub fn effective_offset(&self, available_height: usize) -> usize {
        let total = self.commits.len();
        if total == 0 || available_height == 0 {
            return 0;
        }
        // Selected row in display space (the list is drawn reversed when `reverse`).
        let visual_selection = if self.reverse {
            total - 1 - self.selection_index.min(total - 1)
        } else {
            self.selection_index.min(total - 1)
        };
        let max_scroll = total.saturating_sub(available_height);
        // Range of offsets that keep the selection on screen: from "selection at
        // the bottom row" up to "selection at the top row" (never past max_scroll).
        let min_off = visual_selection.saturating_sub(available_height - 1);
        let max_off = visual_selection.min(max_scroll);
        let derived = if visual_selection < available_height {
            0
        } else {
            visual_selection - (available_height - 1)
        };
        self.scroll_override
            .unwrap_or(derived)
            .clamp(min_off, max_off)
    }

    /// Scroll one row up (toward earlier display rows) without moving the
    /// selection. Clamped on render so it never scrolls the selected row off
    /// screen.
    pub fn scroll_up(&mut self) {
        let base = self.effective_offset(self.visible_height);
        self.scroll_override = Some(base.saturating_sub(1));
    }

    /// Scroll one row down without moving the selection.
    pub fn scroll_down(&mut self) {
        let base = self.effective_offset(self.visible_height);
        self.scroll_override = Some(base + 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Oid;

    fn create_test_commit(oid: &str, summary: &str) -> CommitInfo {
        CommitInfo {
            oid: VirtualOid::Real(Oid::from(oid)),
            summary: summary.to_string(),
            author: Some("Test Author".to_string()),
            date: Some("2024-01-01".to_string()),
            parent_oids: vec![],
            message: summary.to_string(),
            author_email: Some("test@example.com".to_string()),
            author_date: Some(time::OffsetDateTime::from_unix_timestamp(1704110400).unwrap()),
            committer: Some("Test Committer".to_string()),
            committer_email: Some("committer@example.com".to_string()),
            commit_date: Some(time::OffsetDateTime::from_unix_timestamp(1704110400).unwrap()),
        }
    }

    /// A list of `n` commits with the given selection.
    fn list_of(n: usize, selection: usize) -> CommitListState {
        CommitListState {
            commits: (0..n)
                .map(|i| create_test_commit(&format!("{i:040x}"), &format!("c{i}")))
                .collect(),
            selection_index: selection,
            ..Default::default()
        }
    }

    #[test]
    fn test_move_up_with_empty_list() {
        let mut app = CommitListState::default();
        assert_eq!(app.selection_index, 0);
        app.move_up();
        assert_eq!(app.selection_index, 0);
    }

    #[test]
    fn test_move_up_at_top() {
        let mut app = list_of(2, 0);
        app.move_up();
        assert_eq!(app.selection_index, 0);
    }

    #[test]
    fn test_move_up_from_middle() {
        let mut app = list_of(3, 2);
        app.move_up();
        assert_eq!(app.selection_index, 1);
        app.move_up();
        assert_eq!(app.selection_index, 0);
    }

    #[test]
    fn test_move_down_with_empty_list() {
        let mut app = CommitListState::default();
        assert_eq!(app.selection_index, 0);
        app.move_down();
        assert_eq!(app.selection_index, 0);
    }

    #[test]
    fn test_move_down_at_bottom() {
        let mut app = list_of(2, 1);
        app.move_down();
        assert_eq!(app.selection_index, 1);
    }

    #[test]
    fn test_move_down_from_middle() {
        let mut app = list_of(3, 0);
        app.move_down();
        assert_eq!(app.selection_index, 1);
        app.move_down();
        assert_eq!(app.selection_index, 2);
    }

    /// Build an app with `n` commits, the given selection, and visible height.
    fn app_with(n: usize, selection: usize, height: usize) -> CommitListState {
        CommitListState {
            visible_height: height,
            ..list_of(n, selection)
        }
    }

    /// The selected row's display index must lie within the visible window.
    fn selection_visible(app: &CommitListState, offset: usize, height: usize) -> bool {
        let total = app.commits.len();
        let visual = if app.reverse {
            total - 1 - app.selection_index
        } else {
            app.selection_index
        };
        offset <= visual && visual < offset + height
    }

    #[test]
    fn effective_offset_without_override_follows_selection() {
        // Selection at index 5 of 10 with a 4-row window pins it to the bottom.
        let app = app_with(10, 5, 4);
        assert_eq!(app.scroll_override, None);
        assert_eq!(app.effective_offset(4), 2);
        assert!(selection_visible(&app, 2, 4));
    }

    #[test]
    fn scroll_down_then_up_keeps_selection_visible_and_clamps() {
        let mut app = app_with(10, 5, 4);
        // Starts with the selection at the bottom (offset 2).
        assert_eq!(app.effective_offset(4), 2);

        // Scrolling down advances the offset until the selection hits the top…
        app.scroll_down();
        assert_eq!(app.effective_offset(4), 3);
        app.scroll_down();
        assert_eq!(app.effective_offset(4), 4);
        app.scroll_down();
        assert_eq!(app.effective_offset(4), 5);
        // …then stops (further scroll would push the selection off screen).
        app.scroll_down();
        assert_eq!(app.effective_offset(4), 5);
        assert!(selection_visible(&app, 5, 4));

        // Scrolling back up returns to the bottom-pinned offset, then stops.
        for expected in [4, 3, 2, 2] {
            app.scroll_up();
            assert_eq!(app.effective_offset(4), expected);
            assert!(selection_visible(&app, expected, 4));
        }
    }

    #[test]
    fn scroll_keeps_selection_visible_in_reverse_mode() {
        let mut app = app_with(10, 5, 4);
        app.reverse = true;
        let off = app.effective_offset(4);
        assert!(selection_visible(&app, off, 4));
        for _ in 0..6 {
            app.scroll_down();
            let off = app.effective_offset(4);
            assert!(
                selection_visible(&app, off, 4),
                "offset {off} hid selection"
            );
        }
    }
}
