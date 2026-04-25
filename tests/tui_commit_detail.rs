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

mod common;

use anyhow::{Result, anyhow};
use git_tailor::{
    CommitDiff, CommitInfo, DeltaStatus, DiffLine, DiffLineKind, FileDiff, Hunk, Oid,
    app::AppState,
    repo::{ConflictState, GitRepo, RebaseOutcome, SquashContext},
    views,
};
use ratatui::{Terminal, backend::TestBackend};

/// Minimal GitRepo stub for commit detail tests.
///
/// Returns no diff for any commit so tests exercise metadata rendering and
/// the scrollbar logic without needing a real repository on disk.
struct NoOpRepo;

impl GitRepo for NoOpRepo {
    fn head_oid(&self) -> Result<Oid> {
        unimplemented!()
    }
    fn find_reference_point(&self, _commit_ish: &str) -> Result<Oid> {
        unimplemented!()
    }
    fn list_commits(&self, _from: &Oid, _to: &Oid) -> Result<Vec<CommitInfo>> {
        unimplemented!()
    }
    fn commit_diff(&self, _oid: &Oid) -> Result<CommitDiff> {
        Err(anyhow!("no diff"))
    }
    fn commit_diff_for_fragmap(&self, _oid: &Oid) -> Result<CommitDiff> {
        unimplemented!()
    }
    fn staged_diff(&self) -> Option<CommitDiff> {
        None
    }
    fn unstaged_diff(&self) -> Option<CommitDiff> {
        None
    }
    fn split_commit_per_file(&self, _commit_oid: &Oid, _head_oid: &Oid) -> Result<()> {
        unimplemented!()
    }
    fn split_commit_per_hunk(&self, _commit_oid: &Oid, _head_oid: &Oid) -> Result<()> {
        unimplemented!()
    }
    fn split_commit_per_hunk_group(
        &self,
        _commit_oid: &Oid,
        _head_oid: &Oid,
        _reference_oid: &Oid,
    ) -> Result<()> {
        unimplemented!()
    }
    fn count_split_per_file(&self, _commit_oid: &Oid) -> Result<usize> {
        unimplemented!()
    }
    fn count_split_per_hunk(&self, _commit_oid: &Oid) -> Result<usize> {
        unimplemented!()
    }
    fn count_split_per_hunk_group(
        &self,
        _commit_oid: &Oid,
        _head_oid: &Oid,
        _reference_oid: &Oid,
    ) -> Result<usize> {
        unimplemented!()
    }
    fn reword_commit(&self, _commit_oid: &Oid, _new_message: &str, _head_oid: &Oid) -> Result<()> {
        unimplemented!()
    }
    fn get_config_string(&self, _key: &str) -> Option<String> {
        unimplemented!()
    }
    fn drop_commit(&self, _commit_oid: &Oid, _head_oid: &Oid) -> Result<RebaseOutcome> {
        unimplemented!()
    }
    fn move_commit(
        &self,
        _commit_oid: &Oid,
        _insert_after_oid: Option<&Oid>,
        _head_oid: &Oid,
    ) -> Result<RebaseOutcome> {
        unimplemented!()
    }
    fn rebase_continue(&self, _state: &ConflictState) -> Result<RebaseOutcome> {
        unimplemented!()
    }
    fn rebase_abort(&self, _state: &ConflictState) -> Result<()> {
        unimplemented!()
    }
    fn workdir(&self) -> Option<std::path::PathBuf> {
        unimplemented!()
    }
    fn read_index_stage(&self, _path: &str, _stage: i32) -> Result<Option<Vec<u8>>> {
        unimplemented!()
    }
    fn read_conflicting_files(&self) -> Vec<String> {
        unimplemented!()
    }
    fn squash_commits(
        &self,
        _source_oid: &Oid,
        _target_oid: &Oid,
        _message: &str,
        _head_oid: &Oid,
    ) -> Result<RebaseOutcome> {
        unimplemented!()
    }
    fn squash_try_combine(
        &self,
        _source_oid: &Oid,
        _target_oid: &Oid,
        _combined_message: &str,
        _is_fixup: bool,
        _head_oid: &Oid,
    ) -> Result<Option<ConflictState>> {
        unimplemented!()
    }
    fn squash_finalize(
        &self,
        _ctx: &SquashContext,
        _message: &str,
        _original_branch_oid: &Oid,
    ) -> Result<RebaseOutcome> {
        unimplemented!()
    }
    fn stage_file(&self, _path: &str) -> Result<()> {
        unimplemented!()
    }
    fn auto_stage_resolved_conflicts(&self, _files: &[String]) -> Result<()> {
        unimplemented!()
    }

    fn default_branch(&self) -> Option<String> {
        None
    }

