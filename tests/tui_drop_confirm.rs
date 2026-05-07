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

// TUI snapshot tests for the drop confirmation dialog.

#[allow(dead_code)]
mod common;
use common::TuiTestHarness;

use git_tailor::{
    Oid,
    app::{AppMode, AppState, PendingDrop},
    repo::ConflictState,
    views,
};

fn make_app_in_drop_confirm(commit_oid: &str, commit_summary: &str) -> AppState {
    let mut app = AppState::new();
    app.commits = vec![
        common::create_test_commit("abc123def456", "Refactor parser module"),
        common::create_test_commit("def456ghi789", "Add feature X"),
    ];
    app.selection_index = 0;
    app.mode = AppMode::DropConfirm(PendingDrop {
        commit_oid: Oid::from(commit_oid),
        commit_summary: commit_summary.to_string(),
        head_oid: Oid::from("def456ghi789abcdef012"),
    });
    app
}

#[test]
fn test_drop_confirm_dialog() {
    let mut harness = TuiTestHarness::typical();

    let mut app = make_app_in_drop_confirm("abc123def456", "Refactor parser module");

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::drop::render_drop_confirm(&mut app, frame);
    }));
}

#[test]
fn test_drop_confirm_dialog_long_summary() {
    let mut harness = TuiTestHarness::typical();

    let mut app = make_app_in_drop_confirm(
        "abc123def456",
        "Refactor the entire parser module to use trait-based dispatching for better extensibility",
    );

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::drop::render_drop_confirm(&mut app, frame);
    }));
}

#[test]
fn test_drop_confirm_dialog_narrow_terminal() {
    let mut harness = TuiTestHarness::very_narrow();

    let mut app = make_app_in_drop_confirm("abc123def456", "Add feature X");

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::drop::render_drop_confirm(&mut app, frame);
    }));
}

// ---------------------------------------------------------------------------
// RebaseConflict dialog
// ---------------------------------------------------------------------------

fn make_app_in_drop_conflict(conflicting_oid: &str, remaining: Vec<&str>) -> AppState {
    let mut app = AppState::new();
    app.commits = vec![
        common::create_test_commit("abc123def456", "Refactor parser module"),
        common::create_test_commit("def456ghi789", "Add feature X"),
    ];
    app.selection_index = 0;
    app.mode = AppMode::RebaseConflict(Box::new(ConflictState {
        operation_label: "Drop".to_string(),
        original_branch_oid: Oid::from("def456ghi789abcdef012"),
        new_tip_oid: Oid::from("aabbccddeeff00112233"),
        conflicting_commit_oid: Oid::from(conflicting_oid),
        remaining_oids: remaining.iter().copied().map(Oid::from).collect(),
        conflicting_files: vec![],
        still_unresolved: false,
        moved_commit_oid: None,
        squash_context: None,
    }));
    app
}

#[test]
fn test_drop_conflict_dialog_no_remaining() {
    let mut harness = TuiTestHarness::typical();

    let mut app = make_app_in_drop_conflict("abc123def456", vec![]);

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::conflict::render_conflict(&mut app, frame);
    }));
}

#[test]
fn test_drop_conflict_dialog_with_remaining() {
    let mut harness = TuiTestHarness::typical();

    let mut app = make_app_in_drop_conflict(
        "abc123def456",
        vec!["111111111111", "222222222222", "333333333333"],
    );

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::conflict::render_conflict(&mut app, frame);
    }));
}

#[test]
fn test_drop_conflict_dialog_narrow_terminal() {
    let mut harness = TuiTestHarness::very_narrow();

    let mut app = make_app_in_drop_conflict("abc123def456", vec!["111111111111"]);

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::conflict::render_conflict(&mut app, frame);
    }));
}

#[test]
fn test_drop_conflict_dialog_long_summary() {
    let mut harness = TuiTestHarness::typical();

    let mut app = AppState::new();
    app.commits = vec![
        common::create_test_commit(
            "abc123def456",
            "Refactor the entire parser module to use trait-based dispatching for better extensibility",
        ),
        common::create_test_commit("def456ghi789", "Add feature X"),
    ];
    app.selection_index = 0;
    app.mode = AppMode::RebaseConflict(Box::new(ConflictState {
        operation_label: "Drop".to_string(),
        original_branch_oid: Oid::from("def456ghi789abcdef012"),
        new_tip_oid: Oid::from("aabbccddeeff00112233"),
        conflicting_commit_oid: Oid::from("abc123def456"),
        remaining_oids: vec![Oid::from("111111111111"), Oid::from("222222222222")],
        conflicting_files: vec![],
        still_unresolved: false,
        moved_commit_oid: None,
        squash_context: None,
    }));

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::conflict::render_conflict(&mut app, frame);
    }));
}

#[test]
fn test_drop_conflict_dialog_with_files() {
    let mut harness = TuiTestHarness::typical();

    let mut app = AppState::new();
    app.commits = vec![
        common::create_test_commit("abc123def456", "Refactor parser module"),
        common::create_test_commit("def456ghi789", "Add feature X"),
    ];
    app.selection_index = 0;
    app.mode = AppMode::RebaseConflict(Box::new(ConflictState {
        operation_label: "Drop".to_string(),
        original_branch_oid: Oid::from("def456ghi789abcdef012"),
        new_tip_oid: Oid::from("aabbccddeeff00112233"),
        conflicting_commit_oid: Oid::from("abc123def456"),
        remaining_oids: vec![],
        conflicting_files: vec![
            "src/parser/mod.rs".to_string(),
            "src/parser/expr.rs".to_string(),
            "tests/integration.rs".to_string(),
        ],
        still_unresolved: false,
        moved_commit_oid: None,
        squash_context: None,
    }));

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::conflict::render_conflict(&mut app, frame);
    }));
}

#[test]
fn test_drop_conflict_dialog_still_unresolved_warning() {
    let mut harness = TuiTestHarness::typical();

    let mut app = AppState::new();
    app.commits = vec![
        common::create_test_commit("abc123def456", "Refactor parser module"),
        common::create_test_commit("def456ghi789", "Add feature X"),
    ];
    app.selection_index = 0;
    app.mode = AppMode::RebaseConflict(Box::new(ConflictState {
        operation_label: "Drop".to_string(),
        original_branch_oid: Oid::from("def456ghi789abcdef012"),
        new_tip_oid: Oid::from("aabbccddeeff00112233"),
        conflicting_commit_oid: Oid::from("abc123def456"),
        remaining_oids: vec![],
        conflicting_files: vec!["src/parser/mod.rs".to_string()],
        still_unresolved: true,
        moved_commit_oid: None,
        squash_context: None,
    }));

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::conflict::render_conflict(&mut app, frame);
    }));
}
