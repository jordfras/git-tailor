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

mod common;

use git_tailor::{
    app::{AppMode, AppState},
    views,
};
use ratatui::{backend::TestBackend, Terminal};

fn make_app_in_split_select(strategy_index: usize) -> AppState {
    let mut app = AppState::new();
    app.commits = vec![
        common::create_test_commit("abc123def456", "Refactor parser module"),
        common::create_test_commit("def456ghi789", "Add feature X"),
    ];
    app.selection_index = 0;
    app.mode = AppMode::SplitSelect { strategy_index };
    app
}

#[test]
fn test_split_dialog_per_file_selected() {
    let backend = TestBackend::new(80, 20);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    let mut app = make_app_in_split_select(0);

    terminal
        .draw(|frame| {
            views::commit_list::render(&mut app, frame);
            views::split_select::render(&app, frame);
        })
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    insta::assert_debug_snapshot!(buffer);
}

#[test]
fn test_split_dialog_per_hunk_selected() {
    let backend = TestBackend::new(80, 20);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    let mut app = make_app_in_split_select(1);

    terminal
        .draw(|frame| {
            views::commit_list::render(&mut app, frame);
            views::split_select::render(&app, frame);
        })
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    insta::assert_debug_snapshot!(buffer);
}

#[test]
fn test_split_dialog_per_hunk_group_selected() {
    let backend = TestBackend::new(80, 20);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    let mut app = make_app_in_split_select(2);

    terminal
        .draw(|frame| {
            views::commit_list::render(&mut app, frame);
            views::split_select::render(&app, frame);
        })
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    insta::assert_debug_snapshot!(buffer);
}
