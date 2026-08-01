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

#[allow(dead_code)]
mod common;

use git_tailor::{app::AppState, fragmap::TouchKind, views};

use common::{StubRepoBuilder, TuiTestHarness, create_fragmap, simple_cluster};
use ratatui::buffer::Buffer;

fn make_repo_with_empty_diff() -> common::StubRepo {
    let diff = git_tailor::CommitDiff {
        commit: common::create_test_commit("abc123def456", "Initial commit"),
        files: vec![],
    };
    StubRepoBuilder::new().with_commit_diff(diff).build()
}

fn app_with_commits() -> AppState {
    let mut app = AppState::new();
    let oids = ["abc123def456", "def456ghi789", "ghi789jkl012"];
    app.list.commits = vec![
        common::create_test_commit(oids[0], "Initial commit"),
        common::create_test_commit(oids[1], "Add feature X"),
        common::create_test_commit(oids[2], "Fix bug in parser"),
    ];
    app.list.selection_index = 0;
    // One visible cluster so compute_fragmap_sep_x takes the fragmap path
    // and the separator aligns with the fragmap │ column.
    app.fragmap = Some(create_fragmap(
        oids.to_vec(),
        vec![simple_cluster("a.rs", 1, 5, &[])],
        vec![
            vec![TouchKind::Modified],
            vec![TouchKind::None],
            vec![TouchKind::None],
        ],
    ));
    app
}

// BASE_SPLIT_X from main_view.rs; separator cell is at column (BASE_SPLIT_X + offset - 1).
const BASE_SPLIT_X: i32 = 72;

fn separator_column(offset: i16) -> usize {
    (BASE_SPLIT_X + offset as i32 - 1) as usize
}

/// Return the character at the given (col, row) in a rendered buffer.
fn cell_at(buf: &Buffer, col: usize, row: usize) -> String {
    buf.cell((col as u16, row as u16))
        .map(|c| c.symbol().to_string())
        .unwrap_or_default()
}

/// Default separator position (offset = 0) — separator bar at column 71.
#[test]
fn test_separator_default_position() {
    let repo = make_repo_with_empty_diff();
    let mut harness = TuiTestHarness::wide();
    let mut app = app_with_commits();
    // offset = 0 is the default

    let buf = harness.render(|frame| views::main_view::render(&repo, &mut app, frame));

    // Separator should appear at column BASE_SPLIT_X - 1 = 71.
    let sep_col = separator_column(0);
    assert_eq!(
        cell_at(&buf, sep_col, 0),
        "│",
        "separator should be at column {sep_col} with offset 0"
    );
    // Row 1 is the selected commit (selection_index = 0). The selection
    // highlight must not bleed into the separator column.
    assert_eq!(
        cell_at(&buf, sep_col, 1),
        "│",
        "separator on selected data row should be '│', not overwritten by row highlight"
    );
    // Footer row (last row) must span the full terminal width — not capped to
    // the left panel. The footer text starts with a space then the OID, so
    // column 1 should be 'a' (first char of "abc123def456"). A cell in the
    // right half (sep_col+10) should also be non-null (blue background extends
    // past the separator).
    let last_row = 19usize;
    let footer_oid_start = cell_at(&buf, 1, last_row);
    assert_eq!(
        footer_oid_start, "a",
        "footer should start with the commit OID at col 1"
    );
    // A cell well past the separator should be a space (blue bg fill), not the
    // null character that appears for completely-unpainted cells.
    let footer_far_right = cell_at(&buf, sep_col + 10, last_row);
    assert_ne!(
        footer_far_right, "\u{0}",
        "footer should extend past the separator column into the right half"
    );
    insta::assert_debug_snapshot!(buf);
}

/// Separator moved left by 16 columns (offset = -16) — bar at column 55.
#[test]
fn test_separator_shifted_left() {
    let repo = make_repo_with_empty_diff();
    let mut harness = TuiTestHarness::wide();
    let mut app = app_with_commits();
    app.separator_offset = -16;

    let buf = harness.render(|frame| views::main_view::render(&repo, &mut app, frame));

    let sep_col = separator_column(-16);
    assert_eq!(
        cell_at(&buf, sep_col, 0),
        "│",
        "separator should be at column {sep_col} with offset -16"
    );
    insta::assert_debug_snapshot!(buf);
}

/// Separator moved right by 8 columns (offset = +8) — bar at column 79.
#[test]
fn test_separator_shifted_right() {
    let repo = make_repo_with_empty_diff();
    let mut harness = TuiTestHarness::wide();
    let mut app = app_with_commits();
    app.separator_offset = 8;

    let buf = harness.render(|frame| views::main_view::render(&repo, &mut app, frame));

    let sep_col = separator_column(8);
    assert_eq!(
        cell_at(&buf, sep_col, 0),
        "│",
        "separator should be at column {sep_col} with offset 8"
    );
    insta::assert_debug_snapshot!(buf);
}

