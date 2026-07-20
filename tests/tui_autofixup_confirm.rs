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

// TUI snapshot tests for the bulk autofixup confirmation dialog.

#[allow(dead_code)]
mod common;
use common::TuiTestHarness;

use git_tailor::{
    Oid,
    app::{AppMode, AppState, PendingAutofixup, SquashMode},
    autofixup::AutofixupPair,
    views,
};

fn pair(
    source_oid: &str,
    source_summary: &str,
    target_oid: &str,
    mode: SquashMode,
) -> AutofixupPair {
    AutofixupPair {
        source_oid: Oid::from(source_oid),
        target_oid: Oid::from(target_oid),
        source_summary: source_summary.to_string(),
        target_summary: String::new(),
        source_message: source_summary.to_string(),
        target_message: String::new(),
        mode,
    }
}

fn make_app_in_autofixup_confirm(pairs: Vec<AutofixupPair>) -> AppState {
    let mut app = AppState::new();
    app.commits = vec![
        common::create_test_commit("abc123def456", "Add parser"),
        common::create_test_commit("def456ghi789", "fixup! Add parser"),
    ];
    app.selection_index = 0;
    app.mode = AppMode::AutofixupConfirm(PendingAutofixup {
        pairs,
        head_oid: Oid::from("def456ghi789abcdef012"),
        reference_oid: Oid::from("000000000000abcdef012"),
    });
    app
}

#[test]
fn test_autofixup_confirm_dialog_single_pair() {
    let mut harness = TuiTestHarness::typical();

    let mut app = make_app_in_autofixup_confirm(vec![pair(
        "abc123def456",
        "fixup! Add parser",
        "def456ghi789",
        SquashMode::Fixup,
    )]);

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::autofixup::render_autofixup_confirm(&mut app, frame);
    }));
}

#[test]
fn test_autofixup_confirm_dialog_multiple_pairs_mixed_modes() {
    let mut harness = TuiTestHarness::typical();

    let mut app = make_app_in_autofixup_confirm(vec![
        pair(
            "111111111111",
            "fixup! Add parser",
            "abc123def456",
            SquashMode::Fixup,
        ),
        pair(
            "222222222222",
            "fixup! Add parser",
            "abc123def456",
            SquashMode::Fixup,
        ),
        pair(
            "333333333333",
            "squash! Add lexer",
            "444444444444",
            SquashMode::Squash,
        ),
    ]);

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::autofixup::render_autofixup_confirm(&mut app, frame);
    }));
}

#[test]
fn test_autofixup_confirm_dialog_long_summary() {
    let mut harness = TuiTestHarness::typical();

    let mut app = make_app_in_autofixup_confirm(vec![pair(
        "abc123def456",
        "fixup! Refactor the entire parser module to use trait-based dispatching",
        "def456ghi789",
        SquashMode::Fixup,
    )]);

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::autofixup::render_autofixup_confirm(&mut app, frame);
    }));
}

#[test]
fn test_autofixup_confirm_dialog_narrow_terminal() {
    let mut harness = TuiTestHarness::very_narrow();

    let mut app = make_app_in_autofixup_confirm(vec![pair(
        "abc123def456",
        "fixup! Add parser",
        "def456ghi789",
        SquashMode::Fixup,
    )]);

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::autofixup::render_autofixup_confirm(&mut app, frame);
    }));
}
