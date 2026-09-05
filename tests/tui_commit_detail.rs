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

// TUI snapshot tests for the commit detail view, covering the horizontal
// scrollbar that appears when content lines exceed the terminal width.

#[allow(dead_code)]
mod common;

use git_tailor::{
    CommitDiff, DeltaStatus, DiffLine, DiffLineKind, FileDiff, Hunk, app::AppState, views,
};

use common::{StubRepoBuilder, TuiTestHarness};
use ratatui::buffer::Buffer;

fn make_repo_with_empty_diff(oid: &str, summary: &str) -> common::StubRepo {
    let diff = CommitDiff {
        commit: common::create_test_commit(oid, summary),
        files: vec![],
    };
    StubRepoBuilder::new().with_commit_diff(diff).build()
}

/// Short message: all content lines fit within 80 columns — no horizontal scrollbar.
#[test]
fn test_commit_detail_short_lines_no_hscroll() {
    let repo = make_repo_with_empty_diff("abc123def456", "Short commit");
    let mut harness = TuiTestHarness::typical();

    let mut app = AppState::new();
    app.list.commits = vec![common::create_test_commit("abc123def456", "Short commit")];
    app.list.selection_index = 0;

    views::commit_detail::load_diff(&repo, &mut app);
    insta::assert_debug_snapshot!(harness.render(|frame| {
        let area = frame.area();
        views::commit_detail::render(frame, &mut app, area);
    }));
}

/// Long message line (100 chars) exceeds the 80-column terminal width, so the
/// horizontal scrollbar row must appear at the bottom of the content area.
#[test]
fn test_commit_detail_long_lines_hscroll_visible() {
    let long_message = "A".repeat(100);
    let repo = make_repo_with_empty_diff("abc123def456", &long_message);
    let mut harness = TuiTestHarness::typical();

    let mut app = AppState::new();
    app.list.commits = vec![common::create_test_commit("abc123def456", &long_message)];
    app.list.selection_index = 0;

    views::commit_detail::load_diff(&repo, &mut app);
    insta::assert_debug_snapshot!(harness.render(|frame| {
        let area = frame.area();
        views::commit_detail::render(frame, &mut app, area);
    }));
}

/// With a positive `detail.h.offset`, the paragraph is rendered
/// starting from a later column so the leading characters of long lines
/// are clipped out of view.
#[test]
fn test_commit_detail_hscroll_offset_clips_content() {
    let long_message = "A".repeat(100);
    let repo = make_repo_with_empty_diff("abc123def456", &long_message);
    let mut harness = TuiTestHarness::typical();

    let mut app = AppState::new();
    app.list.commits = vec![common::create_test_commit("abc123def456", &long_message)];
    app.list.selection_index = 0;
    app.detail.h.offset = 10;

    views::commit_detail::load_diff(&repo, &mut app);
    insta::assert_debug_snapshot!(harness.render(|frame| {
        let area = frame.area();
        views::commit_detail::render(frame, &mut app, area);
    }));
}

/// A scroll offset past the content bottom (e.g. after the diff shrinks because
/// the context lines were reduced) is clamped in place to `detail.v.max`,
/// so the user can scroll back up immediately rather than being stuck.
#[test]
fn test_detail_scroll_offset_clamped_to_content_bottom() {
    let repo = make_repo_with_empty_diff("abc123def456", "Short commit");
    let mut harness = TuiTestHarness::typical();

    let mut app = AppState::new();
    app.list.commits = vec![common::create_test_commit("abc123def456", "Short commit")];
    app.list.selection_index = 0;
    app.detail.v.offset = 9999; // far beyond the (tiny) content

    views::commit_detail::load_diff(&repo, &mut app);
    harness.render(|frame| {
        let area = frame.area();
        views::commit_detail::render(frame, &mut app, area);
    });

    assert_eq!(
        app.detail.v.offset, app.detail.v.max,
        "scroll offset should be clamped to the content bottom, not left stale"
    );
}

