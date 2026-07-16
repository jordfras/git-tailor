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

#[allow(dead_code)]
mod common;
use common::TuiTestHarness;

use git_tailor::{app::AppState, views};

#[test]
fn test_commit_list_empty() {
    let mut harness = TuiTestHarness::short();

    let mut app = AppState::new();

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
    }));
}

#[test]
fn test_commit_list_with_commits() {
    let mut harness = TuiTestHarness::typical();

    let mut app = AppState::new();
    app.commits = vec![
        common::create_test_commit("abc123def456", "Initial commit"),
        common::create_test_commit("def456ghi789", "Add feature X"),
        common::create_test_commit("ghi789jkl012", "Fix bug in parser"),
    ];
    app.selection_index = 0;

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
    }));
}

#[test]
fn test_commit_list_with_selection() {
    let mut harness = TuiTestHarness::typical();

    let mut app = AppState::new();
    app.commits = vec![
        common::create_test_commit("abc123def456", "Initial commit"),
        common::create_test_commit("def456ghi789", "Add feature X"),
        common::create_test_commit("ghi789jkl012", "Fix bug in parser"),
    ];
    app.selection_index = 1;

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
    }));
}

#[test]
fn campbell_palette_resolves_header_to_rgb() {
    use git_tailor::views::palette::Colors;
    use ratatui::style::Color;

    let mut harness = TuiTestHarness::typical();

    let mut app = AppState::new();
    app.commits = vec![common::create_test_commit("abc123def456", "Initial commit")];
    app.selection_index = 0;

    // With --colors terminal the header keeps the ANSI slots (White on Green).
    app.colors = Colors::Terminal;
    let terminal_buf = harness.render(|frame| views::commit_list::render(&mut app, frame));
    assert_eq!(terminal_buf.cell((0, 0)).unwrap().bg, Color::Green);
    assert_eq!(terminal_buf.cell((0, 0)).unwrap().fg, Color::White);

    // With --colors campbell those slots resolve to the fixed Campbell RGB.
    app.colors = Colors::Campbell;
    let campbell_buf = harness.render(|frame| views::commit_list::render(&mut app, frame));
    assert_eq!(
        campbell_buf.cell((0, 0)).unwrap().bg,
        Color::Rgb(0x13, 0xa1, 0x0e),
        "Campbell should resolve the green header background to its RGB"
    );
    assert_eq!(
        campbell_buf.cell((0, 0)).unwrap().fg,
        Color::Rgb(0xf2, 0xf2, 0xf2),
        "Campbell should resolve the white header foreground to its RGB"
    );
}

#[test]
fn test_commit_list_long_summary() {
    let mut harness = TuiTestHarness::short();

    let mut app = AppState::new();
    app.commits = vec![
        common::create_test_commit(
            "abc123def456",
            "This is a very long commit summary that exceeds normal length",
        ),
        common::create_test_commit("def456ghi789", "Short"),
    ];
    app.selection_index = 0;

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
    }));
}

#[test]
fn test_commit_list_scrolled_to_top() {
    // 10 commits, 60-col terminal — scrollbar should appear.
    let mut harness = TuiTestHarness::narrow();

    let mut app = AppState::new();
    app.commits = (0..10)
        .map(|i| {
            common::create_test_commit(&format!("{:012x}", i), &format!("Commit number {}", i))
        })
        .collect();
    app.selection_index = 0;

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
    }));
}

#[test]
fn test_commit_list_scrolled_to_bottom() {
    // Same setup as above, but selection at last commit.
    let mut harness = TuiTestHarness::narrow();

    let mut app = AppState::new();
    app.commits = (0..10)
        .map(|i| {
            common::create_test_commit(&format!("{:012x}", i), &format!("Commit number {}", i))
        })
        .collect();
    app.selection_index = 9;

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
    }));
}

#[test]
fn test_commit_list_reversed_with_commits() {
    let mut harness = TuiTestHarness::typical();

    let mut app = AppState::new();
    app.commits = vec![
        common::create_test_commit("abc123def456", "Initial commit"),
        common::create_test_commit("def456ghi789", "Add feature X"),
        common::create_test_commit("ghi789jkl012", "Fix bug in parser"),
    ];
    app.selection_index = 2; // HEAD
    app.reverse = true;

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
    }));
}

#[test]
fn test_commit_list_reversed_scrolled() {
    // 10 commits, reverse mode with HEAD selected (index 9) → visual index 0 (top).
    let mut harness = TuiTestHarness::narrow();

    let mut app = AppState::new();
    app.commits = (0..10)
        .map(|i| {
            common::create_test_commit(&format!("{:012x}", i), &format!("Commit number {}", i))
        })
        .collect();
    app.selection_index = 9; // HEAD
    app.reverse = true;

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
    }));
}

#[test]
fn test_status_bar_short_error() {
    let mut harness = TuiTestHarness::short();

    let mut app = AppState::new();
    app.commits = vec![common::create_test_commit("abc123def456", "Initial commit")];
    app.selection_index = 0;
    app.status_message = Some("Cannot split staged/unstaged changes".to_string());
    app.status_is_error = true;

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
    }));
}
// Footer is exactly 65 columns wide: left=" abc123def456abc123def456abc123def456abc1 1/1" (45 chars)
// + 2 padding + hint "Press 'h' for help" (18 chars) = 65. Hint should be visible.
#[test]
fn test_footer_hint_fits() {
    let mut harness = TuiTestHarness::new(65, 5);

    let mut app = AppState::new();
    app.commits = vec![common::create_test_commit(
        "abc123def456abc123def456abc123def456abc1",
        "A commit",
    )];
    app.selection_index = 0;

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
    }));
}