    fn root_commit_oid(&self) -> Result<Oid> {
        unimplemented!()
    }
}

/// GitRepo stub that returns a fixed CommitDiff for the selected commit OID.
struct FakeDiffRepo(CommitDiff);

impl GitRepo for FakeDiffRepo {
    fn head_oid(&self) -> Result<Oid> {
        unimplemented!()
    }
    fn find_reference_point(&self, _commit_ish: &str) -> Result<Oid> {
        unimplemented!()
    }
    fn list_commits(&self, _from: &Oid, _to: &Oid) -> Result<Vec<CommitInfo>> {
        unimplemented!()
    }
    fn commit_diff(&self, _oid: &Oid) -> Result<CommitDiff> {
        Ok(self.0.clone())
    }
    fn commit_diff_for_fragmap(&self, _oid: &Oid) -> Result<CommitDiff> {
        unimplemented!()
    }
    fn staged_diff(&self) -> Option<CommitDiff> {
        None
    }
    fn unstaged_diff(&self) -> Option<CommitDiff> {
        None
    }
    fn split_commit_per_file(&self, _commit_oid: &Oid, _head_oid: &Oid) -> Result<()> {
        unimplemented!()
    }
    fn split_commit_per_hunk(&self, _commit_oid: &Oid, _head_oid: &Oid) -> Result<()> {
        unimplemented!()
    }
    fn split_commit_per_hunk_group(
        &self,
        _commit_oid: &Oid,
        _head_oid: &Oid,
        _reference_oid: &Oid,
    ) -> Result<()> {
        unimplemented!()
    }
    fn count_split_per_file(&self, _commit_oid: &Oid) -> Result<usize> {
        unimplemented!()
    }
    fn count_split_per_hunk(&self, _commit_oid: &Oid) -> Result<usize> {
        unimplemented!()
    }
    fn count_split_per_hunk_group(
        &self,
        _commit_oid: &Oid,
        _head_oid: &Oid,
        _reference_oid: &Oid,
    ) -> Result<usize> {
        unimplemented!()
    }
    fn reword_commit(&self, _commit_oid: &Oid, _new_message: &str, _head_oid: &Oid) -> Result<()> {
        unimplemented!()
    }
    fn get_config_string(&self, _key: &str) -> Option<String> {
        unimplemented!()
    }
    fn drop_commit(&self, _commit_oid: &Oid, _head_oid: &Oid) -> Result<RebaseOutcome> {
        unimplemented!()
    }
    fn move_commit(
        &self,
        _commit_oid: &Oid,
        _insert_after_oid: Option<&Oid>,
        _head_oid: &Oid,
    ) -> Result<RebaseOutcome> {
        unimplemented!()
    }
    fn rebase_continue(&self, _state: &ConflictState) -> Result<RebaseOutcome> {
        unimplemented!()
    }
    fn rebase_abort(&self, _state: &ConflictState) -> Result<()> {
        unimplemented!()
    }
    fn workdir(&self) -> Option<std::path::PathBuf> {
        unimplemented!()
    }
    fn read_index_stage(&self, _path: &str, _stage: i32) -> Result<Option<Vec<u8>>> {
        unimplemented!()
    }
    fn read_conflicting_files(&self) -> Vec<String> {
        unimplemented!()
    }
    fn squash_commits(
        &self,
        _source_oid: &Oid,
        _target_oid: &Oid,
        _message: &str,
        _head_oid: &Oid,
    ) -> Result<RebaseOutcome> {
        unimplemented!()
    }
    fn squash_try_combine(
        &self,
        _source_oid: &Oid,
        _target_oid: &Oid,
        _combined_message: &str,
        _is_fixup: bool,
        _head_oid: &Oid,
    ) -> Result<Option<ConflictState>> {
        unimplemented!()
    }
    fn squash_finalize(
        &self,
        _ctx: &SquashContext,
        _message: &str,
        _original_branch_oid: &Oid,
    ) -> Result<RebaseOutcome> {
        unimplemented!()
    }
    fn stage_file(&self, _path: &str) -> Result<()> {
        unimplemented!()
    }
    fn auto_stage_resolved_conflicts(&self, _files: &[String]) -> Result<()> {
        unimplemented!()
    }

    fn default_branch(&self) -> Option<String> {
        None
    }

    fn root_commit_oid(&self) -> Result<Oid> {
        unimplemented!()
    }
}

/// Short message: all content lines fit within 80 columns — no horizontal scrollbar.
#[test]
fn test_commit_detail_short_lines_no_hscroll() {
    let repo = NoOpRepo;
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    let mut app = AppState::new();
    app.commits = vec![common::create_test_commit("abc123def456", "Short commit")];
    app.selection_index = 0;

    terminal
        .draw(|frame| {
            let area = frame.area();
            views::commit_detail::render(&repo, frame, &mut app, area);
        })
        .unwrap();

    insta::assert_debug_snapshot!(terminal.backend().buffer().clone());
}

