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

#[allow(dead_code)]
mod common;

use git_tailor::{Oid, repo::GitRepo};

// ---------------------------------------------------------------------------
// Happy-path tests
// ---------------------------------------------------------------------------

#[test]
fn reword_head_commit_changes_message() {
    let test = common::TestRepo::new();

    let _base = test.commit_file("a.txt", "v1\n", "base");
    let to_reword = test.commit_file("a.txt", "v2\n", "old message");

    let git_repo = test.git_repo();
    git_repo
        .reword_commit(&Oid::from(to_reword), "new message", &Oid::from(to_reword))
        .unwrap();

    let head_oid = test.repo.head().unwrap().target().unwrap();
    let head_commit = test.repo.find_commit(head_oid).unwrap();
    assert_eq!(head_commit.message().unwrap().trim(), "new message");
}

#[test]
fn reword_middle_commit_propagates_to_descendants() {
    let test = common::TestRepo::new();

    let _base = test.commit_file("a.txt", "v1\n", "base");
    let to_reword = test.commit_file("a.txt", "v2\n", "old message");
    let _descendant = test.commit_file("b.txt", "hello\n", "descendant");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();
    git_repo
        .reword_commit(&Oid::from(to_reword), "new message", &head_oid)
        .unwrap();

    let new_head_oid = test.repo.head().unwrap().target().unwrap();
    let new_head = test.repo.find_commit(new_head_oid).unwrap();
    // The HEAD (descendant) should still have its own message
    assert_eq!(new_head.message().unwrap().trim(), "descendant");
    // Its parent is the reworded commit
    let rewarded_commit = new_head.parent(0).unwrap();
    assert_eq!(rewarded_commit.message().unwrap().trim(), "new message");
}

// ---------------------------------------------------------------------------
// Dirty-state preservation tests
// ---------------------------------------------------------------------------

#[test]
fn reword_preserves_staged_changes() {
    let test = common::TestRepo::new();

    let _base = test.commit_file("a.txt", "v1\n", "base");
    let to_reword = test.commit_file("a.txt", "v2\n", "old message");

    // Stage a change to an unrelated file before the operation
    let workdir = test.repo.workdir().unwrap();
    std::fs::write(workdir.join("unrelated.txt"), "staged work\n").unwrap();
    let mut index = test.repo.index().unwrap();
    index
        .add_path(std::path::Path::new("unrelated.txt"))
        .unwrap();
    index.write().unwrap();

    let git_repo = test.git_repo();
    git_repo
        .reword_commit(&Oid::from(to_reword), "new message", &Oid::from(to_reword))
        .unwrap();

    // Staged file must still be present in the index
    let mut idx = test.repo.index().unwrap();
    idx.read(true).unwrap();
    assert!(
        idx.get_path(std::path::Path::new("unrelated.txt"), 0)
            .is_some(),
        "staged file should still be in the index after reword"
    );
    assert_eq!(
        std::fs::read_to_string(workdir.join("unrelated.txt")).unwrap(),
        "staged work\n",
        "staged file on disk should be unchanged"
    );
}

#[test]
fn reword_preserves_unstaged_changes() {
    let test = common::TestRepo::new();

    let _base = test.commit_file("a.txt", "v1\n", "base");
    let to_reword = test.commit_file("b.txt", "content\n", "old message");

    // Write an unstaged change to a tracked file that the reword doesn't touch
    let workdir = test.repo.workdir().unwrap();
    std::fs::write(workdir.join("a.txt"), "unstaged work\n").unwrap();

    let git_repo = test.git_repo();
    git_repo
        .reword_commit(&Oid::from(to_reword), "new message", &Oid::from(to_reword))
        .unwrap();

    assert_eq!(
        std::fs::read_to_string(workdir.join("a.txt")).unwrap(),
        "unstaged work\n",
        "unstaged content should be unchanged after reword"
    );
}
