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

// TUI snapshot tests for the main split view (commit list + commit detail).
// These tests exercise separator bar positioning via `separator_offset`.

mod common;

use anyhow::{Result, anyhow};
use git_tailor::{
    CommitDiff, CommitInfo, Oid, VirtualOid,
    app::AppState,
    fragmap::{FileSpan, FragMap, SpanCluster, TouchKind},
    repo::{ConflictState, GitRepo, RebaseOutcome, SquashContext},
    views,
};
use ratatui::{Terminal, backend::TestBackend};

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

fn app_with_commits() -> AppState {
    let mut app = AppState::new();
    app.commits = vec![
        common::create_test_commit("abc123def456", "Initial commit"),
        common::create_test_commit("def456ghi789", "Add feature X"),
        common::create_test_commit("ghi789jkl012", "Fix bug in parser"),
    ];
    app.selection_index = 0;
    // One visible cluster so compute_fragmap_sep_x takes the fragmap path
    // and the separator aligns with the fragmap │ column.
    app.fragmap = Some(FragMap {
        commits: vec![VirtualOid::Real(Oid::new("abc123def456".to_string())); 3],
        clusters: vec![SpanCluster {
            spans: vec![FileSpan {
                path: "a.rs".to_string(),
                start_line: 1,
                end_line: 5,
            }],
            commit_oids: vec![],
        }],
        matrix: {
            let mut m = vec![vec![TouchKind::None; 1]; 3];
            m[0][0] = TouchKind::Modified;
            m
        },
    });
    app
}

// BASE_SPLIT_X from main_view.rs; separator cell is at column (BASE_SPLIT_X + offset - 1).
const BASE_SPLIT_X: i32 = 72;

fn separator_column(offset: i16) -> usize {
    (BASE_SPLIT_X + offset as i32 - 1) as usize
}

/// Render main_view and return the character at the given (col, row).
fn cell_at(terminal: &Terminal<TestBackend>, col: usize, row: usize) -> String {
    let buf = terminal.backend().buffer();
    buf.cell((col as u16, row as u16))
        .map(|c| c.symbol().to_string())
        .unwrap_or_default()
}

