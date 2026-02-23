// TUI snapshot tests with TestBackend

use git_scissors::{app::AppState, views, CommitInfo};
use ratatui::{backend::TestBackend, Terminal};

fn create_test_commit(oid: &str, summary: &str) -> CommitInfo {
    CommitInfo {
        oid: oid.to_string(),
        summary: summary.to_string(),
        author: "Test Author <test@example.com>".to_string(),
        date: "2024-01-15 10:30:00".to_string(),
        parent_oids: vec!["parent123".to_string()],
    }
}

#[test]
fn test_commit_list_empty() {
    let backend = TestBackend::new(80, 10);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    let app = AppState::new();

    terminal
        .draw(|frame| {
            views::commit_list::render(&app, frame);
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
        create_test_commit("abc123def456", "Initial commit"),
        create_test_commit("def456ghi789", "Add feature X"),
        create_test_commit("ghi789jkl012", "Fix bug in parser"),
    ];
    app.selection_index = 0;

    terminal
        .draw(|frame| {
            views::commit_list::render(&app, frame);
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
        create_test_commit("abc123def456", "Initial commit"),
        create_test_commit("def456ghi789", "Add feature X"),
        create_test_commit("ghi789jkl012", "Fix bug in parser"),
    ];
    app.selection_index = 1;

    terminal
        .draw(|frame| {
            views::commit_list::render(&app, frame);
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
        create_test_commit(
            "abc123def456",
            "This is a very long commit summary that exceeds normal length",
        ),
        create_test_commit("def456ghi789", "Short"),
    ];
    app.selection_index = 0;

    terminal
        .draw(|frame| {
            views::commit_list::render(&app, frame);
        })
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    insta::assert_debug_snapshot!(buffer);
}
