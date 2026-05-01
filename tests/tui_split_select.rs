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
    app::{AppMode, AppState},
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
        views::split_select::render(&app, frame);
    }));
}

#[test]
fn test_split_dialog_per_hunk_selected() {
    let mut harness = TuiTestHarness::typical();

    let mut app = make_app_in_split_select(1);

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::split_select::render(&app, frame);
    }));
}

#[test]
fn test_split_dialog_per_hunk_group_selected() {
    let mut harness = TuiTestHarness::typical();

    let mut app = make_app_in_split_select(2);

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::split_select::render(&app, frame);
    }));
}
