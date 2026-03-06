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
    fragmap::{FileSpan, FragMap, SpanCluster, TouchKind},
    views,
};

fn make_app_in_squash_select(source_index: usize, selection_index: usize) -> AppState {
    let mut app = AppState::new();
    app.commits = vec![
        common::create_test_commit("aaa111bbb222", "Oldest commit on branch"),
        common::create_test_commit("ccc333ddd444", "Middle commit"),
        common::create_test_commit("eee555fff666", "Newest commit (HEAD)"),
    ];
    app.selection_index = selection_index;
    app.mode = AppMode::SquashSelect {
        source_index,
        is_fixup: false,
    };
    app
}

use ratatui::{backend::TestBackend, Terminal};

#[test]
fn test_squash_footer_renders() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    let mut app = make_app_in_squash_select(2, 2);

    terminal
        .draw(|frame| {
            views::commit_list::render(&mut app, frame);
        })
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    insta::assert_debug_snapshot!(buffer);
}

#[test]
fn test_squash_footer_source_different_from_selection() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    // Source is index 2, but user has navigated selection to index 0
    let mut app = make_app_in_squash_select(2, 0);

    terminal
        .draw(|frame| {
            views::commit_list::render(&mut app, frame);
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
    app.mode = AppMode::SquashSelect {
        source_index: 0,
        is_fixup: false,
    };

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

#[test]
fn test_squash_clamps_to_source_index() {
    // source_index=1 (middle), start at 1 — cannot move to index 2 (later)
    let mut app = make_app_in_squash_select(1, 1);

    views::squash_select::handle_key(KeyCommand::MoveDown, &mut app);
    assert_eq!(app.selection_index, 1);

    views::squash_select::handle_key(KeyCommand::MoveUp, &mut app);
    assert_eq!(app.selection_index, 0);

    views::squash_select::handle_key(KeyCommand::MoveDown, &mut app);
    assert_eq!(app.selection_index, 1);

    views::squash_select::handle_key(KeyCommand::MoveDown, &mut app);
    assert_eq!(app.selection_index, 1);
}

#[test]
fn test_squash_clamps_in_reverse_mode() {
    let mut app = make_app_in_squash_select(1, 1);
    app.reverse = true;

    // In reverse, MoveUp maps to move_down (increase index) — clamped at source
    views::squash_select::handle_key(KeyCommand::MoveUp, &mut app);
    assert_eq!(app.selection_index, 1);

    // In reverse, MoveDown maps to move_up (decrease index) — allowed
    views::squash_select::handle_key(KeyCommand::MoveDown, &mut app);
    assert_eq!(app.selection_index, 0);
}

#[test]
fn test_squash_page_down_clamped() {
    let mut app = make_app_in_squash_select(1, 0);
    app.commit_list_visible_height = 10;

    // PageDown would jump past source_index — clamped to 1
    views::squash_select::handle_key(KeyCommand::PageDown, &mut app);
    assert_eq!(app.selection_index, 1);
}

fn simple_cluster(path: &str, start: u32, end: u32, oids: &[&str]) -> SpanCluster {
    SpanCluster {
        spans: vec![FileSpan {
            path: path.to_string(),
            start_line: start,
            end_line: end,
        }],
        commit_oids: oids.iter().map(|s| s.to_string()).collect(),
    }
}

/// Squash mode with fragmap: source is commit 2 (selected), commit 0 is
/// squashable (shares cluster cleanly), commit 1 is unrelated.
/// Source should have magenta bg, squashable candidate should be yellow,
/// unrelated should be dim.
#[test]
fn test_squash_candidate_coloring_with_fragmap() {
    let backend = TestBackend::new(80, 10);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    let mut app = AppState::new();
    app.commits = vec![
        common::create_test_commit("aaaa11112222", "Add config file"),
        common::create_test_commit("bbbb33334444", "Unrelated change"),
        common::create_test_commit("cccc55556666", "Fix config typo"),
    ];
    app.selection_index = 0; // navigated to commit 0 as target
    app.mode = AppMode::SquashSelect {
        source_index: 2,
        is_fixup: false,
    };

    // Cluster 0: commits 0 and 2 both touch it, commit 1 does not → squashable
    app.fragmap = Some(FragMap {
        commits: vec![
            "aaaa11112222".to_string(),
            "bbbb33334444".to_string(),
            "cccc55556666".to_string(),
        ],
        clusters: vec![
            simple_cluster("config.rs", 10, 20, &["aaaa11112222", "cccc55556666"]),
            simple_cluster("other.rs", 1, 5, &["bbbb33334444"]),
        ],
        matrix: vec![
            vec![TouchKind::Added, TouchKind::None],
            vec![TouchKind::None, TouchKind::Modified],
            vec![TouchKind::Modified, TouchKind::None],
        ],
    });

    terminal
        .draw(|frame| views::commit_list::render(&mut app, frame))
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    insta::assert_debug_snapshot!(buffer);
}

/// Squash mode with fragmap: source is commit 2, commit 0 would conflict
/// (commit 1 also touches the same cluster, creating a conflict).
#[test]
fn test_squash_candidate_coloring_conflicting() {
    let backend = TestBackend::new(80, 10);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    let mut app = AppState::new();
    app.commits = vec![
        common::create_test_commit("aaaa11112222", "Add parser"),
        common::create_test_commit("bbbb33334444", "Refactor parser"),
        common::create_test_commit("cccc55556666", "Fix parser bug"),
    ];
    app.selection_index = 0; // navigated to commit 0 as target
    app.mode = AppMode::SquashSelect {
        source_index: 2,
        is_fixup: false,
    };

    // All three commits touch cluster 0 → conflicting between 0 and 2
    app.fragmap = Some(FragMap {
        commits: vec![
            "aaaa11112222".to_string(),
            "bbbb33334444".to_string(),
            "cccc55556666".to_string(),
        ],
        clusters: vec![simple_cluster(
            "parser.rs",
            10,
            30,
            &["aaaa11112222", "bbbb33334444", "cccc55556666"],
        )],
        matrix: vec![
            vec![TouchKind::Added],
            vec![TouchKind::Modified],
            vec![TouchKind::Modified],
        ],
    });

    terminal
        .draw(|frame| views::commit_list::render(&mut app, frame))
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    insta::assert_debug_snapshot!(buffer);
}

/// Squash mode with source_index=1 (middle commit): commit at index 2 (newest)
/// should be dimmed with DarkGray to indicate it is an unreachable target.
#[test]
fn test_squash_dims_later_commits() {
    let backend = TestBackend::new(80, 10);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    let mut app = make_app_in_squash_select(1, 0);

    terminal
        .draw(|frame| views::commit_list::render(&mut app, frame))
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    insta::assert_debug_snapshot!(buffer);
}

#[test]
fn test_fixup_confirm_returns_prepare_fixup() {
    let mut app = AppState::new();
    app.commits = vec![
        common::create_test_commit("aaa111bbb222", "Oldest commit on branch"),
        common::create_test_commit("ccc333ddd444", "Middle commit"),
        common::create_test_commit("eee555fff666", "Newest commit (HEAD)"),
    ];
    app.selection_index = 0;
    app.mode = AppMode::SquashSelect {
        source_index: 2,
        is_fixup: true,
    };

    let result = views::squash_select::handle_key(KeyCommand::Confirm, &mut app);
    match result {
        AppAction::PrepareSquash {
            source_oid,
            target_oid,
            is_fixup,
            ..
        } => {
            assert_eq!(source_oid, "eee555fff666");
            assert_eq!(target_oid, "aaa111bbb222");
            assert!(is_fixup);
        }
        other => panic!("Expected PrepareSquash with is_fixup, got {:?}", other),
    }
    assert_eq!(app.mode, AppMode::CommitList);
}

#[test]
fn test_fixup_footer_renders() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    let mut app = AppState::new();
    app.commits = vec![
        common::create_test_commit("aaa111bbb222", "Oldest commit on branch"),
        common::create_test_commit("ccc333ddd444", "Middle commit"),
        common::create_test_commit("eee555fff666", "Newest commit (HEAD)"),
    ];
    app.selection_index = 0;
    app.mode = AppMode::SquashSelect {
        source_index: 2,
        is_fixup: true,
    };

    terminal
        .draw(|frame| {
            views::commit_list::render(&mut app, frame);
        })
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    insta::assert_debug_snapshot!(buffer);
}
