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
            handle_dialog_scroll(action, &mut app.dialog);
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
    let (max_scroll, visible_height) = Dialog::new(DialogKind::Info, app.colors)
        .section(" Operations")
        .key_binding("   Space          ", "Choose an operation for this row")
        .key_binding("   p              ", "Split commit (choose strategy)")
        .key_binding("   s              ", "Squash commit (pick target)")
        .key_binding("   f              ", "Fixup commit (pick target)")
        .key_binding("   r              ", "Reword commit message")
        .key_binding("   d              ", "Drop commit")
        .key_binding("   m              ", "Move commit (pick new position)")
        .key_binding("   E              ", "Edit commit in a shell")
        .key_binding("   F              ", "Autofixup (fixup!/squash! commits)")
        .key_binding("   a              ", "Stage all tracked changes")
        .key_binding("   A              ", "Unstage all changes")
        .key_binding("   c              ", "Commit staged changes")
        .key_binding("   u              ", "Undo last operation")
        .key_binding("   Ctrl-r         ", "Redo last operation")
        .section(" Views")
        .key_binding("   Enter, i       ", "Open commit detail view")
        .key_binding("   h              ", "Show this help dialog")
        .key_binding("   R, F5          ", "Refresh commit list from HEAD")
        .section(" Navigation")
        .key_binding("   ↓/↑, k/j       ", "Move selection down/up")
        .key_binding("   PgDn/PgUp      ", "Move one page down/up")
        .key_binding("   b              ", "Move one page up")
        .key_binding("   Ctrl-f/b       ", "Move one page down/up")
        .key_binding("   Ctrl-d/u       ", "Move half page down/up")
        .key_binding("   Ctrl-PgDn/PgUp ", "Move half page down/up")
        .key_binding("   Ctrl-↓/↑       ", "Scroll list, keep selection")
        .key_binding("   g/G            ", "Jump to first/last commit")
        .key_binding("   Home/End       ", "Jump to first/last commit")
        .key_binding("   ←/→            ", "Scroll matrix left/right")
        .key_binding("   0/$            ", "Scroll matrix to left/right edge")
        .key_binding("   Ctrl-a/e       ", "Scroll matrix to left/right edge")
        .key_binding("   Ctrl-Home/End  ", "Scroll matrix to left/right edge")
        .key_binding("   Ctrl-←/→       ", "Move separator bar left/right")
        .section(" Other")
        .key_binding("   Esc, q         ", "Close dialog / Quit application")
        .blank()
        .render(frame, "Help — Commit List", 56, app.dialog.offset);
    app.dialog.set_bounds(max_scroll, visible_height);
}

fn render_commit_detail_help(app: &mut crate::app::AppState, frame: &mut Frame) {
    let (max_scroll, visible_height) = Dialog::new(DialogKind::Info, app.colors)
        .section(" Search")
        .key_binding("   /              ", "Search (regex)")
        .key_binding("   n              ", "Next search match")
        .key_binding("   N              ", "Previous search match")
        .key_binding("   Esc            ", "Dismiss search")
        .section(" Diff")
        .key_binding("   + / -          ", "Increase / decrease context lines")
        .section(" Views")
        .key_binding("   Enter, i       ", "Return to commit list")
        .key_binding("   h              ", "Show this help dialog")
        .section(" Navigation")
        .key_binding("   ↓/↑, k/j       ", "Scroll down/up")
        .key_binding("   ←/→            ", "Scroll view left/right")
        .key_binding("   PgDn/PgUp      ", "Scroll one page down/up")
        .key_binding("   Space/b        ", "Scroll one page down/up")
        .key_binding("   Ctrl-f/b       ", "Scroll one page down/up")
        .key_binding("   Ctrl-d/u       ", "Scroll half page down/up")
        .key_binding("   Ctrl-PgDn/PgUp ", "Scroll half page down/up")
        .key_binding("   g/G            ", "Scroll to top/bottom")
        .key_binding("   Home/End       ", "Scroll to top/bottom")
        .key_binding("   0/$            ", "Scroll view to left/right edge")
        .key_binding("   Ctrl-a/e       ", "Scroll view to left/right edge")
        .key_binding("   Ctrl-Home/End  ", "Scroll view to left/right edge")
        .key_binding("   Ctrl-←/→       ", "Move separator bar left/right")
        .key_binding("   f/F            ", "Jump to next/prev file in diff")
        .section(" Other")
        .key_binding("   Esc, q         ", "Return to commit list")
        .blank()
        .render(frame, "Help — Commit Detail", 56, app.dialog.offset);
    app.dialog.set_bounds(max_scroll, visible_height);
}
