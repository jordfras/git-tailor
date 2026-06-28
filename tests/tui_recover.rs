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

// TUI snapshot tests for the startup crash-recovery dialog.

#[allow(dead_code)]
mod common;
use common::TuiTestHarness;

use git_tailor::{
    Oid,
    app::{AppMode, AppState},
    repo::ConflictState,
    views,
};

fn make_app_in_recover(operation_label: &str, conflicting_files: Vec<&str>) -> AppState {
    let mut app = AppState::new();
    app.commits = vec![
        common::create_test_commit("abc123def456", "Refactor parser module"),
        common::create_test_commit("def456ghi789", "Add feature X"),
    ];
    app.selection_index = 0;
    app.mode = AppMode::RecoverConfirm(Box::new(ConflictState {
        operation_label: operation_label.to_string(),
        original_branch_oid: Oid::from("def456ghi789abcdef012"),
        new_tip_oid: Oid::from("aabbccddeeff00112233"),
        conflicting_commit_oid: Oid::from("abc123def456"),
        conflicting_files: conflicting_files.into_iter().map(String::from).collect(),
        ..Default::default()
    }));
    app
}

#[test]
fn test_recover_dialog() {
    let mut harness = TuiTestHarness::typical();
    let mut app = make_app_in_recover("Drop", vec!["src/parser/mod.rs"]);

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::recover::render_recover(&mut app, frame);
    }));
}

/// A very narrow terminal with a long conflicting-file path must not panic when
/// truncating it (the dialog inner width can drop below the truncation margin).
#[test]
fn test_recover_dialog_tiny_terminal_does_not_panic() {
    for width in [2u16, 4, 6, 8] {
        let mut harness = TuiTestHarness::new(width, 12);
        let mut app = make_app_in_recover("Drop", vec!["src/very/long/path/overflows.rs"]);
        let _ = harness.render(|frame| views::recover::render_recover(&mut app, frame));
    }
}

/// Truncating a non-ASCII path must cut on a character boundary, not a byte
/// offset, so a narrow terminal does not panic mid-codepoint.
#[test]
fn test_recover_dialog_non_ascii_path_does_not_panic() {
    for width in [2u16, 4, 6, 8, 12, 20] {
        let mut harness = TuiTestHarness::new(width, 12);
        let mut app = make_app_in_recover("Drop", vec!["src/très/lÄngé/café_naïve_файл.rs"]);
        let _ = harness.render(|frame| views::recover::render_recover(&mut app, frame));
    }
}

#[test]
fn test_recover_dialog_multiple_files() {
    let mut harness = TuiTestHarness::typical();
    let mut app = make_app_in_recover(
        "Squash",
        vec![
            "src/parser/mod.rs",
            "src/parser/expr.rs",
            "tests/integration.rs",
        ],
    );

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::recover::render_recover(&mut app, frame);
    }));
}
