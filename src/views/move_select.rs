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

// Move commit target selection — key handling only; rendering is done via
// the commit list (separator row injection + footer).

use crate::app::{AppAction, AppMode, AppState, KeyCommand};

/// Handle an action while in MoveSelect mode.
///
/// The user navigates an insertion cursor between commits. The cursor
/// (`insert_before`) represents the position where the source commit will
/// be placed. Arrow keys move the insertion point; Enter confirms; Esc
/// cancels.
pub fn handle_key(action: KeyCommand, app: &mut AppState) -> AppAction {
    let (source_index, insert_before) = match app.mode {
        AppMode::MoveSelect {
            source_index,
            insert_before,
        } => (source_index, insert_before),
        _ => return AppAction::Handled,
    };

    // Valid insertion positions: 0..=commits.len()-1, excluding source_index
    // (moving a commit to its own position is a no-op).
    let max_insert = app.commits.len().saturating_sub(1);

    match action {
        KeyCommand::MoveUp => {
            let mut next = if app.reverse {
                insert_before.saturating_add(1).min(max_insert)
            } else {
                insert_before.saturating_sub(1)
            };
            if next == source_index {
                next = if app.reverse {
                    next.saturating_add(1).min(max_insert)
                } else {
                    next.saturating_sub(1)
                };
            }
            app.mode = AppMode::MoveSelect {
                source_index,
                insert_before: next,
            };
            AppAction::Handled
        }
        KeyCommand::MoveDown => {
            let mut next = if app.reverse {
                insert_before.saturating_sub(1)
            } else {
                insert_before.saturating_add(1).min(max_insert)
            };
            if next == source_index {
                next = if app.reverse {
                    next.saturating_sub(1)
                } else {
                    next.saturating_add(1).min(max_insert)
                };
            }
            app.mode = AppMode::MoveSelect {
                source_index,
                insert_before: next,
            };
            AppAction::Handled
        }
        KeyCommand::Confirm => {
            if insert_before == source_index {
                app.set_error_message("Commit is already at this position");
                return AppAction::Handled;
            }

            let source = &app.commits[source_index];
            if source.oid == "staged" || source.oid == "unstaged" {
                app.set_error_message("Cannot move staged/unstaged changes");
                return AppAction::Handled;
            }

            let source_oid = source.oid.clone();

            app.mode = AppMode::CommitList;
            app.set_success_message(format!(
                "Move {} → position {} (not yet implemented)",
                &source_oid[..source_oid.len().min(8)],
                insert_before,
            ));
            AppAction::Handled
        }
        KeyCommand::ShowHelp => {
            app.toggle_help();
            AppAction::Handled
        }
        KeyCommand::Quit => {
            app.cancel_move_select();
            AppAction::Handled
        }
        _ => AppAction::Handled,
    }
}
