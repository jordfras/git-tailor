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
    ToggleHunkSelect,
    ToggleDetail,
    ShowHelp,
    OperationMenu,
    Split,
    Squash,
    Fixup,
    Autofixup,
    Reword,
    Drop,
    Move,
    StageAll,
    UnstageAll,
    CommitStaged,
    Undo,
    Redo,
    Mergetool,
    OpenEditor,
    Refresh,
    Search,
    SearchNext,
    SearchPrev,
    IncreaseContext,
    DecreaseContext,
    Quit,
    Confirm,
    ForceQuit,
    Suspend,
    SeparatorLeft,
    SeparatorRight,
    ScrollListUp,
    ScrollListDown,
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
    /// Keys map to a single command regardless of mode; the per-mode handlers
    /// decide whether that command does anything. Only keys whose *meaning*
    /// genuinely differs by mode are disambiguated here with a `match self`.
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
                    // Ctrl-Up / Ctrl-Down: scroll the commit list without moving
                    // the selection.
                    KeyCode::Up => return KeyCommand::ScrollListUp,
                    KeyCode::Down => return KeyCommand::ScrollListDown,
                    // Ctrl-F / Ctrl-B: page scroll (less/vi convention).
                    KeyCode::Char('f') => return KeyCommand::PageDown,
                    KeyCode::Char('b') => return KeyCommand::PageUp,
                    // Ctrl-D / Ctrl-U: half-page scroll (vim convention).
                    KeyCode::Char('d') => return KeyCommand::HalfPageDown,
                    KeyCode::Char('u') => return KeyCommand::HalfPageUp,
                    // Ctrl-R: redo (vim convention).
                    KeyCode::Char('r') => return KeyCommand::Redo,
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
                    _ => KeyCommand::Autofixup,
                },
                KeyCode::Char('r') => KeyCommand::Reword,
                KeyCode::Char('d') => KeyCommand::Drop,
                KeyCode::Char('m') => match self {
                    AppMode::RebaseConflict(_) | AppMode::StashConflict(_) => KeyCommand::Mergetool,
                    _ => KeyCommand::Move,
                },
                KeyCode::Char('e') => KeyCommand::OpenEditor,
                // a / A / c: stage / unstage all changes, commit staged.
                KeyCode::Char('a') => KeyCommand::StageAll,
                KeyCode::Char('A') => KeyCommand::UnstageAll,
                KeyCode::Char('c') => KeyCommand::CommitStaged,
                // u: undo (vim convention).
                KeyCode::Char('u') => KeyCommand::Undo,
                KeyCode::Char('/') => KeyCommand::Search,
                KeyCode::Char('n') => KeyCommand::SearchNext,
                KeyCode::Char('N') => KeyCommand::SearchPrev,
                KeyCode::Char('+') => KeyCommand::IncreaseContext,
                KeyCode::Char('-') => KeyCommand::DecreaseContext,
                // Space: open the operation picker in the commit list;
                // toggle-select a hunk in the split-out-hunks picker; elsewhere
                // (detail/pager view, other dialogs) it keeps the less-style
                // page-down.
                KeyCode::Char(' ') => match self {
                    AppMode::CommitList => KeyCommand::OperationMenu,
                    AppMode::SplitHunksSelect { .. } => KeyCommand::ToggleHunkSelect,
                    _ => KeyCommand::PageDown,
                },
                KeyCode::Char('b') => KeyCommand::PageUp,
                // 0 / $: scroll to left/right edge (vi/less convention).
                KeyCode::Char('0') => KeyCommand::ScrollToLeftEdge,
                KeyCode::Char('$') => KeyCommand::ScrollToRightEdge,
                // R / F5: refresh the commit list from HEAD.
                KeyCode::Char('R') | KeyCode::F(5) => KeyCommand::Refresh,
                KeyCode::Esc | KeyCode::Char('q') => KeyCommand::Quit,
                _ => KeyCommand::None,
            };
        }
        KeyCommand::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    #[test]
    fn space_opens_operation_menu_in_commit_list() {
        assert_eq!(
            AppMode::CommitList.parse_key(press(KeyCode::Char(' '))),
            KeyCommand::OperationMenu
        );
    }

    #[test]
    fn space_pages_down_outside_the_commit_list() {
        // In the pager-style detail view Space keeps its less-style page-down.
        assert_eq!(
            AppMode::CommitDetail.parse_key(press(KeyCode::Char(' '))),
            KeyCommand::PageDown
        );
    }

    #[test]
    fn space_toggles_hunk_select_in_split_hunks_select() {
        let mode = AppMode::SplitHunksSelect {
            commit_oid: crate::Oid::from("0".repeat(40).as_str()),
            hunks: Vec::new(),
            hunk_index: 0,
            selected: std::collections::HashSet::new(),
            context_lines: 3,
        };
        assert_eq!(
            mode.parse_key(press(KeyCode::Char(' '))),
            KeyCommand::ToggleHunkSelect
        );
    }

    #[test]
    fn stage_keys_map_unconditionally() {
        // `a`/`A`/`c` map the same in every mode (like split/drop/reword); which
        // rows they apply to is enforced by the handlers, not the keymap.
        for mode in [
            AppMode::CommitList,
            AppMode::CommitDetail,
            AppMode::OperationSelect {
                operation: crate::app::Operation::Stage,
            },
        ] {
            assert_eq!(
                mode.parse_key(press(KeyCode::Char('a'))),
                KeyCommand::StageAll
            );
            assert_eq!(
                mode.parse_key(press(KeyCode::Char('A'))),
                KeyCommand::UnstageAll
            );
            assert_eq!(
                mode.parse_key(press(KeyCode::Char('c'))),
                KeyCommand::CommitStaged
            );
        }
    }

    #[test]
    fn plus_minus_map_to_context_commands() {
        // `+` / `-` map unconditionally; only the detail-view handler acts on
        // them (they are a no-op elsewhere).
        for mode in [AppMode::CommitDetail, AppMode::CommitList] {
            assert_eq!(
                mode.parse_key(press(KeyCode::Char('+'))),
                KeyCommand::IncreaseContext
            );
            assert_eq!(
                mode.parse_key(press(KeyCode::Char('-'))),
                KeyCommand::DecreaseContext
            );
        }
    }
}