/// Diff lines from a file with Windows (CRLF) line endings must be stripped of
/// `\r` before rendering — a trailing carriage return in a cell causes the
/// cursor to jump to column 0, overwriting content on real terminals.
#[test]
fn test_commit_detail_crlf_lines_no_carriage_return() {
    let diff = CommitDiff {
        commit: common::create_test_commit("crlf001", "File with CRLF line endings"),
        files: vec![FileDiff {
            old_path: Some("hello.txt".to_string()),
            new_path: Some("hello.txt".to_string()),
            status: DeltaStatus::Modified,
            is_binary: false,
            hunks: vec![Hunk {
                old_start: 1,
                old_lines: 2,
                new_start: 1,
                new_lines: 2,
                lines: vec![
                    DiffLine {
                        kind: DiffLineKind::Context,
                        content: "unchanged line\r\n".to_string(),
                    },
                    DiffLine {
                        kind: DiffLineKind::Deletion,
                        content: "old content\r\n".to_string(),
                    },
                    DiffLine {
                        kind: DiffLineKind::Addition,
                        content: "new content\r\n".to_string(),
                    },
                ],
            }],
        }],
    };
    let repo = StubRepoBuilder::new().with_commit_diff(diff).build();
    let mut harness = TuiTestHarness::typical();

    let mut app = AppState::new();
    app.list.commits = vec![common::create_test_commit(
        "crlf001",
        "File with CRLF line endings",
    )];
    app.list.selection_index = 0;

    views::commit_detail::load_diff(&repo, &mut app);
    let buf = harness.render(|frame| {
        let area = frame.area();
        views::commit_detail::render(frame, &mut app, area);
    });

    for cell in buf.content() {
        assert!(
            !cell.symbol().contains('\r'),
            "carriage return found in rendered cell: {:?}",
            cell.symbol()
        );
    }
    insta::assert_debug_snapshot!(buf);
}

// --- Unit tests for horizontal detail scrolling ---

#[test]
fn test_scroll_detail_right_increments() {
    let mut app = AppState::new();
    app.detail.h.max = 20;
    app.detail.h.offset = 0;
    app.detail.h.step_forward();
    assert_eq!(app.detail.h.offset, 1);
}

#[test]
fn test_scroll_detail_right_clamps_at_max() {
    let mut app = AppState::new();
    app.detail.h.max = 5;
    app.detail.h.offset = 5;
    app.detail.h.step_forward();
    assert_eq!(app.detail.h.offset, 5);
}

#[test]
fn test_scroll_detail_left_decrements() {
    let mut app = AppState::new();
    app.detail.h.offset = 5;
    app.detail.h.step_back();
    assert_eq!(app.detail.h.offset, 4);
}

#[test]
fn test_scroll_detail_left_clamps_at_zero() {
    let mut app = AppState::new();
    app.detail.h.offset = 0;
    app.detail.h.step_back();
    assert_eq!(app.detail.h.offset, 0);
}

// --- Search feature tests ---

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use git_tailor::app::{AppAction, AppMode, KeyCommand};

fn make_key_event(code: KeyCode) -> Event {
    Event::Key(KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    })
}

/// Snapshot: search bar appears in footer when search is activated.
#[test]
fn test_commit_detail_search_bar_visible() {
    let diff = CommitDiff {
        commit: common::create_test_commit("abc123", "Add feature"),
        files: vec![FileDiff {
            old_path: Some("hello.txt".to_string()),
            new_path: Some("hello.txt".to_string()),
            status: DeltaStatus::Modified,
            is_binary: false,
            hunks: vec![Hunk {
                old_start: 1,
                old_lines: 1,
                new_start: 1,
                new_lines: 1,
                lines: vec![
                    DiffLine {
                        kind: DiffLineKind::Deletion,
                        content: "old line\n".to_string(),
                    },
                    DiffLine {
                        kind: DiffLineKind::Addition,
                        content: "new line\n".to_string(),
                    },
                ],
            }],
        }],
    };
    let repo = StubRepoBuilder::new().with_commit_diff(diff).build();
    let mut harness = TuiTestHarness::typical();

    let mut app = AppState::new();
    app.list.commits = vec![common::create_test_commit("abc123", "Add feature")];
    app.list.selection_index = 0;
    app.mode = AppMode::CommitDetail;

    // Activate search and type a query
    app.search.activate();
    app.search.query = "old".to_string();

    views::commit_detail::load_diff(&repo, &mut app);
    insta::assert_debug_snapshot!(harness.render(|frame| {
        let area = frame.area();
        views::commit_detail::render(frame, &mut app, area);
    }));
}

