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

use crate::{
    CommitInfo, Oid, app::SquashMode, fragmap::FragMap, repo::ConflictState, views::theme::Theme,
};

use super::{AppMode, PendingDrop, PendingSplit, SplitStrategy};

/// Application state for the TUI.
///
/// Manages the overall state of the interactive terminal interface,
/// including quit flag and commit list state.
#[derive(Default)]
pub struct AppState {
    pub should_quit: bool,
    pub commits: Vec<CommitInfo>,
    pub selection_index: usize,
    pub reverse: bool,
    /// Show all hunk-group columns without deduplication (--full flag).
    pub full_fragmap: bool,
    /// Active fragmap rendering theme.
    pub theme: Theme,
    /// The reference OID (merge-base) used when the session started.
    /// Stored here so 'u' update can rescan from HEAD down to the same base.
    pub reference_oid: Oid,
    /// Optional fragmap visualization data.
    /// None if fragmap computation failed or was not performed.
    pub fragmap: Option<FragMap>,
    /// Horizontal scroll offset for the fragmap grid.
    pub fragmap_scroll_offset: usize,
    /// Current display mode.
    pub mode: AppMode,
    /// Vertical scroll offset for the detail view.
    pub detail_scroll_offset: usize,
    /// Maximum vertical scroll offset for the detail view (updated during render).
    pub max_detail_scroll: usize,
    /// Horizontal scroll offset for the detail view.
    pub detail_h_scroll_offset: usize,
    /// Maximum horizontal scroll offset for the detail view (updated during render).
    pub max_detail_h_scroll: usize,
    /// Visible height of the commit list area (updated during render).
    pub commit_list_visible_height: usize,
    /// Visible height of the detail view area (updated during render).
    pub detail_visible_height: usize,
    /// Transient status message shown in the footer (cleared on next keypress).
    pub status_message: Option<String>,
    /// Whether the current status message represents an error (red) or success (green).
    pub status_is_error: bool,
    /// User-controlled offset for the vertical separator bar (positive = right, negative = left).
    pub separator_offset: i16,
    /// Scroll offset for the current dialog (e.g. help). Reset when a dialog opens.
    pub dialog_scroll_offset: usize,
    /// Maximum allowed dialog scroll offset (updated during render).
    pub max_dialog_scroll: usize,
    /// Visible content height of the current dialog (updated during render, used for paging).
    pub dialog_visible_height: usize,
    /// When true, the reference_oid commit is included in the commit list.
    /// Set when the user passes `--all` to browse the complete repository history.
    pub include_reference_oid: bool,
    /// Current search query string (regex pattern).
    pub search_query: String,
    /// Whether the user is actively typing in the search bar.
    pub search_input_active: bool,
    /// Whether search results (highlights, match navigation) are active.
    pub search_active: bool,
    /// Line indices in the detail content that match the search regex.
    pub search_matches: Vec<usize>,
    /// Index into `search_matches` for the current match.
    pub search_match_index: Option<usize>,
}

