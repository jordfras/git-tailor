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

/// How many rows to keep between the cursor and the edge of the commit list,
/// so there is context visible in the direction of travel.
///
/// A newtype-style enum so its default survives `CommitListState`'s derived
/// `Default`, the same reason [`DetailContextLines`][crate::app::DetailContextLines]
/// is one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ScrollMargin {
    /// Scale with the window: a sixth of the visible rows.
    #[default]
    Auto,
    /// Exactly this many rows, `0` letting the cursor reach the edge.
    Fixed(usize),
}

impl ScrollMargin {
    /// Rows to keep past the cursor in a viewport of `visible_height`.
    pub fn rows(self, visible_height: usize) -> usize {
        match self {
            // Three rows on a full-screen 24-row terminal, and nothing on a
            // window too short to give any up without crowding the cursor.
            Self::Auto => visible_height / 6,
            Self::Fixed(rows) => rows,
        }
    }
}

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
    /// Rows kept between the cursor and the top or bottom of the list.
    pub scroll_margin: ScrollMargin,
    /// Selection the viewport was last settled against, so `follow_selection`
    /// can tell the cursor having moved from the viewport having been scrolled
    /// under a standing cursor. Bookkeeping, so unlike its neighbors it is
    /// private: setting it by hand silently turns the margin off for a frame.
    settled_for: Option<usize>,
}

/// Everything [`follow_selection`][CommitListState::follow_selection] writes.
///
/// A caller that has to measure a layout it will not draw takes one of these
/// first and puts it back after, so the measurement cannot decide where the
/// list is scrolled — see `views::commit_list::compute_fragmap_sep_x`.
#[derive(Debug, Clone, Copy)]
pub struct ViewportSnapshot {
    scroll: ScrollState,
    settled_for: Option<usize>,
}

impl CommitListState {
    /// A list of `commits` with `selection_index` selected, everything else
    /// left at its default. A constructor rather than struct-update syntax
    /// because the viewport bookkeeping above is private.
    pub fn with_selection(commits: Vec<CommitInfo>, selection_index: usize) -> Self {
        Self {
            commits,
            selection_index,
            ..Default::default()
        }
    }

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
    /// Idempotent at a fixed `available_height`, but *not* across two different
    /// ones: it only ever moves the minimum, so a call with a shorter height
    /// scrolls further than the taller one would and the taller one will not
    /// bring it back. Only the layout actually being drawn may call this —
    /// `compute_fragmap_sep_x` measures a hypothetical one and restores the
    /// viewport for that reason.
    pub fn follow_selection(&mut self, available_height: usize) {
        let max_scroll = self.commits.len().saturating_sub(available_height);
        self.scroll.set_bounds(max_scroll, available_height);
        if self.commits.is_empty() {
            return;
        }
        // The margin says where the cursor should sit *as it moves*. When the
        // cursor stood still the viewport is wherever Ctrl-Up/Ctrl-Down put it,
        // and dragging it back to honor the margin would make those keys dead
        // `margin` rows from each end — so then only keep the cursor on screen.
        // A resize lands here too, and holding the view still is right there as
        // well; the margin reasserts itself on the next cursor move.
        let margin = if self.settled_for == Some(self.selection_index) {
            0
        } else {
            self.scroll_margin.rows(available_height)
        };
        self.scroll
            .ensure_visible_with_margin(self.visual_selection(), 1, margin);
        self.settled_for = Some(self.selection_index);
    }

    /// Take everything `follow_selection` writes, to put back with
    /// [`restore_viewport`][Self::restore_viewport].
    pub fn viewport_snapshot(&self) -> ViewportSnapshot {
        ViewportSnapshot {
            scroll: self.scroll,
            settled_for: self.settled_for,
        }
    }

    /// Put back a [`viewport_snapshot`][Self::viewport_snapshot].
    pub fn restore_viewport(&mut self, snapshot: ViewportSnapshot) {
        self.scroll = snapshot.scroll;
        self.settled_for = snapshot.settled_for;
    }

