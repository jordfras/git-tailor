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

use common::prelude::*;

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
    test.write_file("unrelated.txt", "staged work\n");
    test.stage_file("unrelated.txt");

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
    test.write_file("a.txt", "unstaged work\n");

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

#[test]
fn reword_preserves_descendant_trees() {
    // Rewording rebuilds the commit with commit.tree() verbatim, so replaying
    // descendants onto it must reproduce their trees byte-for-byte — the
    // property that makes the replay conflict-free.
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "v1\n", "base");
    let to_reword = test.commit_file("a.txt", "v2\n", "old message");
    let original_tree = test.tree_id(to_reword);
    let d1 = test.commit_file("b.txt", "b\n", "descendant 1");
    let d2 = test.commit_file("c.txt", "c\n", "descendant 2");
    let d1_tree = test.tree_id(d1);
    let d2_tree = test.tree_id(d2);

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();
    git_repo
        .reword_commit(&Oid::from(to_reword), "new message", &head_oid)
        .unwrap();

    assert_eq!(
        test.tree_ids_from_head(base),
        vec![original_tree, d1_tree, d2_tree],
        "rewording must not alter any tree in the range"
    );
}

#[test]
fn reword_rejects_a_merge_commit_in_the_replay_range() {
    // Descendants are collected with a bare revwalk that stops at the reworded
    // commit. A merge in that range makes the walk unreliable (it can emit
    // commits that are not descendants at all), so it must be refused with a
    // message of our own rather than whatever libgit2 says about mainlines.
    let test = common::TestRepo::new();

    test.commit_file("a.txt", "v1\n", "base");
    let to_reword = test.commit_file("a.txt", "v2\n", "old message");
    let d1 = test.commit_file("b.txt", "b\n", "descendant");

    let sig = git2::Signature::now("Test User", "test@example.com").unwrap();
    let d1_commit = test.repo.find_commit(d1).unwrap();
    let target_commit = test.repo.find_commit(to_reword).unwrap();
    let tree = d1_commit.tree().unwrap();
    test.repo
        .commit(
            Some("HEAD"),
            &sig,
            &sig,
            "a merge",
            &tree,
            &[&d1_commit, &target_commit],
        )
        .unwrap();

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();
    let head_before = head_oid.clone();

    let err = git_repo
        .reword_commit(&Oid::from(to_reword), "new message", &head_oid)
        .expect_err("a merge in the replay range must be refused");
    let msg = err.to_string();

    assert!(
        !msg.contains("mainline"),
        "should not leak libgit2's mainline wording: {msg}"
    );
    assert!(
        msg.to_lowercase().contains("merge"),
        "the error should name the merge commit as the reason: {msg}"
    );
    assert_eq!(
        git_repo.head_oid().unwrap(),
        head_before,
        "the branch must be left untouched"
    );
}
