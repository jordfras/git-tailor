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
fn split_per_hunk_single_file_two_hunks() {
    let test = common::TestRepo::new();

    // Base commit: file with two separate regions.
    let base = test.commit_file(
        "a.txt",
        "line1\nline2\nline3\nPAD1\nPAD2\nPAD3\nPAD4\nPAD5\nline6\nline7\nline8\n",
        "base",
    );

    // Commit that changes line1 AND line6 — produces two separate hunks
    // (with 0 context and enough padding between them).
    let to_split = test.commit_file(
        "a.txt",
        "LINE1\nline2\nline3\nPAD1\nPAD2\nPAD3\nPAD4\nPAD5\nLINE6\nline7\nline8\n",
        "two independent changes",
    );

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    git_repo
        .split_commit_per_hunk(&Oid::from(to_split), &head_oid)
        .unwrap();

    // Should now have 2 commits above base
    let commits_above_base = test.commits_from_head(base);
    assert_eq!(
        commits_above_base.len(),
        2,
        "expected 2 split commits above base"
    );

    let split1 = test.repo.find_commit(commits_above_base[0]).unwrap();
    let split2 = test.repo.find_commit(commits_above_base[1]).unwrap();

    assert!(
        split1
            .summary()
            .ok()
            .flatten()
            .unwrap_or("")
            .contains("(1/2)"),
        "expected (1/2) in first split commit, got: {}",
        split1.summary().ok().flatten().unwrap_or("")
    );
    assert!(
        split2
            .summary()
            .ok()
            .flatten()
            .unwrap_or("")
            .contains("(2/2)"),
        "expected (2/2) in second split commit, got: {}",
        split2.summary().ok().flatten().unwrap_or("")
    );

    // Final content is intact at HEAD
    let tip = commits_above_base[1];
    assert_file_contents!(
        &test.repo,
        tip,
        "a.txt",
        "LINE1\nline2\nline3\nPAD1\nPAD2\nPAD3\nPAD4\nPAD5\nLINE6\nline7\nline8\n"
    );
}

#[test]
fn split_per_hunk_two_files_one_hunk_each() {
    let test = common::TestRepo::new();

    let base = test.commit_files(&[("a.txt", "alpha\n"), ("b.txt", "beta\n")], "base");

    let to_split = test.commit_files(
        &[("a.txt", "alpha2\n"), ("b.txt", "beta2\n")],
        "change both",
    );

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    git_repo
        .split_commit_per_hunk(&Oid::from(to_split), &head_oid)
        .unwrap();

    let commits_above_base = test.commits_from_head(base);
    assert_eq!(
        commits_above_base.len(),
        2,
        "expected 2 split commits (one per file's hunk)"
    );

    // The final tip contains both file changes
    let tip = *commits_above_base.last().unwrap();
    assert_file_contents!(&test.repo, tip, "a.txt", "alpha2\n");
    assert_file_contents!(&test.repo, tip, "b.txt", "beta2\n");
}

#[test]
fn split_per_hunk_refuses_single_hunk_commit() {
    let test = common::TestRepo::new();
    test.commit_file("a.txt", "alpha\n", "base");
    let only_one = test.commit_file("a.txt", "alpha2\n", "single hunk");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    let result = git_repo.split_commit_per_hunk(&Oid::from(only_one), &head_oid);
    assert!(result.is_err(), "should fail when commit has only 1 hunk");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("fewer than 2 hunks"),
        "unexpected error message: {}",
        msg
    );
}

#[test]
fn split_per_hunk_last_piece_has_original_tree() {
    let test = common::TestRepo::new();

    test.commit_file(
        "a.txt",
        "line1\nline2\nline3\nPAD1\nPAD2\nPAD3\nPAD4\nPAD5\nline6\nline7\nline8\n",
        "base",
    );
    let to_split = test.commit_file(
        "a.txt",
        "LINE1\nline2\nline3\nPAD1\nPAD2\nPAD3\nPAD4\nPAD5\nLINE6\nline7\nline8\n",
        "two independent changes",
    );
    let original_tree = test.tree_id(to_split);

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();
    git_repo
        .split_commit_per_hunk(&Oid::from(to_split), &head_oid)
        .unwrap();

    assert_eq!(
        test.head_tree_id(),
        original_tree,
        "the last split piece must reproduce the original commit's tree"
    );
}
