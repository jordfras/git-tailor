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
use crate::app::{AppAction, AppMode, KeyCommand};
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
///
/// `prev_mode` is the mode the user was in before opening help; it controls
/// which keybinding subset is displayed.
pub fn render(prev_mode: &AppMode, app: &mut crate::app::AppState, frame: &mut Frame) {
    match prev_mode {
        AppMode::CommitDetail => render_commit_detail_help(app, frame),
        _ => render_commit_list_help(app, frame),
    }
}

fn render_commit_list_help(app: &mut crate::app::AppState, frame: &mut Frame) {
    let (max_scroll, visible_height) = Dialog::new(DialogKind::Info)
        .section(" Navigation")
        .key_binding("   ↑/↓, j/k  ", "Move selection up/down")
        .key_binding("   PgUp/PgDn ", "Move one page up/down")
        .key_binding("   Space/b   ", "Move one page down/up")
        .key_binding("   Ctrl-F/B  ", "Move one page down/up")
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
        .key_binding("   Enter, i  ", "Open commit detail view")
        .key_binding("   h         ", "Show this help dialog")
        .key_binding("   u         ", "Update commit list from HEAD")
        .section(" Other")
        .key_binding("   Esc, q    ", "Close dialog / Quit application")
        .blank()
        .render(frame, "Help — Commit List", 48, app.dialog_scroll_offset);
    app.max_dialog_scroll = max_scroll;
    app.dialog_visible_height = visible_height;
}

fn render_commit_detail_help(app: &mut crate::app::AppState, frame: &mut Frame) {
    let (max_scroll, visible_height) = Dialog::new(DialogKind::Info)
        .section(" Navigation")
        .key_binding("   ↑/↓, j/k  ", "Scroll up/down")
        .key_binding("   PgUp/PgDn ", "Scroll one page up/down")
        .key_binding("   Space/b   ", "Scroll one page down/up")
        .key_binding("   Ctrl-F/B  ", "Scroll one page down/up")
        .key_binding("   ←/→       ", "Scroll diff left/right")
        .key_binding("   Ctrl ←/→  ", "Move separator bar left/right")
        .section(" Search")
        .key_binding("   /         ", "Search (regex)")
        .key_binding("   n         ", "Next search match")
        .key_binding("   N         ", "Previous search match")
        .key_binding("   Esc       ", "Dismiss search")
        .section(" Views")
        .key_binding("   Enter, i  ", "Return to commit list")
        .key_binding("   h         ", "Show this help dialog")
        .section(" Other")
        .key_binding("   Esc, q    ", "Return to commit list")
        .blank()
        .render(frame, "Help — Commit Detail", 48, app.dialog_scroll_offset);
    app.max_dialog_scroll = max_scroll;
    app.dialog_visible_height = visible_height;
}
