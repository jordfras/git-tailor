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

// TUI snapshot tests for the help dialog.

#[allow(dead_code)]
mod common;
use common::TuiTestHarness;

use git_tailor::{
    app::{AppMode, AppState},
    views,
};

fn make_app_in_help() -> AppState {
    let mut app =
        common::app_state_from_commit_summaries(&["Refactor parser module", "Add feature X"]);
    app.selection_index = 0;
    app.mode = AppMode::Help(Box::new(AppMode::CommitList));
    app
}

/// Full-size terminal — all keybindings visible without scrolling.
#[test]
fn test_help_dialog_full_size() {
    let mut harness = TuiTestHarness::typical();
    let mut app = make_app_in_help();

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::help::render(&mut app, frame);
    }));
}

/// Short terminal — content taller than the dialog area, scrollbar visible.
#[test]
fn test_help_dialog_short_terminal_scrollbar() {
    let mut harness = TuiTestHarness::short();
    let mut app = make_app_in_help();

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::help::render(&mut app, frame);
    }));
}

/// Short terminal, scrolled to middle — scrollbar thumb moves down.
#[test]
fn test_help_dialog_scrolled() {
    let mut harness = TuiTestHarness::short();
    let mut app = make_app_in_help();
    app.dialog_scroll_offset = 5;

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::help::render(&mut app, frame);
    }));
}
