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

// TUI snapshot tests for the commit list view.

mod common;

use git_tailor::{app::AppState, views};
use ratatui::{backend::TestBackend, Terminal};

#[test]
fn test_commit_list_empty() {
    let backend = TestBackend::new(80, 10);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    let mut app = AppState::new();

    terminal
        .draw(|frame| {
            views::commit_list::render(&mut app, frame);
        })
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    insta::assert_debug_snapshot!(buffer);
}

#[test]
fn test_commit_list_with_commits() {
    let backend = TestBackend::new(80, 15);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    let mut app = AppState::new();
    app.commits = vec![
        common::create_test_commit("abc123def456", "Initial commit"),
        common::create_test_commit("def456ghi789", "Add feature X"),
        common::create_test_commit("ghi789jkl012", "Fix bug in parser"),
    ];
    app.selection_index = 0;

    terminal
        .draw(|frame| {
            views::commit_list::render(&mut app, frame);
        })
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    insta::assert_debug_snapshot!(buffer);
}

#[test]
fn test_commit_list_with_selection() {
    let backend = TestBackend::new(80, 15);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    let mut app = AppState::new();
    app.commits = vec![
        common::create_test_commit("abc123def456", "Initial commit"),
        common::create_test_commit("def456ghi789", "Add feature X"),
        common::create_test_commit("ghi789jkl012", "Fix bug in parser"),
    ];
    app.selection_index = 1;

    terminal
        .draw(|frame| {
            views::commit_list::render(&mut app, frame);
        })
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    insta::assert_debug_snapshot!(buffer);
}

#[test]
fn test_commit_list_long_summary() {
    let backend = TestBackend::new(80, 12);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    let mut app = AppState::new();
    app.commits = vec![
        common::create_test_commit(
            "abc123def456",
            "This is a very long commit summary that exceeds normal length",
        ),
        common::create_test_commit("def456ghi789", "Short"),
    ];
    app.selection_index = 0;

    terminal
        .draw(|frame| {
            views::commit_list::render(&mut app, frame);
        })
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    insta::assert_debug_snapshot!(buffer);
}

#[test]
fn test_commit_list_scrolled_to_top() {
    // Terminal height 8: borders(2) + header(1) = 3 overhead, so 5 rows visible.
    // 10 commits total → scrollbar should appear.
    let backend = TestBackend::new(60, 8);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    let mut app = AppState::new();
    app.commits = (0..10)
        .map(|i| {
            common::create_test_commit(&format!("{:012x}", i), &format!("Commit number {}", i))
        })
        .collect();
    app.selection_index = 0;

    terminal
        .draw(|frame| {
            views::commit_list::render(&mut app, frame);
        })
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    insta::assert_debug_snapshot!(buffer);
}

#[test]
fn test_commit_list_scrolled_to_bottom() {
    // Same setup as above, but selection at last commit.
    let backend = TestBackend::new(60, 8);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    let mut app = AppState::new();
    app.commits = (0..10)
        .map(|i| {
            common::create_test_commit(&format!("{:012x}", i), &format!("Commit number {}", i))
        })
        .collect();
    app.selection_index = 9;

    terminal
        .draw(|frame| {
            views::commit_list::render(&mut app, frame);
        })
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    insta::assert_debug_snapshot!(buffer);
}

#[test]
fn test_commit_list_reversed_with_commits() {
    let backend = TestBackend::new(80, 15);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    let mut app = AppState::new();
    app.commits = vec![
        common::create_test_commit("abc123def456", "Initial commit"),
        common::create_test_commit("def456ghi789", "Add feature X"),
        common::create_test_commit("ghi789jkl012", "Fix bug in parser"),
    ];
    app.selection_index = 2; // HEAD
    app.reverse = true;

    terminal
        .draw(|frame| {
            views::commit_list::render(&mut app, frame);
        })
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    insta::assert_debug_snapshot!(buffer);
}

#[test]
fn test_commit_list_reversed_scrolled() {
    // 10 commits, reverse mode with HEAD selected (index 9) → visual index 0 (top).
    let backend = TestBackend::new(60, 8);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    let mut app = AppState::new();
    app.commits = (0..10)
        .map(|i| {
            common::create_test_commit(&format!("{:012x}", i), &format!("Commit number {}", i))
        })
        .collect();
    app.selection_index = 9; // HEAD
    app.reverse = true;

    terminal
        .draw(|frame| {
            views::commit_list::render(&mut app, frame);
        })
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    insta::assert_debug_snapshot!(buffer);
}