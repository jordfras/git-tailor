// TUI application state management

/// Application state for the TUI.
///
/// Manages the overall state of the interactive terminal interface,
/// including quit flag and commit list state.
pub struct AppState {
    pub should_quit: bool,
}

impl AppState {
    /// Create a new AppState with default values.
    pub fn new() -> Self {
        Self { should_quit: false }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
