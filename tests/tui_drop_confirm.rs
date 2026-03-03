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

mod common;

use git_tailor::{
    app::{AppMode, AppState, PendingDrop},
    views,
};
use ratatui::{backend::TestBackend, Terminal};

fn make_app_in_drop_confirm(commit_oid: &str, commit_summary: &str) -> AppState {
    let mut app = AppState::new();
    app.commits = vec![
        common::create_test_commit("abc123def456", "Refactor parser module"),
        common::create_test_commit("def456ghi789", "Add feature X"),
    ];
    app.selection_index = 0;
    app.mode = AppMode::DropConfirm(PendingDrop {
        commit_oid: commit_oid.to_string(),
        commit_summary: commit_summary.to_string(),
        head_oid: "def456ghi789abcdef012".to_string(),
    });
    app
}

#[test]
fn test_drop_confirm_dialog() {
    let backend = TestBackend::new(80, 20);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    let mut app = make_app_in_drop_confirm("abc123def456", "Refactor parser module");

    terminal
        .draw(|frame| {
            views::commit_list::render(&mut app, frame);
            views::split_select::render_drop_confirm(&app, frame);
        })
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    insta::assert_debug_snapshot!(buffer);
}

#[test]
fn test_drop_confirm_dialog_long_summary() {
    let backend = TestBackend::new(80, 20);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    let mut app = make_app_in_drop_confirm(
        "abc123def456",
        "Refactor the entire parser module to use trait-based dispatching for better extensibility",
    );

    terminal
        .draw(|frame| {
            views::commit_list::render(&mut app, frame);
            views::split_select::render_drop_confirm(&app, frame);
        })
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    insta::assert_debug_snapshot!(buffer);
}

#[test]
fn test_drop_confirm_dialog_narrow_terminal() {
    let backend = TestBackend::new(40, 15);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    let mut app = make_app_in_drop_confirm("abc123def456", "Add feature X");

    terminal
        .draw(|frame| {
            views::commit_list::render(&mut app, frame);
            views::split_select::render_drop_confirm(&app, frame);
        })
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    insta::assert_debug_snapshot!(buffer);
}
