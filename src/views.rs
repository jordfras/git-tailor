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

// TUI views for rendering different screens

pub mod autofixup;
pub mod commit_detail;
pub mod commit_list;
pub mod conflict;
pub mod dialog;
pub mod drop;
pub mod help;
pub mod hunk_groups;
pub mod list_nav;
pub mod loading;
pub mod main_view;
pub mod move_select;
pub mod operation_select;
pub mod palette;
pub mod recover;
pub mod search_bar;
pub mod search_match;
pub mod split_files_select;
pub mod split_hunks_select;
pub mod split_select;
pub mod squash_select;
pub mod stash_conflict;
pub mod theme;
pub mod two_pane_picker;

use crate::app::{AppAction, AppMode, AppState, KeyCommand};
use ratatui::Frame;

/// Render a mode, recursively drawing its background first for overlay modes.
pub fn render(mode: &AppMode, app: &mut AppState, frame: &mut Frame) {
    if let Some(bg) = mode.background() {
        render(&bg, app, frame);
    }

    match mode {
        AppMode::Loading { .. } => loading::render(app, frame),
        AppMode::CommitList => commit_list::render(app, frame),
        AppMode::CommitDetail => main_view::render(app, frame),
        AppMode::OperationSelect { .. } => operation_select::render(app, frame),
        AppMode::SplitSelect { .. } => split_select::render(app, frame),
        AppMode::SplitFilesSelect { .. } => split_files_select::render(app, frame),
        AppMode::SplitHunksSelect { .. } => split_hunks_select::render(app, frame),
        AppMode::SplitConfirm(_) => split_select::render_split_confirm(app, frame),
        AppMode::DropConfirm(_) => drop::render_drop_confirm(app, frame),
        AppMode::AutofixupConfirm(_) => autofixup::render_autofixup_confirm(app, frame),
        AppMode::RebaseConflict(_) => conflict::render_conflict(app, frame),
        AppMode::StashConflict(_) => stash_conflict::render_stash_conflict(app, frame),
        AppMode::RecoverConfirm(_) => recover::render_recover(app, frame),
        AppMode::SquashSelect { .. } => commit_list::render(app, frame),
        AppMode::MoveSelect { .. } => commit_list::render(app, frame),
        AppMode::Help(prev) => help::render(prev, app, frame),
    }
}

/// Hand `action` to the view the app is showing, which turns it into what the
/// app should do.
pub fn handle_key(action: KeyCommand, app: &mut AppState) -> AppAction {
    match app.mode {
        AppMode::Loading { .. } => AppAction::Handled,
        AppMode::CommitList => commit_list::handle_key(action, app),
        AppMode::CommitDetail => commit_detail::handle_key(action, app),
        AppMode::OperationSelect { .. } => operation_select::handle_key(action, app),
        AppMode::SplitSelect { .. } => split_select::handle_key(action, app),
        AppMode::SplitFilesSelect { .. } => split_files_select::handle_key(action, app),
        AppMode::SplitHunksSelect { .. } => split_hunks_select::handle_key(action, app),
        AppMode::SplitConfirm(_) => split_select::handle_confirm_key(action, app),
        AppMode::DropConfirm(_) => drop::handle_confirm_key(action, app),
        AppMode::AutofixupConfirm(_) => autofixup::handle_confirm_key(action, app),
        AppMode::RebaseConflict(_) => conflict::handle_conflict_key(action, app),
        AppMode::StashConflict(_) => stash_conflict::handle_stash_conflict_key(action, app),
        AppMode::RecoverConfirm(_) => recover::handle_recover_key(action, app),
        AppMode::SquashSelect { .. } => squash_select::handle_key(action, app),
        AppMode::MoveSelect { .. } => move_select::handle_key(action, app),
        AppMode::Help(_) => help::handle_key(action, app),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The same key means different things to different views: `q` closes help,
    /// and quits from the commit list.
    #[test]
    fn a_key_goes_to_the_view_on_screen() {
        let mut app = AppState::new();
        app.mode = AppMode::Help(Box::new(AppMode::CommitList));

        assert!(matches!(
            handle_key(KeyCommand::Quit, &mut app),
            AppAction::Handled
        ));
        assert!(matches!(app.mode, AppMode::CommitList));
        assert!(matches!(
            handle_key(KeyCommand::Quit, &mut app),
            AppAction::Quit
        ));
    }
}