/// Snapshot: search highlights matching text in diff lines.
#[test]
fn test_commit_detail_search_highlight_matches() {
    let diff = CommitDiff {
        commit: common::create_test_commit("abc123", "Add feature"),
        files: vec![FileDiff {
            old_path: Some("hello.txt".to_string()),
            new_path: Some("hello.txt".to_string()),
            status: DeltaStatus::Modified,
            is_binary: false,
            hunks: vec![Hunk {
                old_start: 1,
                old_lines: 2,
                new_start: 1,
                new_lines: 2,
                lines: vec![
                    DiffLine {
                        kind: DiffLineKind::Context,
                        content: "context line\n".to_string(),
                    },
                    DiffLine {
                        kind: DiffLineKind::Deletion,
                        content: "hello world\n".to_string(),
                    },
                    DiffLine {
                        kind: DiffLineKind::Addition,
                        content: "hello universe\n".to_string(),
                    },
                ],
            }],
        }],
    };
    let repo = StubRepoBuilder::new().with_commit_diff(diff).build();
    let mut harness = TuiTestHarness::typical();

    let mut app = AppState::new();
    app.list.commits = vec![common::create_test_commit("abc123", "Add feature")];
    app.list.selection_index = 0;
    app.mode = AppMode::CommitDetail;

    // Search for "hello" — should match 2 diff lines + file path
    app.search.activate();
    app.search.query = "hello".to_string();
    app.search.input_active = false; // Confirmed search

    views::commit_detail::load_diff(&repo, &mut app);
    insta::assert_debug_snapshot!(harness.render(|frame| {
        let area = frame.area();
        views::commit_detail::render(frame, &mut app, area);
    }));
}

/// handle_search_event appends characters to the query.
#[test]
fn test_search_event_char_input() {
    let mut app = AppState::new();
    app.mode = AppMode::CommitDetail;
    app.search.activate();

    views::commit_detail::handle_search_event(make_key_event(KeyCode::Char('f')), &mut app);
    views::commit_detail::handle_search_event(make_key_event(KeyCode::Char('o')), &mut app);
    views::commit_detail::handle_search_event(make_key_event(KeyCode::Char('o')), &mut app);

    assert_eq!(app.search.query, "foo");
    assert!(app.search.input_active);
    assert!(app.search.active);
}

/// Backspace removes the last character from the search query.
#[test]
fn test_search_event_backspace() {
    let mut app = AppState::new();
    app.mode = AppMode::CommitDetail;
    app.search.activate();
    app.search.query = "foo".to_string();

    views::commit_detail::handle_search_event(make_key_event(KeyCode::Backspace), &mut app);

    assert_eq!(app.search.query, "fo");
}

/// Enter confirms the search (keeps active, stops input).
#[test]
fn test_search_event_enter_confirms() {
    let mut app = AppState::new();
    app.mode = AppMode::CommitDetail;
    app.search.activate();
    app.search.query = "foo".to_string();

    views::commit_detail::handle_search_event(make_key_event(KeyCode::Enter), &mut app);

    assert!(!app.search.input_active);
    assert!(app.search.active);
    assert_eq!(app.search.query, "foo");
}

/// Enter jumps to the first match at or after the current scroll position.
#[test]
fn test_search_event_enter_jumps_to_match_at_scroll_offset() {
    let mut app = AppState::new();
    app.mode = AppMode::CommitDetail;
    app.search.activate();
    app.search.query = "foo".to_string();
    app.search.matches = vec![5, 20, 30];
    app.search.match_index = Some(0);
    app.detail.v.offset = 12;
    app.detail.v.visible_height = 50;

    views::commit_detail::handle_search_event(make_key_event(KeyCode::Enter), &mut app);

    // First match at or after scroll offset 12 is line 20 (index 1).
    assert_eq!(app.search.match_index, Some(1));
}

/// Enter wraps to match 0 when all matches lie above the current scroll position.
#[test]
fn test_search_event_enter_wraps_when_past_all_matches() {
    let mut app = AppState::new();
    app.mode = AppMode::CommitDetail;
    app.search.activate();
    app.search.query = "foo".to_string();
    app.search.matches = vec![5, 8];
    app.search.match_index = Some(1);
    app.detail.v.offset = 20;
    app.detail.v.visible_height = 50;

    views::commit_detail::handle_search_event(make_key_event(KeyCode::Enter), &mut app);

    // All matches are above scroll offset 20; wraps to first match.
    assert_eq!(app.search.match_index, Some(0));
}

