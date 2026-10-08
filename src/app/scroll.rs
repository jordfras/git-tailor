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

// Reusable scroll offset + bounds for a scrollable panel.

/// Scroll position of one panel along one axis.
///
/// `max` and `visible_height` are measured during render; `offset` is moved by
/// key handling. Method names are direction-neutral (`back`/`forward`) so the
/// same type reads correctly for horizontal scrolling.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ScrollState {
    pub offset: usize,
    pub max: usize,
    pub visible_height: usize,
}

impl ScrollState {
    /// Record the bounds measured during render, clamping the offset to them.
    ///
    /// Clamping here rather than later in the render pass keeps `offset` equal
    /// to what will actually be drawn, so anything reading it in between — the
    /// detail view's search auto-scroll asking whether a match is on screen —
    /// sees the real viewport. A stale offset also makes scrolling back appear
    /// frozen, since each keypress only walks it down invisibly until it
    /// re-enters range.
    pub fn set_bounds(&mut self, max: usize, visible_height: usize) {
        self.max = max;
        self.visible_height = visible_height;
        self.clamp_offset();
    }

    /// Clamp the offset to the current bounds.
    pub fn clamp_offset(&mut self) {
        self.offset = self.offset.min(self.max);
    }

    /// Jump to `target`, clamped to the current bounds.
    pub fn scroll_to(&mut self, target: usize) {
        self.offset = target.min(self.max);
    }

    /// Scroll the minimum needed to bring rows `start..start + height` on
    /// screen, moving nothing if they already fit.
    ///
    /// An item taller than the viewport pins its top: the "above" case is tested
    /// first, so you see the start of an over-long item rather than its end.
    /// Does nothing before the first render, when `visible_height` is still 0
    /// and there is no viewport to reason about.
    pub fn ensure_visible(&mut self, start: usize, height: usize) {
        self.ensure_visible_with_margin(start, height, 0);
    }

    /// [`ensure_visible`][Self::ensure_visible], keeping `margin` further rows
    /// on screen either side of the item.
    ///
    /// A margin needs `2 * margin` rows of viewport beyond the item to hold on
    /// both sides, so it is capped at half of what is spare: past that the item
    /// centers rather than the offset flipping between the two conditions.
    ///
    /// The ends of the content are not padded — `clamp_offset` gives the margin
    /// up there instead of scrolling past the last row, so the first and last
    /// item still reach the top and bottom row.
    pub fn ensure_visible_with_margin(&mut self, start: usize, height: usize, margin: usize) {
        if self.visible_height == 0 {
            return;
        }
        let margin = margin.min(self.visible_height.saturating_sub(height) / 2);
        let lo = start.saturating_sub(margin);
        let hi = start + height + margin;
        if lo < self.offset {
            self.offset = lo;
        } else if hi > self.offset + self.visible_height {
            self.offset = hi - self.visible_height;
        }
        self.clamp_offset();
    }

    pub fn step_back(&mut self) {
        self.offset = self.offset.saturating_sub(1);
    }

    pub fn step_forward(&mut self) {
        if self.offset < self.max {
            self.offset += 1;
        }
    }

    pub fn to_start(&mut self) {
        self.offset = 0;
    }

    pub fn to_end(&mut self) {
        self.offset = self.max;
    }

    pub fn page_back(&mut self) {
        self.offset = self.offset.saturating_sub(page_size(self.visible_height));
    }

    pub fn page_forward(&mut self) {
        let new = self.offset.saturating_add(page_size(self.visible_height));
        self.offset = new.min(self.max);
    }

    pub fn half_page_back(&mut self) {
        self.offset = self
            .offset
            .saturating_sub(half_page_size(self.visible_height));
    }

    pub fn half_page_forward(&mut self) {
        let new = self
            .offset
            .saturating_add(half_page_size(self.visible_height));
        self.offset = new.min(self.max);
    }
}

