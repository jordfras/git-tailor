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

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};

use super::AppMode;

/// Semantic commands derived from keyboard input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCommand {
    MoveUp,
    MoveDown,
    PageUp,
    PageDown,
    HalfPageUp,
    HalfPageDown,
    JumpToTop,
    JumpToBottom,
    ScrollToLeftEdge,
    ScrollToRightEdge,
    ScrollLeft,
    ScrollRight,
    NavFileNext,
    NavFilePrev,
    ToggleDetail,
    ShowHelp,
    Split,
    Squash,
    Fixup,
    Reword,
    Drop,
    Move,
    Mergetool,
    OpenEditor,
    Update,
    Search,
    SearchNext,
    SearchPrev,
    Quit,
    Confirm,
    /// Ctrl+C: quit immediately, aborting any in-progress rebase first.
    ForceQuit,
    /// Ctrl+Z (Unix only): suspend the process with SIGTSTP.
    Suspend,
    SeparatorLeft,
    SeparatorRight,
    None,
}

/// Read the next terminal event, skipping key-release events.
///
/// On Windows, crossterm emits both a Press and a Release event for each
/// keystroke. Skipping Release events here keeps behaviour consistent with
/// Linux (which only emits Press) and prevents spurious state changes such as
/// error messages being cleared the instant they appear.
pub fn read_event() -> Result<Event> {
    loop {
        let ev = event::read()?;
        if let Event::Key(KeyEvent {
            kind: event::KeyEventKind::Release,
            ..
        }) = ev
        {
            continue;
        }
        return Ok(ev);
    }
}

impl AppMode {
    /// Parse a terminal event into a semantic key command for this mode.
    ///
    /// Most keys are mode-independent (arrows, Esc, Enter). Where a key has
    /// different meanings per mode (e.g. 'm' → Move in CommitList vs Mergetool
    /// in RebaseConflict), this method resolves the ambiguity.
    pub fn parse_key(&self, event: Event) -> KeyCommand {
        if let Event::Key(KeyEvent {
            code,
            modifiers,
            kind,
            ..
        }) = event
            && kind == event::KeyEventKind::Press
        {
            if modifiers.contains(KeyModifiers::CONTROL) {
                match code {
                    // Ctrl+C always force-quits regardless of mode.
                    KeyCode::Char('c') => return KeyCommand::ForceQuit,
                    // Ctrl-Z (Unix only): suspend the process with SIGTSTP.
                    #[cfg(unix)]
                    KeyCode::Char('z') => return KeyCommand::Suspend,
                    KeyCode::Left => return KeyCommand::SeparatorLeft,
                    KeyCode::Right => return KeyCommand::SeparatorRight,
                    // Ctrl-F / Ctrl-B: page scroll (less/vi convention).
                    KeyCode::Char('f') => return KeyCommand::PageDown,
                    KeyCode::Char('b') => return KeyCommand::PageUp,
                    // Ctrl-D / Ctrl-U: half-page scroll (vim convention).
                    KeyCode::Char('d') => return KeyCommand::HalfPageDown,
                    KeyCode::Char('u') => return KeyCommand::HalfPageUp,
                    // Ctrl-PageDown / Ctrl-PageUp: half-page scroll.
                    KeyCode::PageDown => return KeyCommand::HalfPageDown,
                    KeyCode::PageUp => return KeyCommand::HalfPageUp, // Ctrl-a / Ctrl-e: scroll to left/right edge (emacs convention).
                    KeyCode::Char('a') => return KeyCommand::ScrollToLeftEdge,
                    KeyCode::Char('e') => return KeyCommand::ScrollToRightEdge,
                    // Ctrl-Home / Ctrl-End: scroll to left/right edge.
                    KeyCode::Home => return KeyCommand::ScrollToLeftEdge,
                    KeyCode::End => return KeyCommand::ScrollToRightEdge,
                    _ => {}
                }
            }
            return match code {
                KeyCode::Up | KeyCode::Char('k') => KeyCommand::MoveUp,
                KeyCode::Down | KeyCode::Char('j') => KeyCommand::MoveDown,
                KeyCode::PageUp => KeyCommand::PageUp,
                KeyCode::PageDown => KeyCommand::PageDown,
                KeyCode::Home | KeyCode::Char('g') => KeyCommand::JumpToTop,
                KeyCode::End | KeyCode::Char('G') => KeyCommand::JumpToBottom,
                KeyCode::Left => KeyCommand::ScrollLeft,
                KeyCode::Right => KeyCommand::ScrollRight,
                KeyCode::Enter => KeyCommand::Confirm,
                KeyCode::Char('i') => KeyCommand::ToggleDetail,
                KeyCode::Char('h') => KeyCommand::ShowHelp,
                KeyCode::Char('p') => KeyCommand::Split,
                KeyCode::Char('s') => KeyCommand::Squash,
                KeyCode::Char('f') => match self {
                    AppMode::CommitDetail => KeyCommand::NavFileNext,
                    _ => KeyCommand::Fixup,
                },
                KeyCode::Char('F') => match self {
                    AppMode::CommitDetail => KeyCommand::NavFilePrev,
                    _ => KeyCommand::None,
                },
                KeyCode::Char('r') => KeyCommand::Reword,
                KeyCode::Char('d') => KeyCommand::Drop,
                KeyCode::Char('m') => match self {
                    AppMode::RebaseConflict(_) => KeyCommand::Mergetool,
                    _ => KeyCommand::Move,
                },
                KeyCode::Char('e') => match self {
                    AppMode::RebaseConflict(_) => KeyCommand::OpenEditor,
                    _ => KeyCommand::None,
                },
                KeyCode::Char('u') => KeyCommand::Update,
                KeyCode::Char('/') => match self {
                    AppMode::CommitDetail => KeyCommand::Search,
                    _ => KeyCommand::None,
                },
                KeyCode::Char('n') => match self {
                    AppMode::CommitDetail => KeyCommand::SearchNext,
                    _ => KeyCommand::None,
                },
                KeyCode::Char('N') => match self {
                    AppMode::CommitDetail => KeyCommand::SearchPrev,
                    _ => KeyCommand::None,
                },
                // Space / b: page scroll (less convention).
                KeyCode::Char(' ') => KeyCommand::PageDown,
                KeyCode::Char('b') => KeyCommand::PageUp,
                // 0 / $: scroll to left/right edge (vi/less convention).
                KeyCode::Char('0') => KeyCommand::ScrollToLeftEdge,
                KeyCode::Char('$') => KeyCommand::ScrollToRightEdge,
                KeyCode::Esc | KeyCode::Char('q') => KeyCommand::Quit,
                _ => KeyCommand::None,
            };
        }
        KeyCommand::None
    }
}
