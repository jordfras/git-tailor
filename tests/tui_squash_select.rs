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

// TUI snapshot tests for the squash target selection dialog.

mod common;

use git_tailor::{
    app::{AppAction, AppMode, AppState},
    event::KeyCommand,
    views,
};
use ratatui::{backend::TestBackend, Terminal};

fn make_app_in_squash_select(source_index: usize, selection_index: usize) -> AppState {
    let mut app = AppState::new();
    app.commits = vec![
        common::create_test_commit("aaa111bbb222", "Oldest commit on branch"),
        common::create_test_commit("ccc333ddd444", "Middle commit"),
        common::create_test_commit("eee555fff666", "Newest commit (HEAD)"),
    ];
    app.selection_index = selection_index;
    app.mode = AppMode::SquashSelect { source_index };
    app
}

#[test]
fn test_squash_dialog_renders() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    let mut app = make_app_in_squash_select(2, 2);

    terminal
        .draw(|frame| {
            views::commit_list::render(&mut app, frame);
            views::squash_select::render(&app, frame);
        })
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    insta::assert_debug_snapshot!(buffer);
}

#[test]
fn test_squash_dialog_source_different_from_selection() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    // Source is index 2, but user has navigated selection to index 0
    let mut app = make_app_in_squash_select(2, 0);

    terminal
        .draw(|frame| {
            views::commit_list::render(&mut app, frame);
            views::squash_select::render(&app, frame);
        })
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    insta::assert_debug_snapshot!(buffer);
}

#[test]
fn test_squash_confirm_returns_prepare_squash() {
    let mut app = make_app_in_squash_select(2, 0);

    let result = views::squash_select::handle_key(KeyCommand::Confirm, &mut app);
    match result {
        AppAction::PrepareSquash {
            source_oid,
            target_oid,
            ..
        } => {
            assert_eq!(source_oid, "eee555fff666");
            assert_eq!(target_oid, "aaa111bbb222");
        }
        other => panic!("Expected PrepareSquash, got {:?}", other),
    }
    assert_eq!(app.mode, AppMode::CommitList);
}

#[test]
fn test_squash_into_self_blocked() {
    let mut app = make_app_in_squash_select(1, 1);

    let result = views::squash_select::handle_key(KeyCommand::Confirm, &mut app);
    assert!(matches!(result, AppAction::Handled));
    assert!(app.status_message.is_some());
    assert!(app.status_is_error);
    // Mode stays in SquashSelect
    assert!(matches!(app.mode, AppMode::SquashSelect { .. }));
}

#[test]
fn test_squash_into_staged_blocked() {
    let mut app = AppState::new();
    app.commits = vec![
        common::create_test_commit("aaa111bbb222", "Real commit"),
        common::create_test_commit("staged", "staged"),
    ];
    app.selection_index = 1;
    app.mode = AppMode::SquashSelect { source_index: 0 };

    let result = views::squash_select::handle_key(KeyCommand::Confirm, &mut app);
    assert!(matches!(result, AppAction::Handled));
    assert!(app.status_is_error);
}

#[test]
fn test_squash_esc_cancels() {
    let mut app = make_app_in_squash_select(2, 0);

    let result = views::squash_select::handle_key(KeyCommand::Quit, &mut app);
    assert!(matches!(result, AppAction::Handled));
    assert_eq!(app.mode, AppMode::CommitList);
}

#[test]
fn test_squash_blocked_on_staged_row() {
    let mut app = AppState::new();
    app.commits = vec![
        common::create_test_commit("aaa111bbb222", "Real commit"),
        common::create_test_commit("staged", "staged"),
    ];
    app.selection_index = 1;
    app.mode = AppMode::CommitList;

    app.enter_squash_select();

    // Should still be in CommitList (blocked)
    assert_eq!(app.mode, AppMode::CommitList);
    assert!(app.status_is_error);
}

#[test]
fn test_squash_navigation_moves_selection() {
    let mut app = make_app_in_squash_select(2, 2);

    views::squash_select::handle_key(KeyCommand::MoveUp, &mut app);
    assert_eq!(app.selection_index, 1);

    views::squash_select::handle_key(KeyCommand::MoveUp, &mut app);
    assert_eq!(app.selection_index, 0);

    views::squash_select::handle_key(KeyCommand::MoveDown, &mut app);
    assert_eq!(app.selection_index, 1);
}
