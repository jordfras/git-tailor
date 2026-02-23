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
}

impl AppState {
    /// Create a new AppState with default values.
    pub fn new() -> Self {
        Self {
            should_quit: false,
            commits: Vec::new(),
            selection_index: 0,
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