/// Enter is a no-op (for navigation) when there are no matches.
#[test]
fn test_search_event_enter_no_op_when_no_matches() {
    let mut app = AppState::new();
    app.mode = AppMode::CommitDetail;
    app.search.activate();
    app.search.query = "foo".to_string();
    // search.matches is empty (default)

    views::commit_detail::handle_search_event(make_key_event(KeyCode::Enter), &mut app);

    assert!(!app.search.input_active);
    assert!(app.search.active);
    assert_eq!(app.search.match_index, None);
}

/// Escape dismisses the search entirely.
#[test]
fn test_search_event_escape_dismisses() {
    let mut app = AppState::new();
    app.mode = AppMode::CommitDetail;
    app.search.activate();
    app.search.query = "foo".to_string();

    views::commit_detail::handle_search_event(make_key_event(KeyCode::Esc), &mut app);

    assert!(!app.search.input_active);
    assert!(!app.search.active);
    assert!(app.search.query.is_empty());
}

/// Quit (Esc) in confirmed-search mode clears search instead of leaving detail.
#[test]
fn test_quit_clears_search_before_leaving_detail() {
    let mut app = AppState::new();
    app.mode = AppMode::CommitDetail;
    app.search.active = true;
    app.search.query = "foo".to_string();

    let result = views::commit_detail::handle_key(KeyCommand::Quit, &mut app);

    assert!(matches!(result, AppAction::Handled));
    assert!(!app.search.active);
    // Still in CommitDetail — didn't leave
    assert!(matches!(app.mode, AppMode::CommitDetail));
}

/// n/N cycle through search matches, wrapping around.
#[test]
fn test_search_next_prev_wraps() {
    let mut app = AppState::new();
    app.mode = AppMode::CommitDetail;
    app.search.active = true;
    app.search.matches = vec![5, 10, 20];
    app.search.match_index = Some(0);
    app.detail.v.visible_height = 100; // large enough to avoid scrolling

    // Forward
    views::commit_detail::handle_key(KeyCommand::SearchNext, &mut app);
    assert_eq!(app.search.match_index, Some(1));

    views::commit_detail::handle_key(KeyCommand::SearchNext, &mut app);
    assert_eq!(app.search.match_index, Some(2));

    // Wrap forward
    views::commit_detail::handle_key(KeyCommand::SearchNext, &mut app);
    assert_eq!(app.search.match_index, Some(0));

    // Wrap backward
    views::commit_detail::handle_key(KeyCommand::SearchPrev, &mut app);
    assert_eq!(app.search.match_index, Some(2));
}

/// Regex search is case-sensitive: "FOO" must not match "foo".
#[test]
fn test_search_case_sensitive() {
    let diff = CommitDiff {
        commit: common::create_test_commit("abc123", "Add FOO feature"),
        files: vec![FileDiff {
            old_path: Some("foo.txt".to_string()),
            new_path: Some("foo.txt".to_string()),
            status: DeltaStatus::Modified,
            is_binary: false,
            hunks: vec![Hunk {
                old_start: 1,
                old_lines: 1,
                new_start: 1,
                new_lines: 2,
                lines: vec![
                    DiffLine {
                        kind: DiffLineKind::Addition,
                        content: "FOO bar\n".to_string(),
                    },
                    DiffLine {
                        kind: DiffLineKind::Addition,
                        content: "foo baz\n".to_string(),
                    },
                ],
            }],
        }],
    };
    let repo = StubRepoBuilder::new().with_commit_diff(diff).build();
    let mut harness = TuiTestHarness::typical();

    let mut app = AppState::new();
    app.list.commits = vec![common::create_test_commit("abc123", "Add FOO feature")];
    app.list.selection_index = 0;
    app.mode = AppMode::CommitDetail;

    // "FOO" must only match uppercase occurrences, not "foo"
    app.search.activate();
    app.search.query = "FOO".to_string();
    app.search.input_active = false;

    views::commit_detail::load_diff(&repo, &mut app);
    let buf = harness.render(|frame| {
        let area = frame.area();
        views::commit_detail::render(frame, &mut app, area);
    });

    // "FOO" appears in the commit message and "+FOO bar" diff line.
    // "foo" in file path and "+foo baz" must NOT match.
    assert_eq!(app.search.matches.len(), 2);

    insta::assert_debug_snapshot!(buf);
}