// Footer is 64 columns wide: one column too narrow to fit the hint (needs 65). Hint suppressed.
#[test]
fn test_footer_hint_too_narrow() {
    let mut harness = TuiTestHarness::new(64, 5);

    let mut app = AppState::new();
    app.commits = vec![common::create_test_commit(
        "abc123def456abc123def456abc123def456abc1",
        "A commit",
    )];
    app.selection_index = 0;

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
    }));
}
// When an update notice is set it replaces the help hint in the right-hand
// footer slot (highlighted), instead of occupying any extra space.
#[test]
fn test_footer_update_notice() {
    let mut harness = TuiTestHarness::new(80, 5);

    let mut app = AppState::new();
    app.commits = vec![common::create_test_commit(
        "abc123def456abc123def456abc123def456abc1",
        "A commit",
    )];
    app.selection_index = 0;
    app.update_notice = Some("Version 9.9.9 available".to_string());

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
    }));
}

#[test]
fn test_status_bar_long_error() {
    let mut harness = TuiTestHarness::short();

    let mut app = AppState::new();
    app.commits = vec![common::create_test_commit("abc123def456", "Initial commit")];
    app.selection_index = 0;
    app.status_message = Some(
        "Split failed: cannot apply patch — overlapping hunks detected in modified_file.rs"
            .to_string(),
    );
    app.status_is_error = true;

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
    }));
}

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use git_tailor::VirtualOid;
use git_tailor::app::{AppAction, AppMode, KeyCommand};

fn key(c: char, modifiers: KeyModifiers) -> Event {
    Event::Key(KeyEvent {
        code: KeyCode::Char(c),
        modifiers,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    })
}

fn synthetic_row(oid: VirtualOid, summary: &str) -> git_tailor::CommitInfo {
    let mut commit = common::create_test_commit("abc123def456", summary);
    commit.oid = oid;
    commit
}

#[test]
fn stage_unstage_keys_are_distinct_from_ctrl_variants() {
    // `a` / `A` map unconditionally (the row gating lives in the handler); only
    // the Ctrl-modified variant carries a different, scroll meaning.
    let list = AppMode::CommitList;
    assert_eq!(
        list.parse_key(key('a', KeyModifiers::NONE)),
        KeyCommand::StageAll
    );
    assert_eq!(
        list.parse_key(key('A', KeyModifiers::SHIFT)),
        KeyCommand::UnstageAll
    );
    assert_eq!(
        list.parse_key(key('a', KeyModifiers::CONTROL)),
        KeyCommand::ScrollToLeftEdge
    );
}

#[test]
fn stage_all_is_gated_to_the_unstaged_row() {
    let mut app = AppState::new();
    app.commits = vec![
        common::create_test_commit("abc123def456", "real commit"),
        synthetic_row(VirtualOid::Unstaged, "(unstaged changes)"),
    ];

    app.selection_index = 1;
    assert!(matches!(
        views::commit_list::handle_key(KeyCommand::StageAll, &mut app),
        AppAction::StageAll
    ));

    // On any other row it is a no-op with a guiding hint.
    app.selection_index = 0;
    assert!(matches!(
        views::commit_list::handle_key(KeyCommand::StageAll, &mut app),
        AppAction::Handled
    ));
    assert!(app.status_is_error);
}

#[test]
fn unstage_all_is_gated_to_the_staged_row() {
    let mut app = AppState::new();
    app.commits = vec![
        synthetic_row(VirtualOid::Staged, "(staged changes)"),
        common::create_test_commit("abc123def456", "real commit"),
    ];

    app.selection_index = 0;
    assert!(matches!(
        views::commit_list::handle_key(KeyCommand::UnstageAll, &mut app),
        AppAction::UnstageAll
    ));

    app.selection_index = 1;
    assert!(matches!(
        views::commit_list::handle_key(KeyCommand::UnstageAll, &mut app),
        AppAction::Handled
    ));
    assert!(app.status_is_error);
}

#[test]
fn commit_key_is_distinct_from_ctrl_force_quit() {
    // `c` maps unconditionally (row gating is in the handler); Ctrl-c stays the
    // force-quit chord.
    let list = AppMode::CommitList;
    assert_eq!(
        list.parse_key(key('c', KeyModifiers::NONE)),
        KeyCommand::CommitStaged
    );
    assert_eq!(
        list.parse_key(key('c', KeyModifiers::CONTROL)),
        KeyCommand::ForceQuit
    );
}

#[test]
fn commit_staged_is_gated_to_the_staged_row() {
    let mut app = AppState::new();
    app.commits = vec![
        synthetic_row(VirtualOid::Staged, "(staged changes)"),
        common::create_test_commit("abc123def456", "real commit"),
    ];

    app.selection_index = 0;
    assert!(matches!(
        views::commit_list::handle_key(KeyCommand::CommitStaged, &mut app),
        AppAction::PrepareCommitStaged
    ));

    // On any other row it is a no-op with a guiding hint.
    app.selection_index = 1;
    assert!(matches!(
        views::commit_list::handle_key(KeyCommand::CommitStaged, &mut app),
        AppAction::Handled
    ));
    assert!(app.status_is_error);
}

#[test]
fn test_commit_list_ctrl_scroll_keeps_selection_visible() {
    // A list taller than the window, selection in the middle.
    let mut harness = TuiTestHarness::narrow();
    let mut app = AppState::new();
    app.commits = (0..12)
        .map(|i| {
            common::create_test_commit(&format!("{:012x}", i), &format!("Commit number {}", i))
        })
        .collect();
    app.selection_index = 6;

    // The first render establishes the visible height the scroll clamps against.
    let _ = harness.render(|frame| views::commit_list::render(&mut app, frame));
    // Scroll the viewport down twice without moving the selection.
    app.scroll_commit_list_down();
    app.scroll_commit_list_down();

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
    }));
}
