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

// TUI snapshot tests for the conflict dialog raised when the other working-tree
// row cannot be carried onto what the user resolved a fold to.

#[allow(dead_code)]
mod common;
use common::TuiTestHarness;

use git_tailor::{
    Oid,
    app::{AppMode, AppState},
    repo::{ConflictState, LiftedRow, Resume, WorktreeSource},
    views,
};

fn make_app_in_carry_conflict(source: WorktreeSource) -> AppState {
    let mut app = AppState::new();
    app.list.commits = vec![
        common::create_test_commit("abc123def456", "Refactor parser module"),
        common::create_test_commit("def456ghi789", "Add feature X"),
    ];
    app.list.selection_index = 0;
    app.mode = AppMode::RebaseConflict(Box::new(ConflictState {
        operation_label: "Fixup".to_string(),
        original_branch_oid: Oid::from("c".repeat(40)),
        new_tip_oid: Oid::from("abc123def456"),
        conflicting_commit_oid: Oid::from("abc123def456"),
        conflicting_files: vec!["src/parser.rs".to_string()],
        resume: Resume::CarryRow(LiftedRow {
            source,
            tip_before: Oid::from("a".repeat(40)),
            index_tree_before: Oid::from("d".repeat(40)),
            worktree_tree: Oid::from("e".repeat(40)),
            source_tree: Oid::from("f".repeat(40)),
            temp_oid: Oid::from("c".repeat(40)),
        }),
        ..Default::default()
    }));
    app
}

/// A fold from the staged row leaves the unstaged changes to carry, so that is
/// what the dialog names.
#[test]
fn carry_conflict_dialog_from_the_staged_row() {
    let mut harness = TuiTestHarness::typical();

    let mut app = make_app_in_carry_conflict(WorktreeSource::Staged);

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::conflict::render_conflict(&mut app, frame);
    }));
}

/// And the other way round: folding the unstaged row leaves the staged changes.
#[test]
fn carry_conflict_dialog_from_the_unstaged_row() {
    let mut harness = TuiTestHarness::typical();

    let mut app = make_app_in_carry_conflict(WorktreeSource::Unstaged);

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::conflict::render_conflict(&mut app, frame);
    }));
}
