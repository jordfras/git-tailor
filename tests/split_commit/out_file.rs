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

/// Number of deltas between a commit and its first parent.
fn delta_count(test: &common::TestRepo, oid: git2::Oid) -> usize {
    let commit = test.repo.find_commit(oid).unwrap();
    let parent_tree = test
        .repo
        .find_commit(commit.parent_id(0).unwrap())
        .unwrap()
        .tree()
        .unwrap();
    let commit_tree = commit.tree().unwrap();
    test.repo
        .diff_tree_to_tree(Some(&parent_tree), Some(&commit_tree), None)
        .unwrap()
        .deltas()
        .len()
}

#[test]
fn split_out_file_peels_chosen_file_into_second_commit() {
    let test = common::TestRepo::new();

    let base = test.commit_files(
        &[("a.txt", "a\n"), ("b.txt", "b\n"), ("c.txt", "c\n")],
        "base",
    );
    let to_split = test.commit_files(
        &[("a.txt", "a2\n"), ("b.txt", "b2\n"), ("c.txt", "c2\n")],
        "big change",
    );

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    git_repo
        .split_commit_out_file(&Oid::from(to_split), "b.txt", &head_oid)
        .unwrap();

    let commits = test.commits_from_head(base);
    assert_eq!(commits.len(), 2, "expected exactly two commits");
    let first = commits[0];
    let second = commits[1];

    // The first commit keeps the other two files and is the "rest" commit.
    assert_eq!(delta_count(&test, first), 2, "first commit keeps a + c");
    // The second commit peels out only the chosen file.
    assert_eq!(delta_count(&test, second), 1, "second commit holds only b");

    // First commit message is unchanged; second is suffixed with the file name.
    let msg1 = test.repo.find_commit(first).unwrap();
    let msg1 = msg1.summary().ok().flatten().unwrap_or("");
    assert_eq!(msg1, "big change", "first commit title must be unchanged");

    let msg2 = test.repo.find_commit(second).unwrap();
    let msg2 = msg2.summary().ok().flatten().unwrap_or("");
    assert_eq!(
        msg2, "big change (b.txt)",
        "second commit suffixed with file"
    );

    // The combined result matches the original commit's tree.
    assert_file_contents!(&test.repo, second, "a.txt", "a2\n");
    assert_file_contents!(&test.repo, second, "b.txt", "b2\n");
    assert_file_contents!(&test.repo, second, "c.txt", "c2\n");
    // The intermediate commit still has the old content for the peeled file.
    assert_file_contents!(&test.repo, first, "b.txt", "b\n");
    assert_file_contents!(&test.repo, first, "a.txt", "a2\n");
}

#[test]
fn split_out_file_rebases_descendants() {
    let test = common::TestRepo::new();

    let base = test.commit_files(&[("a.txt", "a\n"), ("b.txt", "b\n")], "base");
    let to_split = test.commit_files(&[("a.txt", "a2\n"), ("b.txt", "b2\n")], "big change");
    test.commit_file("c.txt", "gamma\n", "add c");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    git_repo
        .split_commit_out_file(&Oid::from(to_split), "a.txt", &head_oid)
        .unwrap();

    // base → rest → peeled-a → rebased-c
    let commits = test.commits_from_head(base);
    assert_eq!(commits.len(), 3, "two split commits + rebased descendant");

    let tip = *commits.last().unwrap();
    assert_file_contents!(&test.repo, tip, "c.txt", "gamma\n");
    assert_file_contents!(&test.repo, tip, "a.txt", "a2\n");
    assert_file_contents!(&test.repo, tip, "b.txt", "b2\n");
}

#[test]
fn split_out_file_handles_added_file() {
    let test = common::TestRepo::new();

    // a.txt exists at base; the commit modifies it and *adds* b.txt.
    let base = test.commit_file("a.txt", "a\n", "base");
    let to_split = test.commit_files(&[("a.txt", "a2\n"), ("b.txt", "b\n")], "change and add");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    // Peel out the added file: the first commit must not contain it at all.
    git_repo
        .split_commit_out_file(&Oid::from(to_split), "b.txt", &head_oid)
        .unwrap();

    let commits = test.commits_from_head(base);
    assert_eq!(commits.len(), 2);
    let first = commits[0];
    let second = commits[1];

    let first_tree = test.repo.find_commit(first).unwrap().tree().unwrap();
    assert!(
        first_tree.get_path(std::path::Path::new("b.txt")).is_err(),
        "the added file must be absent from the first (rest) commit"
    );
    assert_file_contents!(&test.repo, first, "a.txt", "a2\n");
    assert_file_contents!(&test.repo, second, "b.txt", "b\n");
}

#[test]
fn split_out_file_refuses_single_file_commit() {
    let test = common::TestRepo::new();
    test.commit_file("a.txt", "a\n", "base");
    let only_one = test.commit_file("a.txt", "a2\n", "single file");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    let result = git_repo.split_commit_out_file(&Oid::from(only_one), "a.txt", &head_oid);
    assert!(
        result.is_err(),
        "should fail when commit touches only 1 file"
    );
}

#[test]
fn split_out_file_refuses_unknown_file() {
    let test = common::TestRepo::new();
    test.commit_files(&[("a.txt", "a\n"), ("b.txt", "b\n")], "base");
    let to_split = test.commit_files(&[("a.txt", "a2\n"), ("b.txt", "b2\n")], "big change");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    let result = git_repo.split_commit_out_file(&Oid::from(to_split), "missing.txt", &head_oid);
    assert!(result.is_err(), "should fail for a file not in the commit");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("missing.txt"),
        "error should mention the file, got: {msg}"
    );
}

#[test]
fn split_out_file_refuses_dirty_overlap() {
    let test = common::TestRepo::new();
    test.commit_files(&[("a.txt", "a\n"), ("b.txt", "b\n")], "base");
    let to_split = test.commit_files(&[("a.txt", "a2\n"), ("b.txt", "b2\n")], "big change");

    test.write_file("a.txt", "DIRTY\n");
    test.stage_file("a.txt");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    let result = git_repo.split_commit_out_file(&Oid::from(to_split), "b.txt", &head_oid);
    assert!(result.is_err(), "should fail when staged changes overlap");
}

#[test]
fn list_commit_files_returns_changed_paths() {
    let test = common::TestRepo::new();
    test.commit_files(&[("a.txt", "a\n"), ("b.txt", "b\n")], "base");
    let to_split = test.commit_files(&[("a.txt", "a2\n"), ("b.txt", "b2\n")], "big change");

    let git_repo = test.git_repo();
    let mut files = git_repo.list_commit_files(&Oid::from(to_split)).unwrap();
    files.sort();
    assert_eq!(files, vec!["a.txt".to_string(), "b.txt".to_string()]);
}
