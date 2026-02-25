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

// Event handling for terminal input

use anyhow::Result;
use crossterm::event::{self, Event, KeyEvent};

/// Application actions derived from keyboard input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppAction {
    MoveUp,
    MoveDown,
    PageUp,
    PageDown,
    ScrollLeft,
    ScrollRight,
    ToggleDetail,
    ShowHelp,
    Reload,
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
    // To work in Windows, only care about key presses
    if let Event::Key(KeyEvent { code, kind, .. }) = event {
        if kind == event::KeyEventKind::Press {
            return match code {
                KeyCode::Up | KeyCode::Char('k') => AppAction::MoveUp,
                KeyCode::Down | KeyCode::Char('j') => AppAction::MoveDown,
                KeyCode::PageUp => AppAction::PageUp,
                KeyCode::PageDown => AppAction::PageDown,
                KeyCode::Left => AppAction::ScrollLeft,
                KeyCode::Right => AppAction::ScrollRight,
                KeyCode::Enter | KeyCode::Char('i') => AppAction::ToggleDetail,
                KeyCode::Char('h') => AppAction::ShowHelp,
                KeyCode::Char('r') => AppAction::Reload,
                KeyCode::Esc | KeyCode::Char('q') => AppAction::Quit,
                _ => AppAction::None,
            };
        }
    }
    AppAction::None
}

// Re-export commonly used types for convenience
pub use crossterm::event::KeyCode;
pub use crossterm::event::KeyModifiers;