impl AppState {
    /// Create a new AppState with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new AppState with the given commits, selecting the last one (HEAD).
    pub fn with_commits(commits: Vec<CommitInfo>) -> Self {
        let selection_index = commits.len().saturating_sub(1);
        Self {
            commits,
            selection_index,
            ..Self::default()
        }
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

    /// Scroll fragmap grid left.
    pub fn scroll_fragmap_left(&mut self) {
        if self.fragmap_scroll_offset > 0 {
            self.fragmap_scroll_offset -= 1;
        }
    }

    /// Scroll fragmap grid right.
    pub fn scroll_fragmap_right(&mut self) {
        self.fragmap_scroll_offset += 1;
    }

    /// Scroll detail view up (decrease offset).
    pub fn scroll_detail_up(&mut self) {
        if self.detail_scroll_offset > 0 {
            self.detail_scroll_offset -= 1;
        }
    }

    /// Scroll detail view down (increase offset).
    pub fn scroll_detail_down(&mut self) {
        if self.detail_scroll_offset < self.max_detail_scroll {
            self.detail_scroll_offset += 1;
        }
    }

    /// Scroll detail view left (decrease horizontal offset).
    pub fn scroll_detail_left(&mut self) {
        if self.detail_h_scroll_offset > 0 {
            self.detail_h_scroll_offset -= 1;
        }
    }

    /// Scroll detail view right (increase horizontal offset).
    pub fn scroll_detail_right(&mut self) {
        if self.detail_h_scroll_offset < self.max_detail_h_scroll {
            self.detail_h_scroll_offset += 1;
        }
    }

    /// Scroll commit list up by one page (visible_height lines).
    pub fn page_up(&mut self, visible_height: usize) {
        self.selection_index = self
            .selection_index
            .saturating_sub(page_size(visible_height));
    }

    /// Scroll commit list down by one page (visible_height lines).
    pub fn page_down(&mut self, visible_height: usize) {
        if self.commits.is_empty() {
            return;
        }
        let new_index = self
            .selection_index
            .saturating_add(page_size(visible_height));
        self.selection_index = new_index.min(self.commits.len() - 1);
    }

    /// Scroll commit list up by half a page (visible_height lines).
    pub fn half_page_up(&mut self, visible_height: usize) {
        self.selection_index = self
            .selection_index
            .saturating_sub(half_page_size(visible_height));
    }

    /// Scroll commit list down by half a page (visible_height lines).
    pub fn half_page_down(&mut self, visible_height: usize) {
        if self.commits.is_empty() {
            return;
        }
        let new_index = self
            .selection_index
            .saturating_add(half_page_size(visible_height));
        self.selection_index = new_index.min(self.commits.len() - 1);
    }

    /// Scroll detail view up by one page (visible_height lines).
    pub fn scroll_detail_page_up(&mut self, visible_height: usize) {
        self.detail_scroll_offset = self
            .detail_scroll_offset
            .saturating_sub(page_size(visible_height));
    }

    /// Scroll detail view down by one page (visible_height lines).
    pub fn scroll_detail_page_down(&mut self, visible_height: usize) {
        let new_offset = self
            .detail_scroll_offset
            .saturating_add(page_size(visible_height));
        self.detail_scroll_offset = new_offset.min(self.max_detail_scroll);
    }

    /// Scroll detail view up by half a page (visible_height lines).
    pub fn scroll_detail_half_page_up(&mut self, visible_height: usize) {
        self.detail_scroll_offset = self
            .detail_scroll_offset
            .saturating_sub(half_page_size(visible_height));
    }

    /// Scroll detail view down by half a page (visible_height lines).
    pub fn scroll_detail_half_page_down(&mut self, visible_height: usize) {
        let new_offset = self
            .detail_scroll_offset
            .saturating_add(half_page_size(visible_height));
        self.detail_scroll_offset = new_offset.min(self.max_detail_scroll);
    }

    /// Enter the large-split confirmation dialog.
    pub fn enter_split_confirm(
        &mut self,
        strategy: SplitStrategy,
        commit_oid: Oid,
        head_oid: Oid,
        count: usize,
    ) {
        self.enter_dialog(AppMode::SplitConfirm(PendingSplit {
            strategy,
            commit_oid,
            head_oid,
            count,
        }));
    }

    /// Cancel the large-split confirmation and return to CommitList.
    pub fn cancel_split_confirm(&mut self) {
        self.exit_dialog();
    }

    /// Enter the drop confirmation dialog.
    pub fn enter_drop_confirm(&mut self, commit_oid: Oid, commit_summary: String, head_oid: Oid) {
        self.enter_dialog(AppMode::DropConfirm(PendingDrop {
            commit_oid,
            commit_summary,
            head_oid,
        }));
    }

    /// Cancel the drop confirmation and return to CommitList.
    pub fn cancel_drop_confirm(&mut self) {
        self.exit_dialog();
    }

    /// Enter the rebase-conflict resolution dialog.
    pub fn enter_rebase_conflict(&mut self, state: ConflictState) {
        self.enter_dialog(AppMode::RebaseConflict(Box::new(state)));
    }

    /// Returns the selected commit if it is a real (non-synthetic) commit.
    /// Sets an error message and returns `None` for staged/unstaged rows.
    pub fn selected_real_commit(&mut self, action: &str) -> Option<&CommitInfo> {
        if self
            .commits
            .get(self.selection_index)
            .is_some_and(|c| c.oid.is_synthetic())
        {
            self.set_error_message(format!("Cannot {action} staged/unstaged changes"));
            return None;
        }
        self.commits.get(self.selection_index)
    }

    /// Enter split strategy selection mode.
    /// Only allowed for real commits (not staged/unstaged synthetic rows).
    pub fn enter_split_select(&mut self) {
        if self.selected_real_commit("split").is_none() {
            return;
        }
        self.enter_dialog(AppMode::SplitSelect { strategy_index: 0 });
    }

    /// Enter squash target selection mode.
    /// Only allowed for real commits (not staged/unstaged synthetic rows).
    pub fn enter_squash_select(&mut self) {
        self.enter_squash_or_fixup_select(SquashMode::Squash);
    }

    /// Enter fixup target selection mode (same UI as squash, keeps target msg).
    pub fn enter_fixup_select(&mut self) {
        self.enter_squash_or_fixup_select(SquashMode::Fixup);
    }

    fn enter_squash_or_fixup_select(&mut self, squash_mode: SquashMode) {
        let label = squash_mode.label().to_lowercase();
        if self.selected_real_commit(&label).is_none() {
            return;
        }
        let real_count = self
            .commits
            .iter()
            .filter(|c| !c.oid.is_synthetic())
            .count();
        if real_count < 2 {
            self.set_error_message(format!(
                "Nothing to {label} — only one commit on the branch"
            ));
            return;
        }
        self.mode = AppMode::SquashSelect {
            source_index: self.selection_index,
            squash_mode,
        };
    }

    /// Cancel squash selection and return to CommitList.
    pub fn cancel_squash_select(&mut self) {
        self.exit_dialog();
    }

    /// Enter move commit selection mode.
    /// The insertion cursor starts one position before the source (i.e. one
    /// slot earlier in the commit list, which visually means "above" in
    /// chronological order).
    pub fn enter_move_select(&mut self) {
        if self.selected_real_commit("move").is_none() {
            return;
        }

        // Count real (non-synthetic) commits; moving requires at least 2.
        let real_count = self
            .commits
            .iter()
            .filter(|c| !c.oid.is_synthetic())
            .count();
        if real_count < 2 {
            self.set_error_message("Nothing to move — only one commit on the branch");
            return;
        }

        let source = self.selection_index;
        let max = self.commits.len();
        // Pick the first valid (non-no-op) position. No-ops are source and
        // source + 1, so try source - 1 first, then scan forward.
        let insert_before = if source > 0 {
            source - 1
        } else {
            // source == 0 → first valid position is 2 (skip 0 and 1)
            2.min(max)
        };
        self.mode = AppMode::MoveSelect {
            source_index: source,
            insert_before,
        };
    }

    /// Cancel move selection and return to CommitList.
    pub fn cancel_move_select(&mut self) {
        self.exit_dialog();
    }

    /// Set a success status message (shown with green background).
    pub fn set_success_message(&mut self, msg: impl Into<String>) {
        self.status_message = Some(msg.into());
        self.status_is_error = false;
    }

    /// Set an error status message (shown with red background).
    pub fn set_error_message(&mut self, msg: impl Into<String>) {
        self.status_message = Some(msg.into());
        self.status_is_error = true;
    }

    /// Clear the transient status message.
    pub fn clear_status_message(&mut self) {
        self.status_message = None;
        self.status_is_error = false;
    }

    /// Toggle between CommitList and CommitDetail modes.
    pub fn toggle_detail_view(&mut self) {
        let new_mode = match &self.mode {
            AppMode::CommitList => AppMode::CommitDetail,
            AppMode::CommitDetail => {
                self.clear_search();
                AppMode::CommitList
            }
            AppMode::Help(_)
            | AppMode::SplitSelect { .. }
            | AppMode::SplitConfirm(_)
            | AppMode::DropConfirm(_)
            | AppMode::RebaseConflict(_)
            | AppMode::SquashSelect { .. }
            | AppMode::MoveSelect { .. }
            | AppMode::Loading { .. } => return,
        };
        self.mode = new_mode;
        self.detail_scroll_offset = 0;
    }

    /// Show help dialog, saving current mode to return to later.
    pub fn show_help(&mut self) {
        if !matches!(self.mode, AppMode::Help(_)) {
            let current = std::mem::replace(&mut self.mode, AppMode::CommitList);
            self.mode = AppMode::Help(Box::new(current));
            self.dialog_scroll_offset = 0;
        }
    }

    /// Scroll the current dialog up by one line.
    pub fn scroll_dialog_up(&mut self) {
        self.dialog_scroll_offset = self.dialog_scroll_offset.saturating_sub(1);
    }

    /// Scroll the current dialog down by one line.
    pub fn scroll_dialog_down(&mut self) {
        if self.dialog_scroll_offset < self.max_dialog_scroll {
            self.dialog_scroll_offset += 1;
        }
    }

    /// Scroll the current dialog up by one page.
    pub fn scroll_dialog_page_up(&mut self) {
        self.dialog_scroll_offset = self
            .dialog_scroll_offset
            .saturating_sub(page_size(self.dialog_visible_height));
    }

    /// Scroll the current dialog down by one page.
    pub fn scroll_dialog_page_down(&mut self) {
        let new = self
            .dialog_scroll_offset
            .saturating_add(page_size(self.dialog_visible_height));
        self.dialog_scroll_offset = new.min(self.max_dialog_scroll);
    }

    /// Close help dialog and return to previous mode.
    pub fn close_help(&mut self) {
        if matches!(self.mode, AppMode::Help(_)) {
            let prev = std::mem::replace(&mut self.mode, AppMode::CommitList);
            if let AppMode::Help(prev_mode) = prev {
                self.mode = *prev_mode;
            }
        }
    }

    /// Clear all search state.
    pub fn clear_search(&mut self) {
        self.search_query.clear();
        self.search_input_active = false;
        self.search_active = false;
        self.search_matches.clear();
        self.search_match_index = None;
    }

    /// Activate search mode: clear query and show search bar.
    pub fn activate_search(&mut self) {
        self.search_query.clear();
        self.search_input_active = true;
        self.search_active = true;
        self.search_matches.clear();
        self.search_match_index = None;
    }

    /// Toggle help dialog on/off.
    pub fn toggle_help(&mut self) {
        if matches!(self.mode, AppMode::Help(_)) {
            self.close_help();
        } else {
            self.show_help();
        }
    }

    fn enter_dialog(&mut self, mode: AppMode) {
        self.mode = mode;
        self.dialog_scroll_offset = 0;
    }

    fn exit_dialog(&mut self) {
        self.mode = AppMode::CommitList;
    }
}

/// Page-scroll distance for a panel of `visible_height` lines.
///
/// Keeps one line of overlap so the user retains context after paging — the
/// last visible line becomes the first visible line of the next page.
/// Always returns at least 1 so a single-line panel can still page.
fn page_size(visible_height: usize) -> usize {
    visible_height.saturating_sub(1).max(1)
}

/// Half-page step: approximately half the visible height, at least 1.
fn half_page_size(visible_height: usize) -> usize {
    (visible_height / 2).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CommitInfo, Oid, VirtualOid};

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

