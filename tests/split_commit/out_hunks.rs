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

use crate::common;
use crate::common::prelude::*;

#[test]
fn split_out_hunks_single_hunk_two_files() {
    let test = common::TestRepo::new();

    let base = test.commit_files(&[("a.txt", "alpha\n"), ("b.txt", "beta\n")], "base");
    let to_split = test.commit_files(
        &[("a.txt", "alpha2\n"), ("b.txt", "beta2\n")],
        "change both",
    );

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    // a.txt sorts before b.txt, so it's delta 0.
    git_repo
        .split_commit_out_hunks(&Oid::from(to_split), &[(0, 0)], &head_oid, 0)
        .unwrap();

    let commits = test.commits_from_head(base);
    assert_eq!(commits.len(), 2);
    let first = commits[0];
    let second = commits[1];

    assert_file_contents!(&test.repo, first, "a.txt", "alpha\n");
    assert_file_contents!(&test.repo, first, "b.txt", "beta2\n");
    assert_file_contents!(&test.repo, second, "a.txt", "alpha2\n");
    assert_file_contents!(&test.repo, second, "b.txt", "beta2\n");

    let msg1 = test.repo.find_commit(first).unwrap();
    assert_eq!(msg1.summary().unwrap().unwrap(), "change both");
    let msg2 = test.repo.find_commit(second).unwrap();
    assert_eq!(msg2.summary().unwrap().unwrap(), "change both (a.txt)");
}

#[test]
fn split_out_hunks_multiple_hunks_same_file() {
    let test = common::TestRepo::new();

    let base = test.commit_file(
        "a.txt",
        "line1\nline2\nline3\nPAD1\nPAD2\nPAD3\nPAD4\nPAD5\nline6\nline7\nline8\n",
        "base",
    );
    let to_split = test.commit_file(
        "a.txt",
        "LINE1\nline2\nline3\nPAD1\nPAD2\nPAD3\nPAD4\nPAD5\nLINE6\nline7\nline8\n",
        "two independent changes",
    );

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    // Select only the first hunk (the LINE1 change).
    git_repo
        .split_commit_out_hunks(&Oid::from(to_split), &[(0, 0)], &head_oid, 0)
        .unwrap();

    let commits = test.commits_from_head(base);
    assert_eq!(commits.len(), 2);
    let first = commits[0];
    let second = commits[1];

    // Rest commit: only the second hunk (LINE6) applied, first hunk untouched.
    assert_file_contents!(
        &test.repo,
        first,
        "a.txt",
        "line1\nline2\nline3\nPAD1\nPAD2\nPAD3\nPAD4\nPAD5\nLINE6\nline7\nline8\n"
    );
    // Peeled commit: full original content (both hunks applied).
    assert_file_contents!(
        &test.repo,
        second,
        "a.txt",
        "LINE1\nline2\nline3\nPAD1\nPAD2\nPAD3\nPAD4\nPAD5\nLINE6\nline7\nline8\n"
    );
}

#[test]
fn split_out_hunks_across_multiple_files_go_into_one_commit() {
    let test = common::TestRepo::new();

    let base = test.commit_files(
        &[("a.txt", "a\n"), ("b.txt", "b\n"), ("c.txt", "c\n")],
        "base",
    );
    let to_split = test.commit_files(
        &[("a.txt", "a2\n"), ("b.txt", "b2\n"), ("c.txt", "c2\n")],
        "change all three",
    );

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    // a.txt=0, b.txt=1, c.txt=2 (alphabetical). Select a.txt and c.txt's
    // hunks; b.txt's must stay behind in the rest commit.
    git_repo
        .split_commit_out_hunks(&Oid::from(to_split), &[(0, 0), (2, 0)], &head_oid, 0)
        .unwrap();

    let commits = test.commits_from_head(base);
    assert_eq!(commits.len(), 2, "both selected hunks land in one commit");
    let first = commits[0];
    let second = commits[1];

    assert_file_contents!(&test.repo, first, "a.txt", "a\n");
    assert_file_contents!(&test.repo, first, "b.txt", "b2\n");
    assert_file_contents!(&test.repo, first, "c.txt", "c\n");
    assert_file_contents!(&test.repo, second, "a.txt", "a2\n");
    assert_file_contents!(&test.repo, second, "b.txt", "b2\n");
    assert_file_contents!(&test.repo, second, "c.txt", "c2\n");

    let msg2 = test.repo.find_commit(second).unwrap();
    assert_eq!(
        msg2.summary().unwrap().unwrap(),
        "change all three (2 hunks across 2 files)"
    );
}

