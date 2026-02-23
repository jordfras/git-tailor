// TUI application state management

use crate::CommitInfo;

/// Application state for the TUI.
///
/// Manages the overall state of the interactive terminal interface,
/// including quit flag and commit list state.
pub struct AppState {
    pub should_quit: bool,
    pub commits: Vec<CommitInfo>,
    pub selection_index: usize,
    pub reverse: bool,
}

impl AppState {
    /// Create a new AppState with default values.
    pub fn new() -> Self {
        Self {
            should_quit: false,
            commits: Vec::new(),
            selection_index: 0,
            reverse: false,
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