    #[test]
    fn test_move_up_with_empty_list() {
        let mut app = AppState::new();
        assert_eq!(app.selection_index, 0);
        app.move_up();
        assert_eq!(app.selection_index, 0);
    }

    #[test]
    fn test_move_up_at_top() {
        let mut app = AppState::new();
        app.commits = vec![
            create_test_commit("abc123", "First"),
            create_test_commit("def456", "Second"),
        ];
        app.selection_index = 0;
        app.move_up();
        assert_eq!(app.selection_index, 0);
    }

    #[test]
    fn test_move_up_from_middle() {
        let mut app = AppState::new();
        app.commits = vec![
            create_test_commit("abc123", "First"),
            create_test_commit("def456", "Second"),
            create_test_commit("ghi789", "Third"),
        ];
        app.selection_index = 2;
        app.move_up();
        assert_eq!(app.selection_index, 1);
        app.move_up();
        assert_eq!(app.selection_index, 0);
    }

    #[test]
    fn test_move_down_with_empty_list() {
        let mut app = AppState::new();
        assert_eq!(app.selection_index, 0);
        app.move_down();
        assert_eq!(app.selection_index, 0);
    }

    #[test]
    fn test_move_down_at_bottom() {
        let mut app = AppState::new();
        app.commits = vec![
            create_test_commit("abc123", "First"),
            create_test_commit("def456", "Second"),
        ];
        app.selection_index = 1;
        app.move_down();
        assert_eq!(app.selection_index, 1);
    }

    #[test]
    fn test_move_down_from_middle() {
        let mut app = AppState::new();
        app.commits = vec![
            create_test_commit("abc123", "First"),
            create_test_commit("def456", "Second"),
            create_test_commit("ghi789", "Third"),
        ];
        app.selection_index = 0;
        app.move_down();
        assert_eq!(app.selection_index, 1);
        app.move_down();
        assert_eq!(app.selection_index, 2);
    }
}
