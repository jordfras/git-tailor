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

use super::list_nav::{self, ListNav};
use crate::app::{AppAction, AppMode, AppState, KeyCommand};

/// Handle an action while in SquashSelect mode.
///
/// The user navigates the commit list to pick a squash target. The source
/// commit (from `source_index`) will be squashed *into* the chosen target.
/// Navigation is clamped so the cursor cannot move to commits later than
/// the source — squashing into a later commit is not supported.
pub fn handle_key(action: KeyCommand, app: &mut AppState) -> AppAction {
    let (source_index, squash_mode) = match app.mode {
        AppMode::SquashSelect {
            source_index,
            squash_mode,
        } => (source_index, squash_mode),
        _ => return AppAction::Handled,
    };

    let len = app.commits.len();
    let page_size = app.commit_list_visible_height;
    let reverse = app.reverse;
    let mut cursor = app.selection_index;

    match list_nav::handle_list_navigation(action, &mut cursor, len, page_size, reverse) {
        ListNav::Moved => {
            app.selection_index = cursor.min(source_index);
            AppAction::Handled
        }
        ListNav::Confirmed => {
            if app.selection_index == source_index {
                app.set_error_message("Cannot squash a commit into itself");
                return AppAction::Handled;
            }

            let Some(target) = app.selected_real_commit("squash into") else {
                return AppAction::Handled;
            };
            let target_oid = target.oid.expect_real_oid();
            let target_message = target.message.clone();

            let source = &app.commits[source_index];
            let result = AppAction::PrepareSquash {
                source_oid: source.oid.expect_real_oid(),
                target_oid,
                source_message: source.message.clone(),
                target_message,
                squash_mode,
            };

            app.mode = AppMode::CommitList;
            result
        }
        ListNav::Cancelled => {
            app.cancel_squash_select();
            AppAction::Handled
        }
        ListNav::Help => {
            app.toggle_help();
            AppAction::Handled
        }
        ListNav::Unhandled => AppAction::Handled,
    }
}