/// parse_key maps / to Search, n to SearchNext, N to SearchPrev in CommitDetail.
#[test]
fn test_parse_key_search_bindings() {
    let mode = AppMode::CommitDetail;

    let slash_event = Event::Key(KeyEvent {
        code: KeyCode::Char('/'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    });
    assert_eq!(mode.parse_key(slash_event), KeyCommand::Search);

    let n_event = Event::Key(KeyEvent {
        code: KeyCode::Char('n'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    });
    assert_eq!(mode.parse_key(n_event), KeyCommand::SearchNext);

    let shift_n_event = Event::Key(KeyEvent {
        code: KeyCode::Char('N'),
        modifiers: KeyModifiers::SHIFT,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    });
    assert_eq!(mode.parse_key(shift_n_event), KeyCommand::SearchPrev);
}

/// / n N should NOT produce search commands in CommitList mode.
#[test]
fn test_search_keys_map_unconditionally() {
    // `/` `n` `N` map to their search commands in every mode; only the
    // detail-view handler acts on them (they are a no-op elsewhere).
    let press = |c: char| {
        Event::Key(KeyEvent {
            code: KeyCode::Char(c),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    };
    let mode = AppMode::CommitList;
    assert_eq!(mode.parse_key(press('/')), KeyCommand::Search);
    assert_eq!(mode.parse_key(press('n')), KeyCommand::SearchNext);
    assert_eq!(mode.parse_key(press('N')), KeyCommand::SearchPrev);
}

/// A search match that is already on screen must not make the view jump.
///
/// The detail view records its scroll bounds during render but clamps the
/// stored offset afterwards. When the content shrinks (fewer context lines, a
/// taller terminal), the search auto-scroll decided whether the current match
/// was visible using the *pre-clamp* offset — a window past the end of the
/// content that is never rendered — and so recenterd a match that the clamped
/// view already showed.
#[test]
fn test_search_does_not_jump_when_the_match_is_already_visible() {
    let mut lines = Vec::new();
    for i in 0..100 {
        lines.push(DiffLine {
            kind: DiffLineKind::Context,
            content: if i == 70 {
                "NEEDLE here\n".to_string()
            } else {
                format!("filler line {i}\n")
            },
        });
    }
    let diff = CommitDiff {
        commit: common::create_test_commit("abc123", "Long diff"),
        files: vec![FileDiff {
            old_path: Some("big.txt".to_string()),
            new_path: Some("big.txt".to_string()),
            status: DeltaStatus::Modified,
            is_binary: false,
            hunks: vec![Hunk {
                old_start: 1,
                old_lines: 100,
                new_start: 1,
                new_lines: 100,
                lines,
            }],
        }],
    };
    let repo = StubRepoBuilder::new().with_commit_diff(diff).build();

    let mut app = AppState::new();
    app.list.commits = vec![common::create_test_commit("abc123", "Long diff")];
    app.list.selection_index = 0;
    app.mode = AppMode::CommitDetail;
    app.search.activate();
    app.search.query = "NEEDLE".to_string();
    app.search.input_active = false;

    // Short terminal: render, then scroll to the bottom.
    let mut short = TuiTestHarness::new(80, 20);
    views::commit_detail::load_diff(&repo, &mut app);
    short.render(|frame| {
        let area = frame.area();
        views::commit_detail::render(frame, &mut app, area);
    });
    app.detail.v.to_end();
    let stale = app.detail.v.offset;

    // Force the match index to change so the auto-scroll runs this frame.
    app.search.match_index = Some(999);

    // Taller terminal: more fits, so the maximum drops below `stale`.
    let mut tall = TuiTestHarness::new(80, 45);
    tall.render(|frame| {
        let area = frame.area();
        views::commit_detail::render(frame, &mut app, area);
    });

    let max = app.detail.v.max;
    let vh = app.detail.v.visible_height;
    let target = app.search.matches[app.search.match_index.unwrap()];

    // Guard the premises, so a layout change makes this test fail loudly rather
    // than pass without exercising anything.
    assert!(
        max < stale,
        "premise: the maximum must drop below the stale offset"
    );
    assert!(
        target < stale,
        "premise: the match must lie above the stale offset, or both the clamped \
         and unclamped windows agree and nothing is being tested"
    );
    assert!(
        (max..max + vh).contains(&target),
        "premise: the match must be visible once the offset is clamped to the bottom"
    );
    assert_eq!(
        app.detail.v.offset, max,
        "an already-visible match must not scroll the view"
    );
}

/// Opening the detail view queries the repository for the diff; redrawing the
/// same view must not. Re-reading the diff on every frame made cursor movement
/// visibly laggy on filesystems where a libgit2 diff is not nearly free.
#[test]
fn test_commit_detail_redraw_does_not_requery_the_repository() {
    let repo = make_repo_with_empty_diff("abc123def456", "Short commit");
    let mut harness = TuiTestHarness::typical();

    let mut app = AppState::new();
    app.list.commits = vec![common::create_test_commit("abc123def456", "Short commit")];
    app.list.selection_index = 0;

    views::commit_detail::load_diff(&repo, &mut app);
    for _ in 0..3 {
        harness.render(|frame| {
            let area = frame.area();
            views::commit_detail::render(frame, &mut app, area);
        });
    }

    assert_eq!(repo.commit_diff_calls(), 1);
}

/// Every row of a rendered buffer as a trimmed string.
fn buffer_rows(buf: &Buffer) -> Vec<String> {
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .filter_map(|x| buf.cell((x, y)).map(|c| c.symbol().to_string()))
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect()
}

/// Render the detail view for a commit whose diff is `files`, and return the
/// rendered rows.
fn render_detail_rows(files: Vec<FileDiff>) -> Vec<String> {
    let commit = common::create_test_commit("abc123def456", "Hunkless change");
    let repo = StubRepoBuilder::new()
        .with_commit_diff(CommitDiff {
            commit: commit.clone(),
            files,
        })
        .build();

    let mut app = AppState::new();
    app.list.commits = vec![commit];
    app.list.selection_index = 0;
    views::commit_detail::load_diff(&repo, &mut app);

    let mut harness = TuiTestHarness::typical();
    let buf = harness.render(|frame| {
        let area = frame.area();
        views::commit_detail::render(frame, &mut app, area);
    });
    buffer_rows(&buf)
}

fn hunkless_file(path: &str, status: DeltaStatus, is_binary: bool) -> FileDiff {
    FileDiff {
        old_path: Some(path.to_string()),
        new_path: Some(path.to_string()),
        status,
        is_binary,
        hunks: vec![],
    }
}

/// A hunkless delta renders as a bare `---`/`+++` header pair with nothing under
/// it, which reads as "no change at all". Each kind gets a marker line saying
/// what actually happened.
#[test]
fn test_detail_marks_a_binary_file() {
    let rows = render_detail_rows(vec![hunkless_file("blob.bin", DeltaStatus::Modified, true)]);

    assert!(
        rows.iter().any(|r| r.contains("Binary file differs")),
        "expected a binary marker, got:\n{}",
        rows.join("\n")
    );
}

#[test]
fn test_detail_marks_an_added_empty_file() {
    let rows = render_detail_rows(vec![hunkless_file("empty.txt", DeltaStatus::Added, false)]);

    assert!(
        rows.iter().any(|r| r.contains("(empty file)")),
        "expected an empty-file marker, got:\n{}",
        rows.join("\n")
    );
}

#[test]
fn test_detail_marks_a_hunkless_modification() {
    let rows = render_detail_rows(vec![hunkless_file(
        "script.sh",
        DeltaStatus::Modified,
        false,
    )]);

    assert!(
        rows.iter().any(|r| r.contains("(no content changes)")),
        "expected a no-content-changes marker, got:\n{}",
        rows.join("\n")
    );
}

/// A file that does have hunks must not gain a marker.
#[test]
fn test_detail_does_not_mark_a_file_with_hunks() {
    let file = FileDiff {
        old_path: Some("file.txt".to_string()),
        new_path: Some("file.txt".to_string()),
        status: DeltaStatus::Modified,
        is_binary: false,
        hunks: vec![Hunk {
            old_start: 1,
            old_lines: 1,
            new_start: 1,
            new_lines: 1,
            lines: vec![
                DiffLine {
                    kind: DiffLineKind::Deletion,
                    content: "old\n".to_string(),
                },
                DiffLine {
                    kind: DiffLineKind::Addition,
                    content: "new\n".to_string(),
                },
            ],
        }],
    };
    let rows = render_detail_rows(vec![file]);

    assert!(rows.iter().any(|r| r.contains("+new")));
    assert!(
        !rows.iter().any(|r| r.contains("(no content changes)")
            || r.contains("(empty file)")
            || r.contains("Binary file differs")),
        "a file with hunks must not be marked, got:\n{}",
        rows.join("\n")
    );
}
