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
fn drop_root_commit_makes_next_commit_orphan() {
    let test = common::TestRepo::new();

    let root = test.commit_file("a.txt", "root\n", "root commit");
    let _child1 = test.commit_file("b.txt", "child1\n", "first child");
    let child2 = test.commit_file("c.txt", "child2\n", "second child");

    let git_repo = test.git_repo();
    let result = git_repo
        .drop_commit(&Oid::from(root), &Oid::from(child2))
        .unwrap();

    assert_rebase_complete!(result);

    // Should have exactly 2 commits in total (root was dropped).
    let new_head = test.repo.head().unwrap().target().unwrap();
    let mut revwalk = test.repo.revwalk().unwrap();
    revwalk.push(new_head).unwrap();
    let all_oids: Vec<git2::Oid> = revwalk.collect::<Result<_, _>>().unwrap();
    assert_eq!(all_oids.len(), 2, "expected 2 commits after dropping root");

    // The oldest commit (new root) must be an orphan — no parents.
    let new_root_oid = *all_oids.last().unwrap();
    let new_root = test.repo.find_commit(new_root_oid).unwrap();
    assert_eq!(
        new_root.parent_count(),
        0,
        "first descendant must become an orphan root"
    );

    // Files from the dropped root commit should not be in the new root's tree.
    let new_root_tree = new_root.tree().unwrap();
    assert!(
        new_root_tree
            .get_path(std::path::Path::new("a.txt"))
            .is_err(),
        "a.txt (from dropped root) should not exist in the new root"
    );

    // Files from descendant commits should still be present at HEAD.
    assert_file_contents_at_head!(&test.repo, "b.txt", "child1\n");
    assert_file_contents_at_head!(&test.repo, "c.txt", "child2\n");
}

#[test]
fn drop_root_commit_single_descendant() {
    let test = common::TestRepo::new();

    let root = test.commit_file("a.txt", "root\n", "root commit");
    let child = test.commit_file("b.txt", "child\n", "only child");

    let git_repo = test.git_repo();
    let result = git_repo
        .drop_commit(&Oid::from(root), &Oid::from(child))
        .unwrap();

    assert_rebase_complete!(result);

    // Should have exactly 1 commit (the former child, now orphan root).
    let new_head = test.repo.head().unwrap().target().unwrap();
    let mut revwalk = test.repo.revwalk().unwrap();
    revwalk.push(new_head).unwrap();
    let all_oids: Vec<git2::Oid> = revwalk.collect::<Result<_, _>>().unwrap();
    assert_eq!(
        all_oids.len(),
        1,
        "expected 1 commit after dropping root with single descendant"
    );

    let new_root = test.repo.find_commit(all_oids[0]).unwrap();
    assert_eq!(new_root.parent_count(), 0, "must be an orphan root");

    // The dropped root's file should be gone; the child's file should remain.
    let tree = new_root.tree().unwrap();
    assert!(
        tree.get_path(std::path::Path::new("a.txt")).is_err(),
        "a.txt (from dropped root) should not exist"
    );
    assert_file_contents_at_head!(&test.repo, "b.txt", "child\n");
}
