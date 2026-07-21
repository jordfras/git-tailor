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

// TUI snapshot tests for the split-strategy selection dialog.

#[allow(dead_code)]
mod common;
use common::TuiTestHarness;

use git_tailor::{
    Oid,
    app::{AppAction, AppMode, AppState, KeyCommand, SplitStrategy},
    views,
};

fn make_app_in_split_select(strategy_index: usize) -> AppState {
    let mut app =
        common::app_state_from_commit_summaries(&["Refactor parser module", "Add feature X"]);
    app.selection_index = 0;
    app.mode = AppMode::SplitSelect { strategy_index };
    app
}

#[test]
fn test_split_dialog_per_file_selected() {
    let mut harness = TuiTestHarness::typical();

    let mut app = make_app_in_split_select(0);

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::split_select::render(&mut app, frame);
    }));
}

#[test]
fn test_split_dialog_out_file_selected() {
    let mut harness = TuiTestHarness::typical();

    let mut app = make_app_in_split_select(1);

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::split_select::render(&mut app, frame);
    }));
}

#[test]
fn test_split_dialog_per_hunk_selected() {
    let mut harness = TuiTestHarness::typical();

    let mut app = make_app_in_split_select(2);

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::split_select::render(&mut app, frame);
    }));
}

#[test]
fn test_split_dialog_per_hunk_group_selected() {
    let mut harness = TuiTestHarness::typical();

    let mut app = make_app_in_split_select(3);

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::split_select::render(&mut app, frame);
    }));
}

#[test]
fn test_split_dialog_out_hunks_selected() {
    let mut harness = TuiTestHarness::typical();

    let mut app = make_app_in_split_select(4);

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::split_select::render(&mut app, frame);
    }));
}

/// Confirming "Split out hunk(s)" opens the hunk picker — no repository call
/// happens yet, unlike every other strategy which mutates the branch right
/// away — mirroring how "Split out file" opens its own file picker first.
#[test]
fn test_confirm_out_hunks_returns_prepare_split_out_hunks() {
    let mut app = make_app_in_split_select(4);

    let result = views::split_select::handle_key(KeyCommand::Confirm, &mut app);

    match result {
        AppAction::PrepareSplitOutHunks {
            commit_oid,
            context_lines,
        } => {
            assert_eq!(commit_oid, Oid::from("111111111111"));
            assert_eq!(context_lines, git_tailor::repo::DEFAULT_CONTEXT_LINES);
        }
        other => panic!("Expected PrepareSplitOutHunks, got {:?}", other),
    }
    assert_eq!(app.mode, AppMode::CommitList);
}

/// Every other strategy still returns `PrepareSplit` and returns to
/// CommitList, unaffected by adding the "Split out hunk(s)" branch.
#[test]
fn test_confirm_per_file_still_returns_prepare_split() {
    let mut app = make_app_in_split_select(0);

    let result = views::split_select::handle_key(KeyCommand::Confirm, &mut app);

    match result {
        AppAction::PrepareSplit {
            strategy,
            commit_oid,
        } => {
            assert!(matches!(strategy, SplitStrategy::PerFile));
            assert_eq!(commit_oid, Oid::from("111111111111"));
        }
        other => panic!("Expected PrepareSplit, got {:?}", other),
    }
    assert_eq!(app.mode, AppMode::CommitList);
}

/// A long non-ASCII commit summary must truncate on a character boundary, not a
/// byte offset (which would panic mid-codepoint).
#[test]
fn test_split_dialog_long_non_ascii_summary_does_not_panic() {
    let mut app = common::app_state_from_commit_summaries(&[
        "Réfactor le café très naïve — обновление — 日本語のまとめ for the parser",
    ]);
    app.selection_index = 0;
    app.mode = AppMode::SplitSelect { strategy_index: 0 };
    let mut harness = TuiTestHarness::typical();
    let _ = harness.render(|frame| views::split_select::render(&mut app, frame));
}