/// Regression test for the "rest tree" correctness trap: a delta with none of
/// its hunks selected must still be *fully applied* in the rest commit, not
/// silently left at its pre-commit (parent-tree) state.
#[test]
fn split_out_hunks_leaves_untouched_files_fully_applied_in_rest_commit() {
    let test = common::TestRepo::new();

    let base = test.commit_files(&[("a.txt", "a\n"), ("b.txt", "b\n")], "base");
    let to_split = test.commit_files(&[("a.txt", "a2\n"), ("b.txt", "b2\n")], "change both");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    // Only select a.txt's hunk; b.txt has none selected at all.
    git_repo
        .split_commit_out_hunks(&Oid::from(to_split), &[(0, 0)], &head_oid, 0)
        .unwrap();

    let commits = test.commits_from_head(base);
    let first = commits[0];

    // b.txt must be fully updated in the rest commit, not reverted to "b\n".
    assert_file_contents!(&test.repo, first, "b.txt", "b2\n");
}

#[test]
fn split_out_hunks_rebases_descendants() {
    let test = common::TestRepo::new();

    let base = test.commit_files(&[("a.txt", "a\n"), ("b.txt", "b\n")], "base");
    let to_split = test.commit_files(&[("a.txt", "a2\n"), ("b.txt", "b2\n")], "change both");
    test.commit_file("c.txt", "gamma\n", "add c");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    git_repo
        .split_commit_out_hunks(&Oid::from(to_split), &[(0, 0)], &head_oid, 0)
        .unwrap();

    let commits = test.commits_from_head(base);
    assert_eq!(commits.len(), 3, "two split commits + rebased descendant");

    let tip = *commits.last().unwrap();
    assert_file_contents!(&test.repo, tip, "a.txt", "a2\n");
    assert_file_contents!(&test.repo, tip, "b.txt", "b2\n");
    assert_file_contents!(&test.repo, tip, "c.txt", "gamma\n");
}

#[test]
fn split_out_hunks_refuses_empty_selection() {
    let test = common::TestRepo::new();
    test.commit_files(&[("a.txt", "a\n"), ("b.txt", "b\n")], "base");
    let to_split = test.commit_files(&[("a.txt", "a2\n"), ("b.txt", "b2\n")], "change both");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    let result = git_repo.split_commit_out_hunks(&Oid::from(to_split), &[], &head_oid, 0);
    assert!(result.is_err(), "should fail with no hunks selected");
}

#[test]
fn split_out_hunks_refuses_when_every_hunk_is_selected() {
    let test = common::TestRepo::new();
    test.commit_files(&[("a.txt", "a\n"), ("b.txt", "b\n")], "base");
    let to_split = test.commit_files(&[("a.txt", "a2\n"), ("b.txt", "b2\n")], "change both");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    let result =
        git_repo.split_commit_out_hunks(&Oid::from(to_split), &[(0, 0), (1, 0)], &head_oid, 0);
    assert!(
        result.is_err(),
        "should fail when the selection covers every hunk"
    );
}

