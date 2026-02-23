// TUI application state management

use crate::{fragmap::FragMap, CommitInfo};

/// Application display mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    /// Commit list view with fragmap.
    CommitList,
    /// Detailed view of a single commit.
    CommitDetail,
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

    /// Toggle between CommitList and CommitDetail modes.
    pub fn toggle_detail_view(&mut self) {
        self.mode = match self.mode {
            AppMode::CommitList => AppMode::CommitDetail,
            AppMode::CommitDetail => AppMode::CommitList,
        };
        // Reset scroll offset when toggling views
        self.detail_scroll_offset = 0;
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
