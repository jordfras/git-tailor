// Event handling for terminal input

use anyhow::Result;
use crossterm::event::{self, Event, KeyEvent};

/// Application actions derived from keyboard input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppAction {
    MoveUp,
    MoveDown,
    ScrollLeft,
    ScrollRight,
    ToggleDetail,
    Quit,
    None,
}

/// Read the next terminal event.
///
/// Blocks until an event is available. Returns the event wrapped in Result
/// to handle potential I/O errors.
pub fn read() -> Result<Event> {
    Ok(event::read()?)
}

/// Parse a terminal event into an application action.
///
/// Recognizes arrow keys for navigation, 'i' to toggle detail view, and Esc to
/// exit detail view or quit application.
/// Returns AppAction::None for unrecognized events.
pub fn parse_key_event(event: Event) -> AppAction {
    match event {
        Event::Key(KeyEvent { code, .. }) => match code {
            KeyCode::Up => AppAction::MoveUp,
            KeyCode::Down => AppAction::MoveDown,
            KeyCode::Left => AppAction::ScrollLeft,
            KeyCode::Right => AppAction::ScrollRight,
            KeyCode::Char('i') => AppAction::ToggleDetail,
            KeyCode::Esc => AppAction::Quit,
            _ => AppAction::None,
        },
        _ => AppAction::None,
    }
}

// Re-export commonly used types for convenience
pub use crossterm::event::KeyCode;
pub use crossterm::event::KeyModifiers;
