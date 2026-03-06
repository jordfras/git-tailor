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

// Squash target selection — key handling only; rendering is done via the
// commit list footer (see `render_footer` in commit_list.rs).

use crate::app::{AppAction, AppMode, AppState};
use crate::event::KeyCommand;

/// Handle an action while in SquashSelect mode.
///
/// The user navigates the commit list to pick a squash target. The source
/// commit (from `source_index`) will be squashed *into* the chosen target.
/// Navigation is clamped so the cursor cannot move to commits later than
/// the source — squashing into a later commit is not supported.
pub fn handle_key(action: KeyCommand, app: &mut AppState) -> AppAction {
    let (source_index, is_fixup) = match app.mode {
        AppMode::SquashSelect {
            source_index,
            is_fixup,
        } => (source_index, is_fixup),
        _ => return AppAction::Handled,
    };

    match action {
        KeyCommand::MoveUp => {
            if app.reverse {
                app.move_down();
            } else {
                app.move_up();
            }
            app.selection_index = app.selection_index.min(source_index);
            AppAction::Handled
        }
        KeyCommand::MoveDown => {
            if app.reverse {
                app.move_up();
            } else {
                app.move_down();
            }
            app.selection_index = app.selection_index.min(source_index);
            AppAction::Handled
        }
        KeyCommand::PageUp => {
            let h = app.commit_list_visible_height;
            if app.reverse {
                app.page_down(h);
            } else {
                app.page_up(h);
            }
            app.selection_index = app.selection_index.min(source_index);
            AppAction::Handled
        }
        KeyCommand::PageDown => {
            let h = app.commit_list_visible_height;
            if app.reverse {
                app.page_up(h);
            } else {
                app.page_down(h);
            }
            app.selection_index = app.selection_index.min(source_index);
            AppAction::Handled
        }
        KeyCommand::Confirm => {
            let target_index = app.selection_index;

            // Cannot squash onto itself
            if target_index == source_index {
                app.set_error_message("Cannot squash a commit into itself");
                return AppAction::Handled;
            }

            // Cannot squash onto staged/unstaged
            let target = &app.commits[target_index];
            if target.oid == "staged" || target.oid == "unstaged" {
                app.set_error_message("Cannot squash into staged/unstaged changes");
                return AppAction::Handled;
            }

            let source = &app.commits[source_index];
            let result = AppAction::PrepareSquash {
                source_oid: source.oid.clone(),
                target_oid: target.oid.clone(),
                source_message: source.message.clone(),
                target_message: target.message.clone(),
                is_fixup,
            };

            app.mode = AppMode::CommitList;
            result
        }
        KeyCommand::ShowHelp => {
            app.toggle_help();
            AppAction::Handled
        }
        KeyCommand::Quit => {
            app.cancel_squash_select();
            AppAction::Handled
        }
        _ => AppAction::Handled,
    }
}
