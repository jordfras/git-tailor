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

// Shared navigation helper for linear-list picker dialogs (squash, split, …).

use crate::app::KeyCommand;

/// Result of a single key event in a list-picker dialog.
pub enum ListNav {
    /// The cursor was moved; the caller should update any dependent state.
    Moved,
    /// The user confirmed the current selection (Enter).
    Confirmed,
    /// The user cancelled the dialog (Esc / q).
    Cancelled,
    /// The user requested the help overlay.
    Help,
    /// The key is not handled by list navigation.
    Unhandled,
}

/// Handle navigation for a simple linear-list picker.
///
/// Moves `cursor` within `0..len` and returns a [`ListNav`] variant
/// describing what happened. The caller is responsible for any additional
/// clamping (e.g. squash_select clamps the cursor to `source_index` after
/// a move).
///
/// - `visible_height` is the number of visible rows; one line of overlap is
///   kept when paging (same arithmetic as the commit list). The pickers that
///   always fit on screen pass their item count, so a page is the whole list.
/// - `reverse` swaps the visual direction of Up/Down to match the
///   `--reverse` display mode.
pub fn handle_list_navigation(
    action: KeyCommand,
    cursor: &mut usize,
    len: usize,
    visible_height: usize,
    reverse: bool,
) -> ListNav {
    let step_page = crate::app::scroll::page_size(visible_height);
    match action {
        KeyCommand::MoveUp => {
            if reverse {
                cursor_down(cursor, len, 1);
            } else {
                cursor_up(cursor, 1);
            }
            ListNav::Moved
        }
        KeyCommand::MoveDown => {
            if reverse {
                cursor_up(cursor, 1);
            } else {
                cursor_down(cursor, len, 1);
            }
            ListNav::Moved
        }
        KeyCommand::PageUp => {
            if reverse {
                cursor_down(cursor, len, step_page);
            } else {
                cursor_up(cursor, step_page);
            }
            ListNav::Moved
        }
        KeyCommand::PageDown => {
            if reverse {
                cursor_up(cursor, step_page);
            } else {
                cursor_down(cursor, len, step_page);
            }
            ListNav::Moved
        }
        KeyCommand::Confirm => ListNav::Confirmed,
        KeyCommand::Quit => ListNav::Cancelled,
        KeyCommand::ShowHelp => ListNav::Help,
        _ => ListNav::Unhandled,
    }
}

fn cursor_up(cursor: &mut usize, step: usize) {
    *cursor = cursor.saturating_sub(step);
}

fn cursor_down(cursor: &mut usize, len: usize, step: usize) {
    if len > 0 {
        *cursor = cursor.saturating_add(step).min(len - 1);
    }
}
