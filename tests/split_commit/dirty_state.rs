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
fn split_per_file_preserves_staged_changes() {
    let test = common::TestRepo::new();

    let _base = test.commit_files(&[("a.txt", "alpha\n"), ("b.txt", "beta\n")], "base");
    let to_split = test.commit_files(&[("a.txt", "alpha2\n"), ("b.txt", "beta2\n")], "big change");

    // Stage a change to an unrelated file before the operation
    let workdir = test.repo.workdir().unwrap();
    test.write_file("unrelated.txt", "staged work\n");
    test.stage_file("unrelated.txt");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();
    git_repo
        .split_commit_per_file(&Oid::from(to_split), &head_oid)
        .unwrap();

    // The staged file must still be staged (present in the index at stage 0)
    let mut idx = test.repo.index().unwrap();
    idx.read(true).unwrap();
    assert!(
        idx.get_path(std::path::Path::new("unrelated.txt"), 0)
            .is_some(),
        "staged file should still be in the index after split"
    );
    assert_eq!(
        std::fs::read_to_string(workdir.join("unrelated.txt")).unwrap(),
        "staged work\n",
        "staged file on disk should be unchanged"
    );
}

#[test]
fn split_per_file_preserves_unstaged_changes() {
    let test = common::TestRepo::new();

    // c.txt is committed in base but NOT touched by to_split, so modifying it
    // won't trigger the dirty-overlap guard.
    let _base = test.commit_files(
        &[
            ("a.txt", "alpha\n"),
            ("b.txt", "beta\n"),
            ("c.txt", "gamma\n"),
        ],
        "base",
    );
    let to_split = test.commit_files(&[("a.txt", "alpha2\n"), ("b.txt", "beta2\n")], "big change");

    // Write an unstaged change to c.txt (not touched by to_split)
    let workdir = test.repo.workdir().unwrap();
    test.write_file("c.txt", "unstaged work\n");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();
    git_repo
        .split_commit_per_file(&Oid::from(to_split), &head_oid)
        .unwrap();

    // The unstaged modification must survive
    assert_eq!(
        std::fs::read_to_string(workdir.join("c.txt")).unwrap(),
        "unstaged work\n",
        "unstaged content should be unchanged after split"
    );
}

#[test]
fn split_per_hunk_preserves_staged_changes() {
    let test = common::TestRepo::new();

    let _base = test.commit_file("a.txt", "line1\nline2\n", "base");
    let to_split = test.commit_file("a.txt", "LINE1\nline2\nLINE3\n", "two hunks");

    // Stage a change to an unrelated file
    let workdir = test.repo.workdir().unwrap();
    test.write_file("unrelated.txt", "staged work\n");
    test.stage_file("unrelated.txt");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();
    git_repo
        .split_commit_per_hunk(&Oid::from(to_split), &head_oid)
        .unwrap();

    let mut idx = test.repo.index().unwrap();
    idx.read(true).unwrap();
    assert!(
        idx.get_path(std::path::Path::new("unrelated.txt"), 0)
            .is_some(),
        "staged file should still be in the index after split"
    );
    assert_eq!(
        std::fs::read_to_string(workdir.join("unrelated.txt")).unwrap(),
        "staged work\n",
        "staged file on disk should be unchanged"
    );
}

#[test]
fn split_per_hunk_preserves_unstaged_changes() {
    let test = common::TestRepo::new();

    let _base = test.commit_file("a.txt", "line1\nline2\n", "base");
    let to_split = test.commit_file("a.txt", "LINE1\nline2\nLINE3\n", "two hunks");

    // Write an unstaged change to an unrelated tracked file
    // Use a file that won't overlap with the commit being split
    test.commit_file("other.txt", "other\n", "add other");
    let workdir = test.repo.workdir().unwrap();
    test.write_file("other.txt", "unstaged work\n");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();
    git_repo
        .split_commit_per_hunk(&Oid::from(to_split), &head_oid)
        .unwrap();

    assert_eq!(
        std::fs::read_to_string(workdir.join("other.txt")).unwrap(),
        "unstaged work\n",
        "unstaged content should be unchanged after split"
    );
}

#[test]
fn split_per_hunk_preserves_commit_message_body() {
    let test = common::TestRepo::new();

    let _base = test.commit_file(
        "a.txt",
        "line1\nline2\nline3\nPAD1\nPAD2\nPAD3\nPAD4\nPAD5\nline6\nline7\nline8\n",
        "base",
    );
    let to_split = test.commit_file(
        "a.txt",
        "LINE1\nline2\nline3\nPAD1\nPAD2\nPAD3\nPAD4\nPAD5\nLINE6\nline7\nline8\n",
        "two changes\n\nThis body should be kept.",
    );

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();
    git_repo
        .split_commit_per_hunk(&Oid::from(to_split), &head_oid)
        .unwrap();

    let stop = test
        .repo
        .find_commit(to_split)
        .unwrap()
        .parent_id(0)
        .unwrap();
    for oid in test.commits_from_head(stop) {
        let commit = test.repo.find_commit(oid).unwrap();
        let msg = commit.message().unwrap_or("");
        assert!(
            msg.contains("This body should be kept."),
            "expected body in split commit, got: {:?}",
            msg
        );
    }
}
