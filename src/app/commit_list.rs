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

use crate::app::ScrollState;
use crate::app::scroll::{half_page_size, page_size};
use crate::{CommitInfo, VirtualOid};

/// The browsable commit list and its cursor.
///
/// Selection and scrolling are separate concerns here: `selection_index` is the
/// cursor and `scroll` is where the viewport sits. The viewport is remembered
/// rather than derived from the cursor, which is what lets the cursor travel
/// inside it before the list scrolls; [`follow_selection`][Self::follow_selection]
/// reconciles the two from render, where the height is known.
#[derive(Debug, Default)]
pub struct CommitListState {
    /// Rows, oldest first, with the synthetic staged/unstaged rows appended.
    pub commits: Vec<CommitInfo>,
    /// Index of the selected row.
    pub selection_index: usize,
    /// Draw newest-first instead of oldest-first.
    pub reverse: bool,
    /// Viewport position in display space. Its bounds are measured during
    /// render, so `visible_height` is 0 until the first frame.
    pub scroll: ScrollState,
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
        self.move_selection_back(page_size(self.scroll.visible_height));
    }

    /// Move the selection down by one page.
    pub fn select_page_down(&mut self) {
        self.move_selection_forward(page_size(self.scroll.visible_height));
    }

    /// Move the selection up by half a page.
    pub fn select_half_page_up(&mut self) {
        self.move_selection_back(half_page_size(self.scroll.visible_height));
    }

    /// Move the selection down by half a page.
    pub fn select_half_page_down(&mut self) {
        self.move_selection_forward(half_page_size(self.scroll.visible_height));
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

    /// The selected row's index in display space, which is mirrored when the
    /// list is drawn newest-first.
    pub fn visual_selection(&self) -> usize {
        let total = self.commits.len();
        let index = self.selection_index.min(total.saturating_sub(1));
        if self.reverse {
            total.saturating_sub(1) - index
        } else {
            index
        }
    }

    /// Record the list bounds measured during render and move the viewport the
    /// minimum needed to keep the cursor on screen.
    ///
    /// Idempotent, so the extra call split-pane mode makes each frame is free.
    pub fn follow_selection(&mut self, available_height: usize) {
        let max_scroll = self.commits.len().saturating_sub(available_height);
        self.scroll.set_bounds(max_scroll, available_height);
        if self.commits.is_empty() {
            return;
        }
        self.scroll.ensure_visible(self.visual_selection(), 1);
    }

    /// Scroll one row up (toward earlier display rows) without moving the
    /// selection. The next render pulls the viewport back if this would have
    /// scrolled the selected row off screen.
    pub fn scroll_up(&mut self) {
        self.scroll.step_back();
    }

    /// Scroll one row down without moving the selection.
    pub fn scroll_down(&mut self) {
        self.scroll.step_forward();
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

    /// Build an app with `n` commits, the given selection, and visible height,
    /// its viewport settled as the first render leaves it.
    fn app_with(n: usize, selection: usize, height: usize) -> CommitListState {
        let mut app = list_of(n, selection);
        app.follow_selection(height);
        app
    }

    /// The offset that would be rendered for the current state.
    ///
    /// The one line the behavior tests below go through, so they read the
    /// viewport the same way before and after it became stored state. Every
    /// keypress is followed by a draw, so settling here is what the loop does.
    fn offset(app: &mut CommitListState, height: usize) -> usize {
        app.follow_selection(height);
        app.scroll.offset
    }

    /// Steps taken from a settled viewport before `step` first moves it.
    ///
    /// This is what the user feels as "the cursor moves, then the list
    /// scrolls", so it is the number that must match in both directions.
    fn steps_before_the_viewport_moves(
        app: &mut CommitListState,
        height: usize,
        step: fn(&mut CommitListState),
    ) -> usize {
        let settled = offset(app, height);
        for taken in 0..app.commits.len() {
            step(app);
            if offset(app, height) != settled {
                return taken;
            }
        }
        app.commits.len()
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
    fn stepping_forward_from_the_top_moves_the_cursor_before_the_viewport() {
        let mut app = app_with(10, 0, 4);
        app.move_down();
        assert_eq!(
            offset(&mut app, 4),
            0,
            "the second row is on screen already, so nothing should scroll"
        );
    }

    #[test]
    fn stepping_back_from_the_bottom_moves_the_cursor_before_the_viewport() {
        let mut app = app_with(10, 9, 4);
        let settled = offset(&mut app, 4);
        app.move_up();
        assert_eq!(
            offset(&mut app, 4),
            settled,
            "the row above is on screen already, so nothing should scroll"
        );
    }

    #[test]
    fn both_directions_move_the_cursor_equally_far_before_scrolling() {
        for reverse in [false, true] {
            let mut from_top = app_with(10, 0, 4);
            from_top.reverse = reverse;
            let down =
                steps_before_the_viewport_moves(&mut from_top, 4, CommitListState::move_down);

            let mut from_bottom = app_with(10, 9, 4);
            from_bottom.reverse = reverse;
            let up = steps_before_the_viewport_moves(&mut from_bottom, 4, CommitListState::move_up);

            assert_eq!(
                down, up,
                "the cursor travels {down} rows away from one end but {up} from the other \
                 (reverse={reverse})"
            );
        }
    }

    #[test]
    fn the_first_render_scrolls_the_selection_into_view() {
        // Selection at index 5 of 10 with a 4-row window reveals it from the
        // bottom, which is the least the viewport can move from a cold start.
        let mut app = app_with(10, 5, 4);
        assert_eq!(offset(&mut app, 4), 2);
        assert!(selection_visible(&app, 2, 4));
    }

    #[test]
    fn scroll_down_then_up_keeps_selection_visible_and_clamps() {
        let mut app = app_with(10, 5, 4);
        // Starts with the selection at the bottom (offset 2).
        assert_eq!(offset(&mut app, 4), 2);

        // Scrolling down advances the offset until the selection hits the top…
        app.scroll_down();
        assert_eq!(offset(&mut app, 4), 3);
        app.scroll_down();
        assert_eq!(offset(&mut app, 4), 4);
        app.scroll_down();
        assert_eq!(offset(&mut app, 4), 5);
        // …then stops (further scroll would push the selection off screen).
        app.scroll_down();
        assert_eq!(offset(&mut app, 4), 5);
        assert!(selection_visible(&app, 5, 4));

        // Scrolling back up returns to the bottom-pinned offset, then stops.
        for expected in [4, 3, 2, 2] {
            app.scroll_up();
            assert_eq!(offset(&mut app, 4), expected);
            assert!(selection_visible(&app, expected, 4));
        }
    }

    #[test]
    fn scroll_keeps_selection_visible_in_reverse_mode() {
        let mut app = app_with(10, 5, 4);
        app.reverse = true;
        let off = offset(&mut app, 4);
        assert!(selection_visible(&app, off, 4));
        for _ in 0..6 {
            app.scroll_down();
            let off = offset(&mut app, 4);
            assert!(
                selection_visible(&app, off, 4),
                "offset {off} hid selection"
            );
        }
    }

    #[test]
    fn following_the_selection_twice_in_a_frame_changes_nothing() {
        // Split-pane mode measures the layout twice per frame.
        let mut app = app_with(10, 7, 4);
        let once = app.scroll.offset;
        app.follow_selection(4);
        assert_eq!(app.scroll.offset, once);
    }

    #[test]
    fn the_viewport_does_not_move_before_the_first_render() {
        let mut app = list_of(10, 9);
        app.follow_selection(0);
        assert_eq!(app.scroll.offset, 0, "no viewport to reason about yet");
    }

    #[test]
    fn jumping_to_either_end_reaches_the_very_first_and_last_row() {
        let mut app = app_with(10, 5, 4);
        app.jump_to_last();
        assert_eq!(offset(&mut app, 4), 6, "the last row sits at the bottom");
        app.jump_to_first();
        assert_eq!(offset(&mut app, 4), 0, "the first row sits at the top");
    }
}
