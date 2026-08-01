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
fn split_out_files_single_file_two_files() {
    let test = common::TestRepo::new();

    let base = test.commit_files(&[("a.txt", "a\n"), ("b.txt", "b\n")], "base");
    let to_split = test.commit_files(&[("a.txt", "a2\n"), ("b.txt", "b2\n")], "change both");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    git_repo
        .split_commit_out_files(&Oid::from(to_split), &["b.txt".to_string()], &head_oid)
        .unwrap();

    let commits = test.commits_from_head(base);
    assert_eq!(commits.len(), 2);
    let first = commits[0];
    let second = commits[1];

    assert_file_contents!(&test.repo, first, "a.txt", "a2\n");
    assert_file_contents!(&test.repo, first, "b.txt", "b\n");
    assert_file_contents!(&test.repo, second, "a.txt", "a2\n");
    assert_file_contents!(&test.repo, second, "b.txt", "b2\n");

    let msg1 = test.repo.find_commit(first).unwrap();
    assert_eq!(msg1.summary().unwrap().unwrap(), "change both");
    let msg2 = test.repo.find_commit(second).unwrap();
    assert_eq!(msg2.summary().unwrap().unwrap(), "change both (b.txt)");
}

#[test]
fn split_out_files_multiple_files_combined_into_one_commit() {
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

    // Select a.txt and c.txt; b.txt must stay behind in the rest commit.
    git_repo
        .split_commit_out_files(
            &Oid::from(to_split),
            &["a.txt".to_string(), "c.txt".to_string()],
            &head_oid,
        )
        .unwrap();

    let commits = test.commits_from_head(base);
    assert_eq!(commits.len(), 2, "both selected files land in one commit");
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
        "change all three (2 files)"
    );
}

#[test]
fn split_out_files_rebases_descendants() {
    let test = common::TestRepo::new();

    let base = test.commit_files(&[("a.txt", "a\n"), ("b.txt", "b\n")], "base");
    let to_split = test.commit_files(&[("a.txt", "a2\n"), ("b.txt", "b2\n")], "change both");
    test.commit_file("c.txt", "gamma\n", "add c");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    git_repo
        .split_commit_out_files(&Oid::from(to_split), &["a.txt".to_string()], &head_oid)
        .unwrap();

    let commits = test.commits_from_head(base);
    assert_eq!(commits.len(), 3, "two split commits + rebased descendant");

    let tip = *commits.last().unwrap();
    assert_file_contents!(&test.repo, tip, "a.txt", "a2\n");
    assert_file_contents!(&test.repo, tip, "b.txt", "b2\n");
    assert_file_contents!(&test.repo, tip, "c.txt", "gamma\n");
}

#[test]
fn split_out_files_handles_added_file() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "a\n", "base");
    let to_split = test.commit_files(&[("a.txt", "a2\n"), ("b.txt", "b\n")], "change and add");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    git_repo
        .split_commit_out_files(&Oid::from(to_split), &["b.txt".to_string()], &head_oid)
        .unwrap();

    let commits = test.commits_from_head(base);
    let first = commits[0];
    let first_tree = test.repo.find_commit(first).unwrap().tree().unwrap();
    assert!(
        first_tree.get_path(std::path::Path::new("b.txt")).is_err(),
        "the added file must be absent from the first (rest) commit"
    );
}

#[test]
fn split_out_files_refuses_empty_selection() {
    let test = common::TestRepo::new();
    test.commit_files(&[("a.txt", "a\n"), ("b.txt", "b\n")], "base");
    let to_split = test.commit_files(&[("a.txt", "a2\n"), ("b.txt", "b2\n")], "change both");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    let result = git_repo.split_commit_out_files(&Oid::from(to_split), &[], &head_oid);
    assert!(result.is_err(), "should fail with no files selected");
}

#[test]
fn split_out_files_refuses_when_every_file_is_selected() {
    let test = common::TestRepo::new();
    test.commit_files(&[("a.txt", "a\n"), ("b.txt", "b\n")], "base");
    let to_split = test.commit_files(&[("a.txt", "a2\n"), ("b.txt", "b2\n")], "change both");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    let result = git_repo.split_commit_out_files(
        &Oid::from(to_split),
        &["a.txt".to_string(), "b.txt".to_string()],
        &head_oid,
    );
    assert!(
        result.is_err(),
        "should fail when the selection covers every file"
    );
}

#[test]
fn split_out_files_refuses_unknown_file() {
    let test = common::TestRepo::new();
    test.commit_files(&[("a.txt", "a\n"), ("b.txt", "b\n")], "base");
    let to_split = test.commit_files(&[("a.txt", "a2\n"), ("b.txt", "b2\n")], "change both");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    let result = git_repo.split_commit_out_files(
        &Oid::from(to_split),
        &["missing.txt".to_string()],
        &head_oid,
    );
    assert!(result.is_err(), "should fail for a file not in the commit");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("missing.txt"),
        "error should mention the file, got: {msg}"
    );
}

#[test]
fn split_out_files_refuses_dirty_overlap() {
    let test = common::TestRepo::new();
    test.commit_files(&[("a.txt", "a\n"), ("b.txt", "b\n")], "base");
    let to_split = test.commit_files(&[("a.txt", "a2\n"), ("b.txt", "b2\n")], "change both");

    test.write_file("a.txt", "DIRTY\n");
    test.stage_file("a.txt");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    let result =
        git_repo.split_commit_out_files(&Oid::from(to_split), &["b.txt".to_string()], &head_oid);
    assert!(result.is_err(), "should fail when staged changes overlap");
}

#[test]
fn split_out_files_last_piece_has_original_tree() {
    let test = common::TestRepo::new();

    test.commit_files(&[("a.txt", "a\n"), ("b.txt", "b\n")], "base");
    let to_split = test.commit_files(&[("a.txt", "a2\n"), ("b.txt", "b2\n")], "change both");
    let original_tree = test.tree_id(to_split);

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();
    git_repo
        .split_commit_out_files(&Oid::from(to_split), &["b.txt".to_string()], &head_oid)
        .unwrap();

    assert_eq!(
        test.head_tree_id(),
        original_tree,
        "the last split piece must reproduce the original commit's tree"
    );
}