/// Page-scroll distance for a panel of `visible_height` lines.
///
/// Keeps one line of overlap so the user retains context after paging — the
/// last visible line becomes the first visible line of the next page.
/// Always returns at least 1 so a single-line panel can still page.
pub(crate) fn page_size(visible_height: usize) -> usize {
    visible_height.saturating_sub(1).max(1)
}

/// Half-page step: approximately half the visible height, at least 1.
pub(crate) fn half_page_size(visible_height: usize) -> usize {
    (visible_height / 2).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(offset: usize, max: usize, visible_height: usize) -> ScrollState {
        ScrollState {
            offset,
            max,
            visible_height,
        }
    }

    #[test]
    fn ensure_visible_leaves_an_item_already_on_screen_alone() {
        let mut s = state(4, 20, 10);
        s.ensure_visible(6, 1);
        assert_eq!(s.offset, 4);
    }

    #[test]
    fn ensure_visible_scrolls_down_by_the_minimum() {
        let mut s = state(0, 20, 10);
        s.ensure_visible(10, 1);
        assert_eq!(s.offset, 1, "one row is enough to reveal row 10");
    }

    #[test]
    fn ensure_visible_scrolls_up_to_the_item_top() {
        let mut s = state(8, 20, 10);
        s.ensure_visible(3, 1);
        assert_eq!(s.offset, 3);
    }

    #[test]
    fn ensure_visible_accounts_for_item_height() {
        let mut s = state(0, 20, 10);
        // Rows 8..11 — the last two are past the bottom.
        s.ensure_visible(8, 3);
        assert_eq!(s.offset, 1);
    }

    #[test]
    fn ensure_visible_pins_the_top_of_an_over_tall_item() {
        let mut s = state(5, 20, 4);
        s.ensure_visible(2, 10);
        assert_eq!(s.offset, 2, "show the start, not the end, of a long item");
    }

    #[test]
    fn ensure_visible_clamps_to_max() {
        let mut s = state(0, 3, 10);
        s.ensure_visible(30, 1);
        assert_eq!(s.offset, 3);
    }

    #[test]
    fn ensure_visible_does_nothing_before_the_first_render() {
        let mut s = state(0, 0, 0);
        s.ensure_visible(30, 1);
        assert_eq!(s.offset, 0);
    }

    #[test]
    fn a_margin_scrolls_early_to_keep_rows_beyond_the_item() {
        let mut s = state(0, 20, 10);
        s.ensure_visible_with_margin(8, 1, 2);
        assert_eq!(s.offset, 1, "row 10 must be on screen, not just row 8");
    }

    #[test]
    fn a_margin_applies_the_same_way_scrolling_back() {
        let mut s = state(8, 20, 10);
        s.ensure_visible_with_margin(9, 1, 2);
        assert_eq!(s.offset, 7, "row 7 must be on screen, not just row 9");
    }

    #[test]
    fn a_margin_leaves_an_item_with_room_to_spare_alone() {
        let mut s = state(4, 20, 10);
        s.ensure_visible_with_margin(7, 1, 2);
        assert_eq!(s.offset, 4, "rows 5..10 are all on screen already");
    }

    #[test]
    fn a_margin_wider_than_the_viewport_centers_instead_of_oscillating() {
        let mut s = state(0, 40, 9);
        s.ensure_visible_with_margin(20, 1, 99);
        assert_eq!(s.offset, 16, "capped to 4, which centers row 20 of 9");
        // The second call must agree with the first, or the view would flicker.
        s.ensure_visible_with_margin(20, 1, 99);
        assert_eq!(s.offset, 16);
    }

    #[test]
    fn a_margin_is_given_up_at_the_start_of_the_content() {
        let mut s = state(5, 20, 10);
        s.ensure_visible_with_margin(0, 1, 3);
        assert_eq!(s.offset, 0, "nothing above row 0 to keep on screen");
    }

    #[test]
    fn a_margin_is_given_up_at_the_end_of_the_content() {
        let mut s = state(0, 3, 10);
        s.ensure_visible_with_margin(12, 1, 3);
        assert_eq!(s.offset, 3, "clamped to max, so row 12 is the last row");
    }
}
