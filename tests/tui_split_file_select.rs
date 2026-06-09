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

// TUI snapshot tests for the "split out file" file picker dialog.

#[allow(dead_code)]
mod common;
use common::TuiTestHarness;

use git_tailor::{
    Oid,
    app::{AppMode, AppState},
    views,
};

fn make_app_in_file_select(file_index: usize) -> AppState {
    let mut app =
        common::app_state_from_commit_summaries(&["Refactor parser module", "Add feature X"]);
    app.selection_index = 0;
    app.mode = AppMode::SplitFileSelect {
        commit_oid: Oid::from("0".repeat(40).as_str()),
        files: vec![
            "src/parser.rs".to_string(),
            "src/lexer.rs".to_string(),
            "tests/parser.rs".to_string(),
        ],
        file_index,
    };
    app
}

#[test]
fn test_split_file_select_first_highlighted() {
    let mut harness = TuiTestHarness::typical();
    let mut app = make_app_in_file_select(0);

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::split_file_select::render(&mut app, frame);
    }));
}

#[test]
fn test_split_file_select_second_highlighted() {
    let mut harness = TuiTestHarness::typical();
    let mut app = make_app_in_file_select(1);

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::split_file_select::render(&mut app, frame);
    }));
}

#[test]
fn test_split_file_select_long_path_elided() {
    let mut harness = TuiTestHarness::typical();
    let mut app =
        common::app_state_from_commit_summaries(&["Refactor parser module", "Add feature X"]);
    app.selection_index = 0;
    app.mode = AppMode::SplitFileSelect {
        commit_oid: Oid::from("0".repeat(40).as_str()),
        files: vec![
            "src/views/git2_impl/reads/extract_files_and_hunks.rs".to_string(),
            "Cargo.toml".to_string(),
        ],
        file_index: 0,
    };

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::split_file_select::render(&mut app, frame);
    }));
}
