// TUI application state management

use crate::{fragmap::FragMap, CommitInfo};

/// Application display mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    /// Commit list view with fragmap.
    CommitList,
    /// Detailed view of a single commit.
    CommitDetail,
    /// Help dialog overlay.
    Help,
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
    /// Previous mode before showing help (to return to after closing help).
    pub previous_mode: Option<AppMode>,
}

impl AppState {
    /// Create a new AppState with default values.
    pub fn new() -> Self {
        Self {
            should_quit: false,
            commits: Vec::new(),
            selection_index: 0,
            reverse: false,
            fragmap: None,
            fragmap_scroll_offset: 0,
            mode: AppMode::CommitList,
            detail_scroll_offset: 0,
            max_detail_scroll: 0,
            commit_list_visible_height: 0,
            detail_visible_height: 0,
            previous_mode: None,
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
            fragmap: None,
            fragmap_scroll_offset: 0,
            mode: AppMode::CommitList,
            detail_scroll_offset: 0,
            max_detail_scroll: 0,
            commit_list_visible_height: 0,
            detail_visible_height: 0,
            previous_mode: None,
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

    /// Toggle between CommitList and CommitDetail modes.
    pub fn toggle_detail_view(&mut self) {
        self.mode = match self.mode {
            AppMode::CommitList => AppMode::CommitDetail,
            AppMode::CommitDetail => AppMode::CommitList,
            AppMode::Help => AppMode::Help, // Stay in help if already there
        };
        // Reset scroll offset when toggling views
        self.detail_scroll_offset = 0;
    }

    /// Show help dialog, saving current mode to return to later.
    pub fn show_help(&mut self) {
        if self.mode != AppMode::Help {
            self.previous_mode = Some(self.mode);
            self.mode = AppMode::Help;
        }
    }

    /// Close help dialog and return to previous mode.
    pub fn close_help(&mut self) {
        if self.mode == AppMode::Help {
            self.mode = self.previous_mode.unwrap_or(AppMode::CommitList);
            self.previous_mode = None;
        }
    }

    /// Toggle help dialog on/off.
    pub fn toggle_help(&mut self) {
        if self.mode == AppMode::Help {
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
            author: "Test Author".to_string(),
            date: "2024-01-01".to_string(),
            parent_oids: vec![],
            message: summary.to_string(),
            author_email: "test@example.com".to_string(),
            author_date: time::OffsetDateTime::from_unix_timestamp(1704110400).unwrap(),
            committer: "Test Committer".to_string(),
            committer_email: "committer@example.com".to_string(),
            commit_date: time::OffsetDateTime::from_unix_timestamp(1704110400).unwrap(),
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
