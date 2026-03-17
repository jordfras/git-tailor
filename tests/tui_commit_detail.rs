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
    CommitDiff, CommitInfo, DeltaStatus, DiffLine, DiffLineKind, FileDiff, Hunk,
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
    fn head_oid(&self) -> Result<String> {
        unimplemented!()
    }
    fn find_reference_point(&self, _commit_ish: &str) -> Result<String> {
        unimplemented!()
    }
    fn list_commits(&self, _from: &str, _to: &str) -> Result<Vec<CommitInfo>> {
        unimplemented!()
    }
    fn commit_diff(&self, _oid: &str) -> Result<CommitDiff> {
        Err(anyhow!("no diff"))
    }
    fn commit_diff_for_fragmap(&self, _oid: &str) -> Result<CommitDiff> {
        unimplemented!()
    }
    fn staged_diff(&self) -> Option<CommitDiff> {
        None
    }
    fn unstaged_diff(&self) -> Option<CommitDiff> {
        None
    }
    fn split_commit_per_file(&self, _commit_oid: &str, _head_oid: &str) -> Result<()> {
        unimplemented!()
    }
    fn split_commit_per_hunk(&self, _commit_oid: &str, _head_oid: &str) -> Result<()> {
        unimplemented!()
    }
    fn split_commit_per_hunk_group(
        &self,
        _commit_oid: &str,
        _head_oid: &str,
        _reference_oid: &str,
    ) -> Result<()> {
        unimplemented!()
    }
    fn count_split_per_file(&self, _commit_oid: &str) -> Result<usize> {
        unimplemented!()
    }
    fn count_split_per_hunk(&self, _commit_oid: &str) -> Result<usize> {
        unimplemented!()
    }
    fn count_split_per_hunk_group(
        &self,
        _commit_oid: &str,
        _head_oid: &str,
        _reference_oid: &str,
    ) -> Result<usize> {
        unimplemented!()
    }
    fn reword_commit(&self, _commit_oid: &str, _new_message: &str, _head_oid: &str) -> Result<()> {
        unimplemented!()
    }
    fn get_config_string(&self, _key: &str) -> Option<String> {
        unimplemented!()
    }
    fn drop_commit(&self, _commit_oid: &str, _head_oid: &str) -> Result<RebaseOutcome> {
        unimplemented!()
    }
    fn move_commit(
        &self,
        _commit_oid: &str,
        _insert_after_oid: &str,
        _head_oid: &str,
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
        _source_oid: &str,
        _target_oid: &str,
        _message: &str,
        _head_oid: &str,
    ) -> Result<RebaseOutcome> {
        unimplemented!()
    }
    fn squash_try_combine(
        &self,
        _source_oid: &str,
        _target_oid: &str,
        _combined_message: &str,
        _head_oid: &str,
    ) -> Result<Option<ConflictState>> {
        unimplemented!()
    }
    fn squash_finalize(
        &self,
        _ctx: &SquashContext,
        _message: &str,
        _original_branch_oid: &str,
    ) -> Result<RebaseOutcome> {
        unimplemented!()
    }
    fn stage_file(&self, _path: &str) -> Result<()> {
        unimplemented!()
    }

    fn default_branch(&self) -> Option<String> {
        None
    }
}

/// GitRepo stub that returns a fixed CommitDiff for the selected commit OID.
struct FakeDiffRepo(CommitDiff);

impl GitRepo for FakeDiffRepo {
    fn head_oid(&self) -> Result<String> {
        unimplemented!()
    }
    fn find_reference_point(&self, _commit_ish: &str) -> Result<String> {
        unimplemented!()
    }
    fn list_commits(&self, _from: &str, _to: &str) -> Result<Vec<CommitInfo>> {
        unimplemented!()
    }
    fn commit_diff(&self, _oid: &str) -> Result<CommitDiff> {
        Ok(self.0.clone())
    }
    fn commit_diff_for_fragmap(&self, _oid: &str) -> Result<CommitDiff> {
        unimplemented!()
    }
    fn staged_diff(&self) -> Option<CommitDiff> {
        None
    }
    fn unstaged_diff(&self) -> Option<CommitDiff> {
        None
    }
    fn split_commit_per_file(&self, _commit_oid: &str, _head_oid: &str) -> Result<()> {
        unimplemented!()
    }
    fn split_commit_per_hunk(&self, _commit_oid: &str, _head_oid: &str) -> Result<()> {
        unimplemented!()
    }
    fn split_commit_per_hunk_group(
        &self,
        _commit_oid: &str,
        _head_oid: &str,
        _reference_oid: &str,
    ) -> Result<()> {
        unimplemented!()
    }
    fn count_split_per_file(&self, _commit_oid: &str) -> Result<usize> {
        unimplemented!()
    }
    fn count_split_per_hunk(&self, _commit_oid: &str) -> Result<usize> {
        unimplemented!()
    }
    fn count_split_per_hunk_group(
        &self,
        _commit_oid: &str,
        _head_oid: &str,
        _reference_oid: &str,
    ) -> Result<usize> {
        unimplemented!()
    }
    fn reword_commit(&self, _commit_oid: &str, _new_message: &str, _head_oid: &str) -> Result<()> {
        unimplemented!()
    }
    fn get_config_string(&self, _key: &str) -> Option<String> {
        unimplemented!()
    }
    fn drop_commit(&self, _commit_oid: &str, _head_oid: &str) -> Result<RebaseOutcome> {
        unimplemented!()
    }
    fn move_commit(
        &self,
        _commit_oid: &str,
        _insert_after_oid: &str,
        _head_oid: &str,
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
        _source_oid: &str,
        _target_oid: &str,
        _message: &str,
        _head_oid: &str,
    ) -> Result<RebaseOutcome> {
        unimplemented!()
    }
    fn squash_try_combine(
        &self,
        _source_oid: &str,
        _target_oid: &str,
        _combined_message: &str,
        _head_oid: &str,
    ) -> Result<Option<ConflictState>> {
        unimplemented!()
    }
    fn squash_finalize(
        &self,
        _ctx: &SquashContext,
        _message: &str,
        _original_branch_oid: &str,
    ) -> Result<RebaseOutcome> {
        unimplemented!()
    }
    fn stage_file(&self, _path: &str) -> Result<()> {
        unimplemented!()
    }

    fn default_branch(&self) -> Option<String> {
        None
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
