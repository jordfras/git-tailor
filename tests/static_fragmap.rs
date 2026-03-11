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

// Snapshot tests for the static (non-TUI) fragmap renderer.
//
// All tests use `colors: false` so the output is plain Unicode without ANSI
// escape codes, making snapshots readable and stable.

mod common;

use git_tailor::static_views::fragmap::render;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Render `commit_diffs` without colors and return the String.
fn plain(commit_diffs: &[git_tailor::CommitDiff]) -> String {
    render(commit_diffs, false, false)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn test_no_commits() {
    let output = plain(&[]);
    assert_eq!(output, "");
}

/// A commit whose diff has no hunks produces a row with the SHA and title but
/// no cluster columns.
#[test]
fn test_commit_with_no_hunks() {
    let diff = git_tailor::CommitDiff {
        commit: common::create_test_commit("aabbccdd1122", "No changes"),
        files: vec![],
    };
    let output = plain(&[diff]);
    // SHA (8 chars), space, title padded to 26, newline — no cluster columns
    let expected = "aabbccdd No changes                \n";
    assert_eq!(output, expected);
}

/// One commit touching one cluster → a single `█` cell on that row.
#[test]
fn test_single_commit_single_touch() {
    let diffs = vec![common::create_test_commit_diff(
        "aabbccdd1122",
        "Add feature",
        "src/main.rs",
        (1, 10),
    )];
    let output = plain(&diffs);
    insta::assert_snapshot!(output);
}

/// Two commits both directly touching the same cluster (overlapping hunks in
/// the same file) → both rows show `█`, no connector needed.
#[test]
fn test_two_adjacent_touches_no_connector() {
    let diffs = vec![
        common::create_test_commit_diff("aabbccdd1122", "First", "src/lib.rs", (1, 10)),
        common::create_test_commit_diff("eeff00001122", "Second", "src/lib.rs", (5, 15)),
    ];
    let output = plain(&diffs);
    insta::assert_snapshot!(output);
}

/// Commits 0 and 2 touch the same cluster; commit 1 is in between without
/// touching it.  Commit 1 should show a squashable connector `│` for that
/// column (no interfering commits between 0 and 2).
#[test]
fn test_squashable_connector() {
    let diffs = vec![
        common::create_test_commit_diff("aaaa00001111", "Touch A", "src/lib.rs", (1, 10)),
        common::create_test_commit_diff("bbbb22223333", "Unrelated", "other.rs", (1, 5)),
        common::create_test_commit_diff("cccc44445555", "Touch A again", "src/lib.rs", (5, 12)),
    ];
    let output = plain(&diffs);
    insta::assert_snapshot!(output);
}

/// Commits 0, 1, and 3 all touch the same cluster; commit 2 is a connector
/// row.  Because commit 1 sits between the "earliest" (0) and commit 3, the
/// connector on row 2 is conflicting `│`. Both `│` symbols are the same glyph
/// in plain mode; the test verifies the connector is present (not `.`).
#[test]
fn test_conflicting_connector() {
    let diffs = vec![
        common::create_test_commit_diff("aaaa00001111", "Touch 1", "src/lib.rs", (1, 10)),
        common::create_test_commit_diff("bbbb22223333", "Touch 2", "src/lib.rs", (5, 12)),
        common::create_test_commit_diff("cccc44445555", "Unrelated", "other.rs", (1, 5)),
        common::create_test_commit_diff("dddd66667777", "Touch 3", "src/lib.rs", (8, 15)),
    ];
    let output = plain(&diffs);
    insta::assert_snapshot!(output);

    // The connector row (index 2) should have a connector, not a dot, for
    // the first cluster column.
    let row2 = output.lines().nth(2).unwrap();
    // After SHA (8) + space + title (26) there should be a `│` connector.
    let matrix_part = &row2[8 + 1 + 26..];
    assert!(
        matrix_part.contains('|'),
        "expected conflicting connector | in: {matrix_part:?}"
    );
}

/// A synthetic row (staged/unstaged changes with a special OID) is included
/// in the output just like a regular commit row.
#[test]
fn test_synthetic_staged_row_included() {
    let diffs = vec![
        common::create_test_commit_diff("aabbccdd1122", "Real commit", "src/lib.rs", (1, 10)),
        git_tailor::CommitDiff {
            commit: common::create_test_commit("00000000", "(staged changes)"),
            ..common::create_test_commit_diff("00000000", "(staged changes)", "src/lib.rs", (5, 8))
        },
    ];
    let output = plain(&diffs);
    insta::assert_snapshot!(output);

    assert!(
        output.contains("00000000"),
        "staged row OID should appear in output"
    );
}

/// With `full = true`, identical cluster columns are NOT merged, so the row
/// may have more columns than with the default deduplication.
#[test]
fn test_full_flag_no_dedup() {
    let diffs = vec![
        common::create_test_commit_diff("aaaa00001111", "Commit A", "src/lib.rs", (1, 5)),
        common::create_test_commit_diff("bbbb22223333", "Commit B", "src/lib.rs", (1, 5)),
    ];
    let dedup = render(&diffs, false, false);
    let full = render(&diffs, true, false);
    // Both should be valid; with two overlapping identical hunks the cluster
    // count may differ depending on deduplication.
    insta::assert_snapshot!("full_flag_dedup", dedup);
    insta::assert_snapshot!("full_flag_nodedup", full);
}
