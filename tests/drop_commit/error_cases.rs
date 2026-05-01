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
fn drop_abort_after_second_conflict_restores_branch() {
    // Regression test: abort must work even when it is triggered after a
    // second (or later) conflict, not just the first one.
    //
    // Layout (oldest→newest): base → to_drop → child1 → child2
    //
    // to_drop edits a.txt.  child1 and child2 both edit a.txt too, so
    // cherry-picking child1 onto base conflicts, and after a fake-resolve
    // cherry-picking child2 onto that conflicts again.  Aborting at that
    // second conflict must restore HEAD to the original tip (child2).
    let test = common::TestRepo::new();

    let _base = test.commit_file("a.txt", "v1\n", "base");
    let to_drop = test.commit_file("a.txt", "v2\n", "change a (will be dropped)");
    let child1 = test.commit_file("a.txt", "v3\n", "change a again");
    let child2 = test.commit_file("a.txt", "v4\n", "change a a third time");

    let git_repo = test.git_repo();
    let head_oid_before = git_repo.head_oid().unwrap();
    assert_eq!(head_oid_before, Oid::from(child2));

    // First drop → first conflict on child1.
    let result = git_repo
        .drop_commit(&Oid::from(to_drop), &Oid::from(child2))
        .unwrap();
    let state1 = expect_rebase_conflict!(result);
    assert_eq!(state1.conflicting_commit_oid, Oid::from(child1));
    assert_eq!(state1.original_branch_oid, Oid::from(child2));

    // Fake-resolve conflict 1: write content and re-stage.
    test.write_file("a.txt", "v1\nv3\n");
    let mut index = test.repo.index().unwrap();
    index
        .conflict_remove(std::path::Path::new("a.txt"))
        .unwrap();
    index.add_path(std::path::Path::new("a.txt")).unwrap();
    index.write().unwrap();

    // Continue → second conflict on child2.
    let result = git_repo.rebase_continue(&state1).unwrap();
    let state2 = expect_rebase_conflict!(result);
    assert_eq!(state2.conflicting_commit_oid, Oid::from(child2));
    // original_branch_oid must still refer to the pre-drop HEAD.
    assert_eq!(state2.original_branch_oid, Oid::from(child2));

    // Abort from the second conflict — must fully restore the branch.
    git_repo.rebase_abort(&state2).unwrap();

    let head_after = test.repo.head().unwrap().target().unwrap();
    assert_eq!(
        head_after, child2,
        "HEAD must be fully restored after abort at second conflict"
    );
    assert_file_contents_at_head!(&test.repo, "a.txt", "v4\n");
}

#[test]
fn drop_root_commit_fails() {
    let test = common::TestRepo::new();

    let root = test.commit_file("a.txt", "v1\n", "root");

    let git_repo = test.git_repo();
    let result = git_repo.drop_commit(&Oid::from(root), &Oid::from(root));

    assert!(result.is_err(), "dropping a root commit should fail");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("merge or root"),
        "error should mention merge or root: {msg}"
    );
}

#[test]
fn drop_commit_with_no_descendants() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "v1\n", "base");
    let to_drop = test.commit_file("b.txt", "added\n", "add b");

    let git_repo = test.git_repo();
    let result = git_repo
        .drop_commit(&Oid::from(to_drop), &Oid::from(to_drop))
        .unwrap();

    assert_rebase_complete!(result);

    let head_oid = test.repo.head().unwrap().target().unwrap();
    assert_eq!(
        head_oid, base,
        "HEAD should point to base after dropping the only commit above it"
    );
}