#[test]
fn split_out_hunks_refuses_invalid_hunk_index() {
    let test = common::TestRepo::new();
    test.commit_file("a.txt", "a\n", "base");
    let to_split = test.commit_file("a.txt", "a2\n", "change a");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    let result = git_repo.split_commit_out_hunks(&Oid::from(to_split), &[(0, 5)], &head_oid, 0);
    assert!(result.is_err(), "should fail for an out-of-range hunk_idx");

    let result = git_repo.split_commit_out_hunks(&Oid::from(to_split), &[(9, 0)], &head_oid, 0);
    assert!(result.is_err(), "should fail for an out-of-range delta_idx");
}

/// At non-zero context, two nearby single-line changes merge into one hunk
/// that git wouldn't show as separate at context 0. Selecting that merged
/// hunk must split out *both* underlying changes together, and the "rest"
/// commit's untouched context lines must come through unmodified — not
/// duplicated or dropped by `HunkSelection::Whole`'s splice, which replaces
/// the hunk's old-side range with all of its ' '/'+' lines (context included).
#[test]
fn split_out_hunks_at_nonzero_context_merges_and_splits_nearby_changes_together() {
    let test = common::TestRepo::new();

    let lines: Vec<&str> = vec![
        "l1", "l2", "l3", "l4", "l5", "l6", "l7", "l8", "l9", "l10", "l11", "l12", "l13", "l14",
        "l15", "l16", "l17", "l18", "l19", "l20",
    ];
    let base_content = format!("{}\n", lines.join("\n"));
    let base = test.commit_file("f.txt", &base_content, "base");

    // l2 and l6 are close enough to merge into one hunk at context 3 (3 lines
    // between them); l16 is far enough away to stay a separate hunk.
    let mut changed = lines.clone();
    changed[1] = "L2";
    changed[5] = "L6";
    changed[15] = "L16";
    let changed_content = format!("{}\n", changed.join("\n"));
    let to_split = test.commit_file("f.txt", &changed_content, "three small changes");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    // Confirm the fixture actually merges as intended before relying on it.
    let mut diff_opts = git2::DiffOptions::new();
    diff_opts.context_lines(3);
    let full_diff = test
        .repo
        .diff_tree_to_tree(
            Some(&test.repo.find_commit(base).unwrap().tree().unwrap()),
            Some(&test.repo.find_commit(to_split).unwrap().tree().unwrap()),
            Some(&mut diff_opts),
        )
        .unwrap();
    assert_eq!(
        git2::Patch::from_diff(&full_diff, 0)
            .unwrap()
            .unwrap()
            .num_hunks(),
        2,
        "fixture must produce exactly 2 hunks at context 3 (merged l2+l6, separate l16)"
    );

    // Hunk 0 is the merged l2+l6 change; hunk 1 is the separate l16 change.
    git_repo
        .split_commit_out_hunks(&Oid::from(to_split), &[(0, 0)], &head_oid, 3)
        .unwrap();

    let commits = test.commits_from_head(base);
    assert_eq!(commits.len(), 2);
    let rest = commits[0];
    let peeled = commits[1];

    // Rest commit: only l16 changed; l2/l6 and every other line untouched.
    let mut rest_expected = lines.clone();
    rest_expected[15] = "L16";
    let rest_content = format!("{}\n", rest_expected.join("\n"));
    assert_file_contents!(&test.repo, rest, "f.txt", rest_content.as_str());

    // Peeled commit: the full original result (all three changes applied).
    assert_file_contents!(&test.repo, peeled, "f.txt", changed_content.as_str());
}

#[test]
fn split_out_hunks_refuses_dirty_overlap() {
    let test = common::TestRepo::new();
    test.commit_files(&[("a.txt", "a\n"), ("b.txt", "b\n")], "base");
    let to_split = test.commit_files(&[("a.txt", "a2\n"), ("b.txt", "b2\n")], "change both");

    test.write_file("a.txt", "DIRTY\n");
    test.stage_file("a.txt");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    let result = git_repo.split_commit_out_hunks(&Oid::from(to_split), &[(0, 0)], &head_oid, 0);
    assert!(result.is_err(), "should fail when staged changes overlap");
}