    /// Send the viewport back to the top and forget what it was settled
    /// against, so the next render places the cursor from scratch. For a
    /// reload, where the rows and the selection are both replaced.
    pub fn reset_viewport(&mut self) {
        self.scroll.offset = 0;
        self.settled_for = None;
    }

    /// Scroll one row up (toward earlier display rows) without moving the
    /// selection.
    ///
    /// The next render pulls the viewport back as far as the margin wants the
    /// cursor from the edge, not merely far enough to keep it on screen — so
    /// with a margin these keys stop short of the list ends, and the outermost
    /// rows can only be brought on screen by moving the cursor.
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
            summary_key: summary.into(),
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
    fn following_the_selection_twice_at_one_height_changes_nothing() {
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

    /// The offset after the cursor *moves* to `selection` in a 20-row list
    /// whose 9-row window is showing rows 6..15, with margin `margin`.
    ///
    /// The move matters: the margin only governs a settle the cursor caused,
    /// so the selection is assigned after the viewport is placed.
    fn settled_at(selection: usize, margin: usize) -> usize {
        let mut app = app_with(20, 0, 9);
        app.scroll_margin = ScrollMargin::Fixed(margin);
        app.scroll.offset = 6;
        app.selection_index = selection;
        offset(&mut app, 9)
    }

    #[test]
    fn a_margin_starts_the_scroll_early_in_both_directions() {
        // Row 13 is three short of the bottom edge, so only a margin reaches it.
        assert_eq!(settled_at(13, 0), 6, "without a margin, nothing moves yet");
        assert_eq!(settled_at(13, 2), 7, "a margin of 2 scrolls two rows early");

        // And the mirror image at the top edge.
        assert_eq!(settled_at(7, 0), 6, "without a margin, nothing moves yet");
        assert_eq!(settled_at(7, 2), 5, "a margin of 2 scrolls two rows early");
    }

    #[test]
    fn auto_margin_scales_with_the_window_and_vanishes_when_tiny() {
        assert_eq!(ScrollMargin::Auto.rows(22), 3, "a full-screen terminal");
        assert_eq!(ScrollMargin::Auto.rows(4), 0, "no room to give any up");
    }

    #[test]
    fn viewport_scrolling_reaches_both_ends_despite_the_margin() {
        let mut app = app_with(40, 20, 22);
        app.scroll_margin = ScrollMargin::Fixed(3);

        let mut lowest = offset(&mut app, 22);
        for _ in 0..40 {
            app.scroll_up();
            lowest = lowest.min(offset(&mut app, 22));
        }
        assert_eq!(lowest, 0, "Ctrl-Up must reach the first row of the list");

        let mut highest = lowest;
        for _ in 0..60 {
            app.scroll_down();
            highest = highest.max(offset(&mut app, 22));
        }
        assert_eq!(
            highest, app.scroll.max,
            "Ctrl-Down must reach the last row of the list"
        );
        assert_eq!(
            app.selection_index, 20,
            "scrolling the viewport must not move the cursor"
        );
    }

    #[test]
    fn moving_the_cursor_after_a_viewport_scroll_restores_the_margin() {
        let mut app = app_with(40, 20, 22);
        app.scroll_margin = ScrollMargin::Fixed(3);
        for _ in 0..30 {
            app.scroll_up();
            offset(&mut app, 22);
        }
        assert_eq!(app.scroll.offset, 0, "scrolled to the top, cursor held");

        app.move_up();
        assert_eq!(
            offset(&mut app, 22),
            1,
            "the margin governs the cursor again as soon as it moves"
        );
    }

    #[test]
    fn a_margin_does_not_stop_either_end_being_reached() {
        let mut app = app_with(20, 10, 9);
        app.scroll_margin = ScrollMargin::Fixed(3);
        app.jump_to_last();
        assert_eq!(offset(&mut app, 9), 11, "the last row, with no blank space");
        assert_eq!(app.visual_selection(), 19);
        app.jump_to_first();
        assert_eq!(offset(&mut app, 9), 0, "the first row, with none above");
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