/// Long message line (100 chars) exceeds the 80-column terminal width, so the
/// horizontal scrollbar row must appear at the bottom of the content area.
#[test]
fn test_commit_detail_long_lines_hscroll_visible() {
    let repo = NoOpRepo;
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    let mut app = AppState::new();
    let long_message = "A".repeat(100);
    app.commits = vec![common::create_test_commit("abc123def456", &long_message)];
    app.selection_index = 0;

    terminal
        .draw(|frame| {
            let area = frame.area();
            views::commit_detail::render(&repo, frame, &mut app, area);
        })
        .unwrap();

    insta::assert_debug_snapshot!(terminal.backend().buffer().clone());
}

/// With a positive `detail_h_scroll_offset`, the paragraph is rendered
/// starting from a later column so the leading characters of long lines
/// are clipped out of view.
#[test]
fn test_commit_detail_hscroll_offset_clips_content() {
    let repo = NoOpRepo;
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    let mut app = AppState::new();
    let long_message = "A".repeat(100);
    app.commits = vec![common::create_test_commit("abc123def456", &long_message)];
    app.selection_index = 0;
    app.detail_h_scroll_offset = 10;

    terminal
        .draw(|frame| {
            let area = frame.area();
            views::commit_detail::render(&repo, frame, &mut app, area);
        })
        .unwrap();

    insta::assert_debug_snapshot!(terminal.backend().buffer().clone());
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
    let repo = FakeDiffRepo(diff);
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    let mut app = AppState::new();
    app.commits = vec![common::create_test_commit(
        "crlf001",
        "File with CRLF line endings",
    )];
    app.selection_index = 0;

    terminal
        .draw(|frame| {
            let area = frame.area();
            views::commit_detail::render(&repo, frame, &mut app, area);
        })
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    for cell in buffer.content() {
        assert!(
            !cell.symbol().contains('\r'),
            "carriage return found in rendered cell: {:?}",
            cell.symbol()
        );
    }
    insta::assert_debug_snapshot!(buffer);
}

// --- Unit tests for scroll_detail_left / scroll_detail_right ---

#[test]
fn test_scroll_detail_right_increments() {
    let mut app = AppState::new();
    app.max_detail_h_scroll = 20;
    app.detail_h_scroll_offset = 0;
    app.scroll_detail_right();
    assert_eq!(app.detail_h_scroll_offset, 1);
}

#[test]
fn test_scroll_detail_right_clamps_at_max() {
    let mut app = AppState::new();
    app.max_detail_h_scroll = 5;
    app.detail_h_scroll_offset = 5;
    app.scroll_detail_right();
    assert_eq!(app.detail_h_scroll_offset, 5);
}

#[test]
fn test_scroll_detail_left_decrements() {
    let mut app = AppState::new();
    app.detail_h_scroll_offset = 5;
    app.scroll_detail_left();
    assert_eq!(app.detail_h_scroll_offset, 4);
}

#[test]
fn test_scroll_detail_left_clamps_at_zero() {
    let mut app = AppState::new();
    app.detail_h_scroll_offset = 0;
    app.scroll_detail_left();
    assert_eq!(app.detail_h_scroll_offset, 0);
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
    let repo = FakeDiffRepo(diff);
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    let mut app = AppState::new();
    app.commits = vec![common::create_test_commit("abc123", "Add feature")];
    app.selection_index = 0;
    app.mode = AppMode::CommitDetail;

    // Activate search and type a query
    app.activate_search();
    app.search_query = "old".to_string();

    terminal
        .draw(|frame| {
            let area = frame.area();
            views::commit_detail::render(&repo, frame, &mut app, area);
        })
        .unwrap();

    insta::assert_debug_snapshot!(terminal.backend().buffer().clone());
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
    let repo = FakeDiffRepo(diff);
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    let mut app = AppState::new();
    app.commits = vec![common::create_test_commit("abc123", "Add feature")];
    app.selection_index = 0;
    app.mode = AppMode::CommitDetail;

    // Search for "hello" — should match 2 diff lines + file path
    app.activate_search();
    app.search_query = "hello".to_string();
    app.search_input_active = false; // Confirmed search

    terminal
        .draw(|frame| {
            let area = frame.area();
            views::commit_detail::render(&repo, frame, &mut app, area);
        })
        .unwrap();

    insta::assert_debug_snapshot!(terminal.backend().buffer().clone());
}