/// Default separator position (offset = 0) — separator bar at column 71.
#[test]
fn test_separator_default_position() {
    let repo = NoOpRepo;
    let backend = TestBackend::new(120, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut app = app_with_commits();
    // offset = 0 is the default

    terminal
        .draw(|frame| views::main_view::render(&repo, &mut app, frame))
        .unwrap();

    // Separator should appear at column BASE_SPLIT_X - 1 = 71.
    let sep_col = separator_column(0);
    assert_eq!(
        cell_at(&terminal, sep_col, 0),
        "│",
        "separator should be at column {sep_col} with offset 0"
    );
    // Row 1 is the selected commit (selection_index = 0). The selection
    // highlight must not bleed into the separator column.
    assert_eq!(
        cell_at(&terminal, sep_col, 1),
        "│",
        "separator on selected data row should be '│', not overwritten by row highlight"
    );
    // Footer row (last row) must span the full terminal width — not capped to
    // the left panel. The footer text starts with a space then the OID, so
    // column 1 should be 'a' (first char of "abc123def456"). A cell in the
    // right half (sep_col+10) should also be non-null (blue background extends
    // past the separator).
    let last_row = 19usize;
    let footer_oid_start = cell_at(&terminal, 1, last_row);
    assert_eq!(
        footer_oid_start, "a",
        "footer should start with the commit OID at col 1"
    );
    // A cell well past the separator should be a space (blue bg fill), not the
    // null character that appears for completely-unpainted cells.
    let footer_far_right = cell_at(&terminal, sep_col + 10, last_row);
    assert_ne!(
        footer_far_right, "\u{0}",
        "footer should extend past the separator column into the right half"
    );
    insta::assert_debug_snapshot!(terminal.backend().buffer().clone());
}

/// Separator moved left by 16 columns (offset = -16) — bar at column 55.
#[test]
fn test_separator_shifted_left() {
    let repo = NoOpRepo;
    let backend = TestBackend::new(120, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut app = app_with_commits();
    app.separator_offset = -16;

    terminal
        .draw(|frame| views::main_view::render(&repo, &mut app, frame))
        .unwrap();

    let sep_col = separator_column(-16);
    assert_eq!(
        cell_at(&terminal, sep_col, 0),
        "│",
        "separator should be at column {sep_col} with offset -16"
    );
    insta::assert_debug_snapshot!(terminal.backend().buffer().clone());
}

/// Separator moved right by 8 columns (offset = +8) — bar at column 79.
#[test]
fn test_separator_shifted_right() {
    let repo = NoOpRepo;
    let backend = TestBackend::new(120, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut app = app_with_commits();
    app.separator_offset = 8;

    terminal
        .draw(|frame| views::main_view::render(&repo, &mut app, frame))
        .unwrap();

    let sep_col = separator_column(8);
    assert_eq!(
        cell_at(&terminal, sep_col, 0),
        "│",
        "separator should be at column {sep_col} with offset 8"
    );
    insta::assert_debug_snapshot!(terminal.backend().buffer().clone());
}

/// Extreme negative offset is clamped — separator never goes below MIN_LEFT (23).
/// MIN_LEFT=23 means separator_x=23, so separator cell is at column 22.
/// The title column is squeezed to its minimum (10), which is less than the
/// 14-character "Initial commit" summary — titles are truncated.
#[test]
fn test_separator_clamps_left() {
    let repo = NoOpRepo;
    let backend = TestBackend::new(120, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut app = app_with_commits();
    app.separator_offset = -9999;

    terminal
        .draw(|frame| views::main_view::render(&repo, &mut app, frame))
        .unwrap();

    // After clamping, separator_offset is written back by render.
    // min_title=10 → sep_x = SHA_COL_WIDTH + gap + min_title = 10+1+10 = 21 → sep cell at 21.
    let clamped_col = 21usize;
    assert_eq!(
        cell_at(&terminal, clamped_col, 0),
        "│",
        "separator should be clamped to column {clamped_col}"
    );

    // At clamped minimum, title column width is 10. "Initial commit" is 14
    // characters so it overflows — verify the full string is NOT present.
    // (Ratatui clips the cell; the rightmost visible column of the title area
    // is column 20 = SHA(10) + gap(1) + title_end(10) - 1.)
    let first_row_as_text: String = (0..clamped_col)
        .map(|col| cell_at(&terminal, col, 1))
        .collect();
    assert!(
        !first_row_as_text.contains("Initial commit"),
        "'Initial commit' (14 chars) should be truncated when title column is 10 wide; row text: {first_row_as_text:?}"
    );

    insta::assert_debug_snapshot!(terminal.backend().buffer().clone());
}

/// Extreme positive offset is clamped — separator goes as far right as the
/// fragmap layout allows: max_title = effective_width - SHA - 2 - 1 = 107,
/// so sep_x = SHA + 1 + max_title = 118, separator cell at column 118.
#[test]
fn test_separator_clamps_right() {
    let repo = NoOpRepo;
    let backend = TestBackend::new(120, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut app = app_with_commits();
    app.separator_offset = 9999;

    terminal
        .draw(|frame| views::main_view::render(&repo, &mut app, frame))
        .unwrap();

    // max_title = 120 - SHA(10) - col-gaps(2) - min-fragmap(1) = 107
    // sep_x = SHA(10) + gap(1) + max_title(107) = 118
    let clamped_col = 118usize;
    assert_eq!(
        cell_at(&terminal, clamped_col, 0),
        "│",
        "separator should be clamped to column {clamped_col}"
    );
}

/// Moderate left shift (offset = -32) squeezes the left panel to ~40 columns,
/// giving a title width of ~14. A long commit summary is visually cut off.
#[test]
fn test_separator_title_truncation_boundary() {
    let repo = NoOpRepo;
    let backend = TestBackend::new(120, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut app = app_with_commits();
    // Use a commit whose summary is clearly longer than the squeezed column.
    app.commits[0].summary = "A commit with a very long title that will be cut".to_string();
    app.separator_offset = -32; // squeezes left panel to ~40 cols, title to ~14

    terminal
        .draw(|frame| views::main_view::render(&repo, &mut app, frame))
        .unwrap();

    // Collect the first data row (row 1, after the header).
    let left_panel_end = separator_column(-32);
    let first_row_text: String = (0..left_panel_end)
        .map(|col| cell_at(&terminal, col, 1))
        .collect();

    // The full summary should not appear in the left panel — it is truncated.
    assert!(
        !first_row_text.contains("A commit with a very long title"),
        "long summary should be truncated in narrow title column; row: {first_row_text:?}"
    );
    // But the SHA prefix should still be visible.
    assert!(
        first_row_text.contains("abc123de"),
        "SHA should still be visible; row: {first_row_text:?}"
    );

    insta::assert_debug_snapshot!(terminal.backend().buffer().clone());
}

/// When the terminal is narrower than BASE_SPLIT_X (72 cols), right_width == 0.
/// In CommitDetail mode the commit detail view should be rendered fullscreen,
/// not the commit list. Regression test for T168.
#[test]
fn test_commit_detail_shown_on_narrow_terminal() {
    let repo = NoOpRepo;
    let backend = TestBackend::new(60, 10);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut app = app_with_commits();
    app.mode = git_tailor::app::AppMode::CommitDetail;

    terminal
        .draw(|frame| views::main_view::render(&repo, &mut app, frame))
        .unwrap();

    // With a 60-col terminal: sep_x = SHA(10) + gap(1) + title(47) = 58,
    // so the right panel is 1 col wide at column 59. The commit detail header
    // "Commit information" is truncated to a single 'C', proving that
    // commit_detail::render was called rather than falling back to commit_list.
    let right_panel_char = cell_at(&terminal, 59, 0);
    assert_eq!(
        right_panel_char, "C",
        "narrow terminal in CommitDetail mode: right panel (col 59) should show first char of commit detail header; got: {right_panel_char:?}"
    );
}
