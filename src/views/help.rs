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

// Help dialog view showing keybindings

use super::dialog::{Dialog, DialogKind, handle_dialog_scroll};
use crate::app::{AppAction, KeyCommand};
use ratatui::Frame;

/// Handle an action while in Help mode.
pub fn handle_key(action: KeyCommand, app: &mut crate::app::AppState) -> AppAction {
    match action {
        KeyCommand::Quit | KeyCommand::ShowHelp => {
            app.toggle_help();
            AppAction::Handled
        }
        _ => {
            handle_dialog_scroll(action, app);
            AppAction::Handled
        }
    }
}

/// Render the help dialog as a centered overlay.
pub fn render(app: &mut crate::app::AppState, frame: &mut Frame) {
    let (max_scroll, visible_height) = Dialog::new(DialogKind::Info)
        .section(" Navigation")
        .key_binding("   ↑/↓, j/k  ", "Move selection up/down")
        .key_binding("   PgUp/PgDn ", "Move one page up/down")
        .key_binding("   ←/→       ", "Scroll fragmap left/right")
        .key_binding("   Ctrl ←/→  ", "Move separator bar left/right")
        .section(" Operations")
        .key_binding("   p         ", "Split commit (choose strategy)")
        .key_binding("   s         ", "Squash commit (pick target)")
        .key_binding("   f         ", "Fixup commit (pick target)")
        .key_binding("   r         ", "Reword commit message")
        .key_binding("   d         ", "Drop commit")
        .key_binding("   m         ", "Move commit (pick new position)")
        .section(" Views")
        .key_binding("   Enter, i  ", "Toggle commit detail view")
        .key_binding("   h         ", "Show this help dialog")
        .key_binding("   u         ", "Update commit list from HEAD")
        .section(" Search (commit detail)")
        .key_binding("   /         ", "Search (regex)")
        .key_binding("   n         ", "Next search match")
        .key_binding("   N         ", "Previous search match")
        .key_binding("   Esc       ", "Dismiss search")
        .section(" Other")
        .key_binding("   Esc, q    ", "Close dialog / Quit application")
        .blank()
        .render(frame, "Help - Keybindings", 48, app.dialog_scroll_offset);
    app.max_dialog_scroll = max_scroll;
    app.dialog_visible_height = visible_height;
}
