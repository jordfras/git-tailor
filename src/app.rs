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

// TUI application state management

use crate::{fragmap::FragMap, CommitInfo};

/// Split strategy options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitStrategy {
    PerFile,
    PerHunk,
    PerHunkCluster,
}

impl SplitStrategy {
    pub const ALL: [SplitStrategy; 3] = [
        SplitStrategy::PerFile,
        SplitStrategy::PerHunk,
        SplitStrategy::PerHunkCluster,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SplitStrategy::PerFile => "Per file",
            SplitStrategy::PerHunk => "Per hunk",
            SplitStrategy::PerHunkCluster => "Per hunk group",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            SplitStrategy::PerFile => "Create one commit per changed file",
            SplitStrategy::PerHunk => "Create one commit per diff hunk",
            SplitStrategy::PerHunkCluster => "Create one commit per hunk group",
        }
    }
}

/// Application display mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppMode {
    /// Commit list view with fragmap.
    CommitList,
    /// Detailed view of a single commit.
    CommitDetail,
    /// Split strategy selection dialog; carries the highlighted option index.
    SplitSelect { strategy_index: usize },
    /// Confirmation dialog for large splits (> SPLIT_CONFIRM_THRESHOLD commits).
    SplitConfirm(PendingSplit),
    /// Help dialog overlay; carries the mode to return to when closed.
    Help(Box<AppMode>),
}

/// Data retained while the user is shown the large-split confirmation dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingSplit {
    pub strategy: SplitStrategy,
    pub commit_oid: String,
    pub head_oid: String,
    pub count: usize,
}

/// Application state for the TUI.
///
/// Manages the overall state of the interactive terminal interface,
/// including quit flag and commit list state.
pub struct AppState {
    pub should_quit: bool,
    pub commits: Vec<CommitInfo>,
    pub selection_index: usize,
    pub reverse: bool,
    /// Show all hunk-group columns without deduplication (--full flag).
    pub full_fragmap: bool,
    /// The reference OID (merge-base) used when the session started.
    /// Stored here so 'r' reload can rescan from HEAD down to the same base.
    pub reference_oid: String,
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
    /// Visible height of the commit list area (updated during render).
    pub commit_list_visible_height: usize,
    /// Visible height of the detail view area (updated during render).
    pub detail_visible_height: usize,
    /// Transient status message shown in the footer (cleared on next keypress).
    pub status_message: Option<String>,
}

impl AppState {
    /// Create a new AppState with default values.
    pub fn new() -> Self {
        Self {
            should_quit: false,
            commits: Vec::new(),
            selection_index: 0,
            reverse: false,
            full_fragmap: false,
            reference_oid: String::new(),
            fragmap: None,
            fragmap_scroll_offset: 0,
            mode: AppMode::CommitList,
            detail_scroll_offset: 0,
            max_detail_scroll: 0,
            commit_list_visible_height: 0,
            detail_visible_height: 0,
            status_message: None,
        }
    }

