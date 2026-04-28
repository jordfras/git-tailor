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

#[allow(dead_code)]
mod common;

use git_tailor::fragmap::SquashableScope;
use git_tailor::static_views::fragmap::render;
use git_tailor::{CommitDiff, DeltaStatus, FileDiff, Hunk};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Render `commit_diffs` without colors and return the String.
fn plain(commit_diffs: &[git_tailor::CommitDiff]) -> String {
    render(
        commit_diffs,
        false,
        false,
        false,
        SquashableScope::Group,
        None,
    )
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

/// With `reverse = true`, rows are printed oldest-first, and connectors between
/// commits remain correct (squashable/conflicting classification is unchanged).
#[test]
fn test_reverse_flag_output_order() {
    // Three commits: 0=newest touches lines 1-5; 1=middle (unrelated); 2=oldest touches lines 1-5
    // Normal: newest first → row0=#, row1=connector, row2=#
    // Reverse: oldest first → row0=#, row1=connector, row2=#  (same data, flipped)
    let diffs = vec![
        common::create_test_commit_diff("aaaa00001111", "Newest commit", "src/lib.rs", (1, 5)),
        common::create_test_commit_diff("bbbb22223333", "Middle commit", "other.rs", (1, 3)),
        common::create_test_commit_diff("cccc44445555", "Oldest commit", "src/lib.rs", (1, 5)),
    ];

    let normal = render(&diffs, false, false, false, SquashableScope::Group, None);
    let reversed = render(&diffs, false, false, true, SquashableScope::Group, None);

    // Row order is flipped
    assert!(
        normal.lines().next().unwrap().contains("Newest commit"),
        "normal: first row should be newest"
    );
    assert!(
        reversed.lines().next().unwrap().contains("Oldest commit"),
        "reverse: first row should be oldest"
    );

    // The cluster columns reflect the same fragmap data, but the squashable
    // connector symbol flips: '^' points up toward target in normal order,
    // 'v' points down toward target in reverse order.
    let normal_cols: Vec<&str> = normal.lines().map(|l| &l[35..]).collect();
    let reversed_cols: Vec<&str> = reversed.lines().map(|l| &l[35..]).collect();
    let reversed_cols_normalized: Vec<String> =
        reversed_cols.iter().map(|s| s.replace('v', "^")).collect();
    assert_eq!(
        normal_cols,
        reversed_cols_normalized
            .iter()
            .rev()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        "cluster columns (with ^ normalized) should be mirror images of each other"
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
    let dedup = render(&diffs, false, false, false, SquashableScope::Group, None);
    let full = render(&diffs, true, false, false, SquashableScope::Group, None);
    // Both should be valid; with two overlapping identical hunks the cluster
    // count may differ depending on deduplication.
    insta::assert_snapshot!("full_flag_dedup", dedup);
    insta::assert_snapshot!("full_flag_nodedup", full);
}

/// `Group` scope shows a squashable connector when the A↔C pair in the shared
/// cluster has no interfering commit between them — even when other clusters
/// exist in the same row.  Passing `SquashableScope::Group` explicitly verifies
/// that the scope parameter is wired through to the renderer.
#[test]
fn test_squashable_scope_group_shows_squashable_connector() {
    let diffs = vec![
        common::create_test_commit_diff("aaaa00001111", "Touch A", "src/lib.rs", (1, 10)),
        common::create_test_commit_diff("bbbb22223333", "Unrelated", "other.rs", (1, 5)),
        common::create_test_commit_diff("cccc44445555", "Touch A again", "src/lib.rs", (5, 10)),
    ];

    let out = render(&diffs, false, false, false, SquashableScope::Group, None);

    let row_b = out.lines().nth(1).unwrap();
    assert!(
        row_b.contains('^'),
        "Group scope: B row should have squashable connector (^), got: {row_b:?}"
    );
}

/// When C touches the cluster shared with A **plus** an extra cluster that
/// no earlier commit touches, the scopes diverge:
/// - `Group` scope sees the A↔C pair in the shared cluster as squashable → `^`
/// - `Commit` scope requires C to be fully squashable (all clusters) → `|`
#[test]
fn test_squashable_scope_commit_stricter_than_group() {
    let c_diff = CommitDiff {
        commit: common::create_test_commit("cccc44445555", "Touch A and unique"),
        files: vec![
            FileDiff {
                old_path: Some("src/lib.rs".to_string()),
                new_path: Some("src/lib.rs".to_string()),
                status: DeltaStatus::Modified,
                hunks: vec![Hunk {
                    old_start: 5,
                    old_lines: 10,
                    new_start: 5,
                    new_lines: 10,
                    lines: vec![],
                }],
            },
            FileDiff {
                old_path: Some("unique.rs".to_string()),
                new_path: Some("unique.rs".to_string()),
                status: DeltaStatus::Modified,
                hunks: vec![Hunk {
                    old_start: 1,
                    old_lines: 5,
                    new_start: 1,
                    new_lines: 5,
                    lines: vec![],
                }],
            },
        ],
    };

    let diffs = vec![
        common::create_test_commit_diff("aaaa00001111", "Touch A", "src/lib.rs", (1, 10)),
        common::create_test_commit_diff("bbbb22223333", "Unrelated", "other.rs", (1, 5)),
        c_diff,
    ];

    let group_out = render(&diffs, false, false, false, SquashableScope::Group, None);
    let commit_out = render(&diffs, false, false, false, SquashableScope::Commit, None);

    let row_b_group = group_out.lines().nth(1).unwrap();
    let row_b_commit = commit_out.lines().nth(1).unwrap();

    assert!(
        row_b_group.contains('^'),
        "Group scope: B row should have squashable connector (^) — the A↔C pair in the shared cluster is squashable, got: {row_b_group:?}"
    );
    assert!(
        !row_b_commit.contains('^'),
        "Commit scope: B row should NOT have squashable connector (^) — C also touches unique.rs so it is not fully squashable, got: {row_b_commit:?}"
    );
    assert!(
        row_b_commit.contains('|'),
        "Commit scope: B row should have conflicting connector (|), got: {row_b_commit:?}"
    );
}

/// When a file is renamed between commits, overlapping spans should cluster
/// together — the rename should not break the relation.
#[test]
fn test_rename_clusters_with_original_file() {
    // Commit 0: adds lines 1-10 in "src/old.rs"
    let c0 = common::create_test_commit_diff("aaaa00001111", "Add old.rs", "src/old.rs", (1, 10));

    // Commit 1: renames "src/old.rs" → "src/new.rs" and modifies overlapping lines 5-12
    let c1 = CommitDiff {
        commit: common::create_test_commit("bbbb22223333", "Rename old to new"),
        files: vec![FileDiff {
            old_path: Some("src/old.rs".to_string()),
            new_path: Some("src/new.rs".to_string()),
            status: DeltaStatus::Modified,
            hunks: vec![Hunk {
                old_start: 5,
                old_lines: 6,
                new_start: 5,
                new_lines: 8,
                lines: vec![],
            }],
        }],
    };

    let output = plain(&[c0, c1]);
    insta::assert_snapshot!(output);

    // The rename map links "src/new.rs" back to "src/old.rs", so the SPG
    // processes both commits under the same file. The overlapping portion
    // (lines 5-10) forms a shared cluster; the non-overlapping part of c0
    // (lines 1-4) forms a second cluster touched only by c0.
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines.len(), 2, "expected exactly 2 rows");

    // c1 (rename commit) must touch at least one cluster, proving it was
    // grouped with c0's file instead of being isolated.
    let c1_matrix = &lines[1][35..].trim();
    assert!(
        c1_matrix.contains('#'),
        "rename commit should touch a shared cluster: {c1_matrix:?}"
    );
}