/// Extreme negative offset is clamped — separator never goes below MIN_LEFT (23).
/// MIN_LEFT=23 means separator_x=23, so separator cell is at column 22.
/// The title column is squeezed to its minimum (10), which is less than the
/// 14-character "Initial commit" summary — titles are truncated.
#[test]
fn test_separator_clamps_left() {
    let repo = make_repo_with_empty_diff();
    let mut harness = TuiTestHarness::wide();
    let mut app = app_with_commits();
    app.separator_offset = -9999;

    let buf = harness.render(|frame| views::main_view::render(&repo, &mut app, frame));

    // After clamping, separator_offset is written back by render.
    // min_title=10 → sep_x = SHA_COL_WIDTH + gap + min_title = 10+1+10 = 21 → sep cell at 21.
    let clamped_col = 21usize;
    assert_eq!(
        cell_at(&buf, clamped_col, 0),
        "│",
        "separator should be clamped to column {clamped_col}"
    );

    // At clamped minimum, title column width is 10. "Initial commit" is 14
    // characters so it overflows — verify the full string is NOT present.
    // (Ratatui clips the cell; the rightmost visible column of the title area
    // is column 20 = SHA(10) + gap(1) + title_end(10) - 1.)
    let first_row_as_text: String = (0..clamped_col).map(|col| cell_at(&buf, col, 1)).collect();
    assert!(
        !first_row_as_text.contains("Initial commit"),
        "'Initial commit' (14 chars) should be truncated when title column is 10 wide; row text: {first_row_as_text:?}"
    );

    insta::assert_debug_snapshot!(buf);
}

/// Extreme positive offset is clamped — separator goes as far right as the
/// fragmap layout allows: max_title = effective_width - SHA - 2 - 1 = 107,
/// so sep_x = SHA + 1 + max_title = 118, separator cell at column 118.
#[test]
fn test_separator_clamps_right() {
    let repo = make_repo_with_empty_diff();
    let mut harness = TuiTestHarness::wide();
    let mut app = app_with_commits();
    app.separator_offset = 9999;

    let buf = harness.render(|frame| views::main_view::render(&repo, &mut app, frame));

    // max_title = 120 - SHA(10) - col-gaps(2) - min-fragmap(1) = 107
    // sep_x = SHA(10) + gap(1) + max_title(107) = 118
    let clamped_col = 118usize;
    assert_eq!(
        cell_at(&buf, clamped_col, 0),
        "│",
        "separator should be clamped to column {clamped_col}"
    );
}

/// Moderate left shift (offset = -32) squeezes the left panel to ~40 columns,
/// giving a title width of ~14. A long commit summary is visually cut off.
#[test]
fn test_separator_title_truncation_boundary() {
    let repo = make_repo_with_empty_diff();
    let mut harness = TuiTestHarness::wide();
    let mut app = app_with_commits();
    // Use a commit whose summary is clearly longer than the squeezed column.
    app.list.commits[0].summary = "A commit with a very long title that will be cut".to_string();
    app.separator_offset = -32; // squeezes left panel to ~40 cols, title to ~14

    let buf = harness.render(|frame| views::main_view::render(&repo, &mut app, frame));

    // Collect the first data row (row 1, after the header).
    let left_panel_end = separator_column(-32);
    let first_row_text: String = (0..left_panel_end)
        .map(|col| cell_at(&buf, col, 1))
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

    insta::assert_debug_snapshot!(buf);
}

/// When the terminal is narrower than BASE_SPLIT_X (72 cols), right_width == 0.
/// In CommitDetail mode the commit detail view should be rendered fullscreen,
/// not the commit list. Regression test for T168.
#[test]
fn test_commit_detail_shown_on_narrow_terminal() {
    let repo = make_repo_with_empty_diff();
    let mut harness = TuiTestHarness::narrow();
    let mut app = app_with_commits();
    app.mode = git_tailor::app::AppMode::CommitDetail;

    let buf = harness.render(|frame| views::main_view::render(&repo, &mut app, frame));

    // With a 60-col terminal: sep_x = SHA(10) + gap(1) + title(47) = 58,
    // so the right panel is 1 col wide at column 59. The commit detail header
    // "Commit information" is truncated to a single 'C', proving that
    // commit_detail::render was called rather than falling back to commit_list.
    let right_panel_char = cell_at(&buf, 59, 0);
    assert_eq!(
        right_panel_char, "C",
        "narrow terminal in CommitDetail mode: right panel (col 59) should show first char of commit detail header; got: {right_panel_char:?}"
    );
}