/// handle_search_event appends characters to the query.
#[test]
fn test_search_event_char_input() {
    let mut app = AppState::new();
    app.mode = AppMode::CommitDetail;
    app.activate_search();

    views::commit_detail::handle_search_event(make_key_event(KeyCode::Char('f')), &mut app);
    views::commit_detail::handle_search_event(make_key_event(KeyCode::Char('o')), &mut app);
    views::commit_detail::handle_search_event(make_key_event(KeyCode::Char('o')), &mut app);

    assert_eq!(app.search_query, "foo");
    assert!(app.search_input_active);
    assert!(app.search_active);
}

/// Backspace removes the last character from the search query.
#[test]
fn test_search_event_backspace() {
    let mut app = AppState::new();
    app.mode = AppMode::CommitDetail;
    app.activate_search();
    app.search_query = "foo".to_string();

    views::commit_detail::handle_search_event(make_key_event(KeyCode::Backspace), &mut app);

    assert_eq!(app.search_query, "fo");
}

/// Enter confirms the search (keeps active, stops input).
#[test]
fn test_search_event_enter_confirms() {
    let mut app = AppState::new();
    app.mode = AppMode::CommitDetail;
    app.activate_search();
    app.search_query = "foo".to_string();

    views::commit_detail::handle_search_event(make_key_event(KeyCode::Enter), &mut app);

    assert!(!app.search_input_active);
    assert!(app.search_active);
    assert_eq!(app.search_query, "foo");
}

/// Escape dismisses the search entirely.
#[test]
fn test_search_event_escape_dismisses() {
    let mut app = AppState::new();
    app.mode = AppMode::CommitDetail;
    app.activate_search();
    app.search_query = "foo".to_string();

    views::commit_detail::handle_search_event(make_key_event(KeyCode::Esc), &mut app);

    assert!(!app.search_input_active);
    assert!(!app.search_active);
    assert!(app.search_query.is_empty());
}

/// Quit (Esc) in confirmed-search mode clears search instead of leaving detail.
#[test]
fn test_quit_clears_search_before_leaving_detail() {
    let mut app = AppState::new();
    app.mode = AppMode::CommitDetail;
    app.search_active = true;
    app.search_query = "foo".to_string();

    let result = views::commit_detail::handle_key(KeyCommand::Quit, &mut app);

    assert!(matches!(result, AppAction::Handled));
    assert!(!app.search_active);
    // Still in CommitDetail — didn't leave
    assert!(matches!(app.mode, AppMode::CommitDetail));
}

/// n/N cycle through search matches, wrapping around.
#[test]
fn test_search_next_prev_wraps() {
    let mut app = AppState::new();
    app.mode = AppMode::CommitDetail;
    app.search_active = true;
    app.search_matches = vec![5, 10, 20];
    app.search_match_index = Some(0);
    app.detail_visible_height = 100; // large enough to avoid scrolling

    // Forward
    views::commit_detail::handle_key(KeyCommand::SearchNext, &mut app);
    assert_eq!(app.search_match_index, Some(1));

    views::commit_detail::handle_key(KeyCommand::SearchNext, &mut app);
    assert_eq!(app.search_match_index, Some(2));

    // Wrap forward
    views::commit_detail::handle_key(KeyCommand::SearchNext, &mut app);
    assert_eq!(app.search_match_index, Some(0));

    // Wrap backward
    views::commit_detail::handle_key(KeyCommand::SearchPrev, &mut app);
    assert_eq!(app.search_match_index, Some(2));
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
    let repo = FakeDiffRepo(diff);
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    let mut app = AppState::new();
    app.commits = vec![common::create_test_commit("abc123", "Add FOO feature")];
    app.selection_index = 0;
    app.mode = AppMode::CommitDetail;

    // "FOO" must only match uppercase occurrences, not "foo"
    app.activate_search();
    app.search_query = "FOO".to_string();
    app.search_input_active = false;

    terminal
        .draw(|frame| {
            let area = frame.area();
            views::commit_detail::render(&repo, frame, &mut app, area);
        })
        .unwrap();

    // "FOO" appears in the commit message and "+FOO bar" diff line.
    // "foo" in file path and "+foo baz" must NOT match.
    assert_eq!(app.search_matches.len(), 2);

    insta::assert_debug_snapshot!(terminal.backend().buffer().clone());
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
fn test_parse_key_search_only_in_detail() {
    let mode = AppMode::CommitList;

    let slash = Event::Key(KeyEvent {
        code: KeyCode::Char('/'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    });
    assert_eq!(mode.parse_key(slash), KeyCommand::None);

    let n = Event::Key(KeyEvent {
        code: KeyCode::Char('n'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    });
    assert_eq!(mode.parse_key(n), KeyCommand::None);
}
