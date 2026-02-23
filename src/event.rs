// Event handling for terminal input

use anyhow::Result;
use crossterm::event::{self, Event};

/// Read the next terminal event.
///
/// Blocks until an event is available. Returns the event wrapped in Result
/// to handle potential I/O errors.
pub fn read() -> Result<Event> {
    Ok(event::read()?)
}

// Re-export commonly used types for convenience
pub use crossterm::event::KeyCode;
pub use crossterm::event::KeyModifiers;