    /// Create a new AppState with the given commits, selecting the last one (HEAD).
    pub fn with_commits(commits: Vec<CommitInfo>) -> Self {
        let selection_index = commits.len().saturating_sub(1);
        Self {
            should_quit: false,
            commits,
            selection_index,
            reverse: false,
            full_fragmap: false,
            reference_oid: String::new(),
            fragmap: None,
            fragmap_scroll_offset: 0,
            mode: AppMode::CommitList,
            detail_scroll_offset: 0,
            max_detail_scroll: 0,
            commit_list_visible_height: 0,
            detail_visible_height: 0,
            status_message: None,
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

    /// Scroll commit list up by one page (visible_height lines).
    pub fn page_up(&mut self, visible_height: usize) {
        let page_size = visible_height.saturating_sub(1).max(1); // Keep at least one line overlap
        self.selection_index = self.selection_index.saturating_sub(page_size);
    }

    /// Scroll commit list down by one page (visible_height lines).
    pub fn page_down(&mut self, visible_height: usize) {
        if self.commits.is_empty() {
            return;
        }
        let page_size = visible_height.saturating_sub(1).max(1); // Keep at least one line overlap
        let new_index = self.selection_index.saturating_add(page_size);
        self.selection_index = new_index.min(self.commits.len() - 1);
    }

    /// Scroll detail view up by one page (visible_height lines).
    pub fn scroll_detail_page_up(&mut self, visible_height: usize) {
        let page_size = visible_height.saturating_sub(1).max(1);
        self.detail_scroll_offset = self.detail_scroll_offset.saturating_sub(page_size);
    }

    /// Scroll detail view down by one page (visible_height lines).
    pub fn scroll_detail_page_down(&mut self, visible_height: usize) {
        let page_size = visible_height.saturating_sub(1).max(1);
        let new_offset = self.detail_scroll_offset.saturating_add(page_size);
        self.detail_scroll_offset = new_offset.min(self.max_detail_scroll);
    }

    /// Enter the large-split confirmation dialog.
    pub fn enter_split_confirm(
        &mut self,
        strategy: SplitStrategy,
        commit_oid: String,
        head_oid: String,
        count: usize,
    ) {
        self.mode = AppMode::SplitConfirm(PendingSplit {
            strategy,
            commit_oid,
            head_oid,
            count,
        });
    }

    /// Cancel the large-split confirmation and return to CommitList.
    pub fn cancel_split_confirm(&mut self) {
        self.mode = AppMode::CommitList;
    }

    /// Enter split strategy selection mode.
    /// Only allowed for real commits (not staged/unstaged synthetic rows).
    pub fn enter_split_select(&mut self) {
        if let Some(commit) = self.commits.get(self.selection_index) {
            if commit.oid == "staged" || commit.oid == "unstaged" {
                self.status_message = Some("Cannot split staged/unstaged changes".to_string());
                return;
            }
        }
        self.mode = AppMode::SplitSelect { strategy_index: 0 };
    }

    /// Clear the transient status message.
    pub fn clear_status_message(&mut self) {
        self.status_message = None;
    }

    /// Move split strategy selection up.
    pub fn split_select_up(&mut self) {
        if let AppMode::SplitSelect { strategy_index } = &mut self.mode {
            if *strategy_index > 0 {
                *strategy_index -= 1;
            }
        }
    }

    /// Move split strategy selection down.
    pub fn split_select_down(&mut self) {
        if let AppMode::SplitSelect { strategy_index } = &mut self.mode {
            if *strategy_index < SplitStrategy::ALL.len() - 1 {
                *strategy_index += 1;
            }
        }
    }

    /// Get the currently selected split strategy.
    pub fn selected_split_strategy(&self) -> SplitStrategy {
        if let AppMode::SplitSelect { strategy_index } = self.mode {
            SplitStrategy::ALL[strategy_index]
        } else {
            SplitStrategy::ALL[0]
        }
    }

    /// Toggle between CommitList and CommitDetail modes.
    pub fn toggle_detail_view(&mut self) {
        let new_mode = match &self.mode {
            AppMode::CommitList => AppMode::CommitDetail,
            AppMode::CommitDetail => AppMode::CommitList,
            AppMode::Help(_) | AppMode::SplitSelect { .. } | AppMode::SplitConfirm(_) => return,
        };
        self.mode = new_mode;
        self.detail_scroll_offset = 0;
    }

    /// Show help dialog, saving current mode to return to later.
    pub fn show_help(&mut self) {
        if !matches!(self.mode, AppMode::Help(_)) {
            let current = std::mem::replace(&mut self.mode, AppMode::CommitList);
            self.mode = AppMode::Help(Box::new(current));
        }
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

    /// Toggle help dialog on/off.
    pub fn toggle_help(&mut self) {
        if matches!(self.mode, AppMode::Help(_)) {
            self.close_help();
        } else {
            self.show_help();
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_commit(oid: &str, summary: &str) -> CommitInfo {
        CommitInfo {
            oid: oid.to_string(),
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
