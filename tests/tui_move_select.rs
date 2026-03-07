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

// TUI snapshot + behavioral tests for the move commit selection mode.

mod common;

use git_tailor::{
    app::{AppAction, AppMode, AppState, KeyCommand},
    views,
};
use ratatui::{Terminal, backend::TestBackend};

fn make_app_in_move_select(source_index: usize, insert_before: usize) -> AppState {
    let mut app = AppState::new();
    app.commits = vec![
        common::create_test_commit("aaa111bbb222", "Oldest commit on branch"),
        common::create_test_commit("ccc333ddd444", "Middle commit"),
        common::create_test_commit("eee555fff666", "Newest commit (HEAD)"),
    ];
    app.selection_index = source_index;
    app.mode = AppMode::MoveSelect {
        source_index,
        insert_before,
    };
    app
}

// ── Rendering tests ──────────────────────────────────────────────────

#[test]
fn test_move_select_footer_renders() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    // Source is the newest commit (index 2), insertion cursor at 1
    let mut app = make_app_in_move_select(2, 1);

    terminal
        .draw(|frame| {
            views::commit_list::render(&mut app, frame);
        })
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    insta::assert_debug_snapshot!(buffer);
}

#[test]
fn test_move_select_separator_at_top() {
    let backend = TestBackend::new(80, 12);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    // Source is the newest commit (index 2), insertion at 0 (top)
    let mut app = make_app_in_move_select(2, 0);

    terminal
        .draw(|frame| {
            views::commit_list::render(&mut app, frame);
        })
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    insta::assert_debug_snapshot!(buffer);
}

#[test]
fn test_move_select_separator_at_middle() {
    let backend = TestBackend::new(80, 12);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    // Source is index 0 (oldest), insertion at 1 (middle)
    let mut app = make_app_in_move_select(0, 1);

    terminal
        .draw(|frame| {
            views::commit_list::render(&mut app, frame);
        })
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    insta::assert_debug_snapshot!(buffer);
}

#[test]
fn test_move_select_source_highlighted() {
    let backend = TestBackend::new(80, 12);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    // Source is middle commit (index 1), insertion at 0
    let mut app = make_app_in_move_select(1, 0);

    terminal
        .draw(|frame| {
            views::commit_list::render(&mut app, frame);
        })
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    insta::assert_debug_snapshot!(buffer);
}

#[test]
fn test_move_select_reversed() {
    let backend = TestBackend::new(80, 12);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    let mut app = make_app_in_move_select(2, 0);
    app.reverse = true;

    terminal
        .draw(|frame| {
            views::commit_list::render(&mut app, frame);
        })
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    insta::assert_debug_snapshot!(buffer);
}

// ── Key handling tests ───────────────────────────────────────────────

#[test]
fn test_move_navigation_skips_source() {
    // Source is index 1 (middle), start insertion at 0
    let mut app = make_app_in_move_select(1, 0);

    // MoveDown should skip source_index 1 and land on 2
    views::move_select::handle_key(KeyCommand::MoveDown, &mut app);
    assert_eq!(
        app.mode,
        AppMode::MoveSelect {
            source_index: 1,
            insert_before: 2,
        }
    );

    // MoveUp should skip source_index 1 and land on 0
    views::move_select::handle_key(KeyCommand::MoveUp, &mut app);
    assert_eq!(
        app.mode,
        AppMode::MoveSelect {
            source_index: 1,
            insert_before: 0,
        }
    );
}

#[test]
fn test_move_navigation_clamps_at_boundaries() {
    // Source is index 2 (last), start insertion at 0
    let mut app = make_app_in_move_select(2, 0);

    // MoveUp at 0 should stay at 0
    views::move_select::handle_key(KeyCommand::MoveUp, &mut app);
    assert_eq!(
        app.mode,
        AppMode::MoveSelect {
            source_index: 2,
            insert_before: 0,
        }
    );

    // MoveDown from 0 → 1
    views::move_select::handle_key(KeyCommand::MoveDown, &mut app);
    assert_eq!(
        app.mode,
        AppMode::MoveSelect {
            source_index: 2,
            insert_before: 1,
        }
    );
}

#[test]
fn test_move_navigation_reverse_inverts_direction() {
    let mut app = make_app_in_move_select(2, 0);
    app.reverse = true;

    // In reverse mode, MoveUp increases index
    views::move_select::handle_key(KeyCommand::MoveUp, &mut app);
    assert_eq!(
        app.mode,
        AppMode::MoveSelect {
            source_index: 2,
            insert_before: 1,
        }
    );

    // In reverse mode, MoveDown decreases index
    views::move_select::handle_key(KeyCommand::MoveDown, &mut app);
    assert_eq!(
        app.mode,
        AppMode::MoveSelect {
            source_index: 2,
            insert_before: 0,
        }
    );
}

#[test]
fn test_move_confirm_returns_execute_move() {
    let mut app = make_app_in_move_select(2, 0);
    app.reference_oid = "ref000".to_string();

    let result = views::move_select::handle_key(KeyCommand::Confirm, &mut app);
    match result {
        AppAction::ExecuteMove {
            source_oid,
            insert_after_oid,
        } => {
            assert_eq!(source_oid, "eee555fff666");
            assert_eq!(insert_after_oid, "ref000");
        }
        other => panic!("expected ExecuteMove, got {other:?}"),
    }
    assert_eq!(app.mode, AppMode::CommitList);
}

#[test]
fn test_move_confirm_at_same_position_shows_error() {
    // source_index == insert_before → no-op
    let mut app = make_app_in_move_select(1, 1);

    let result = views::move_select::handle_key(KeyCommand::Confirm, &mut app);
    assert!(matches!(result, AppAction::Handled));
    // Should remain in MoveSelect mode
    assert!(matches!(app.mode, AppMode::MoveSelect { .. }));
    assert!(app.status_is_error);
}

#[test]
fn test_move_esc_cancels() {
    let mut app = make_app_in_move_select(2, 0);

    let result = views::move_select::handle_key(KeyCommand::Quit, &mut app);
    assert!(matches!(result, AppAction::Handled));
    assert_eq!(app.mode, AppMode::CommitList);
}

#[test]
fn test_enter_move_select_blocks_on_synthetic() {
    let mut app = AppState::new();
    app.commits = vec![
        common::create_test_commit("aaa111bbb222", "Real commit"),
        common::create_test_commit("staged", "staged"),
    ];
    app.selection_index = 1;
    app.mode = AppMode::CommitList;

    app.enter_move_select();

    // Should still be in CommitList (blocked)
    assert_eq!(app.mode, AppMode::CommitList);
    assert!(app.status_is_error);
}

#[test]
fn test_enter_move_select_sets_correct_indices() {
    let mut app = AppState::new();
    app.commits = vec![
        common::create_test_commit("aaa111bbb222", "First"),
        common::create_test_commit("ccc333ddd444", "Second"),
        common::create_test_commit("eee555fff666", "Third"),
    ];
    app.selection_index = 2;
    app.mode = AppMode::CommitList;

    app.enter_move_select();

    assert_eq!(
        app.mode,
        AppMode::MoveSelect {
            source_index: 2,
            insert_before: 1,
        }
    );
}
