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

// TUI snapshot tests for the auto-stash conflict resolution dialog.

#[allow(dead_code)]
mod common;
use common::TuiTestHarness;

use git_tailor::{
    app::{AppMode, AppState},
    repo::StashConflictState,
    views,
};

fn make_app_in_stash_conflict(files: Vec<&str>, still_unresolved: bool) -> AppState {
    let mut app = AppState::new();
    app.list.commits = vec![
        common::create_test_commit("abc123def456", "Refactor parser module"),
        common::create_test_commit("def456ghi789", "Add feature X"),
    ];
    app.list.selection_index = 0;
    app.mode = AppMode::StashConflict(Box::new(StashConflictState {
        operation_label: "Drop".to_string(),
        conflicting_files: files.iter().map(|s| s.to_string()).collect(),
        still_unresolved,
    }));
    app
}

#[test]
fn test_stash_conflict_dialog_with_files() {
    let mut harness = TuiTestHarness::typical();
    let mut app = make_app_in_stash_conflict(vec!["src/main.rs", "README.md"], false);
    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::stash_conflict::render_stash_conflict(&mut app, frame);
    }));
}

#[test]
fn test_stash_conflict_dialog_still_unresolved_warning() {
    let mut harness = TuiTestHarness::typical();
    let mut app = make_app_in_stash_conflict(vec!["src/main.rs"], true);
    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::stash_conflict::render_stash_conflict(&mut app, frame);
    }));
}

#[test]
fn test_stash_conflict_dialog_narrow_terminal() {
    let mut harness = TuiTestHarness::new(40, 20);
    let mut app = make_app_in_stash_conflict(vec!["a/very/long/path/to/some/file.rs"], false);
    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::stash_conflict::render_stash_conflict(&mut app, frame);
    }));
}
