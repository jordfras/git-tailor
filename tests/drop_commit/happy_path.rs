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
fn drop_head_commit_removes_it() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "v1\n", "base");
    let _middle = test.commit_file("a.txt", "v2\n", "middle");
    let to_drop = test.commit_file("a.txt", "v3\n", "to drop");

    let git_repo = test.git_repo();
    let result = git_repo
        .drop_commit(&Oid::from(to_drop), &Oid::from(to_drop))
        .unwrap();

    assert_rebase_complete!(result);

    let commits = test.commits_from_head(base);
    assert_eq!(commits.len(), 1, "should have 1 commit above base");

    assert_file_contents_at_head!(&test.repo, "a.txt", "v2\n");
}

#[test]
fn drop_middle_commit_rebases_descendants() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "base\n", "base");
    let to_drop = test.commit_file("b.txt", "added\n", "add b.txt");
    let child = test.commit_file("a.txt", "changed\n", "modify a.txt");

    let git_repo = test.git_repo();
    let result = git_repo
        .drop_commit(&Oid::from(to_drop), &Oid::from(child))
        .unwrap();

    assert_rebase_complete!(result);

    let commits = test.commits_from_head(base);
    assert_eq!(
        commits.len(),
        1,
        "should have 1 commit above base (dropped middle)"
    );

    let head_oid = test.repo.head().unwrap().target().unwrap();
    assert_file_contents_at_head!(&test.repo, "a.txt", "changed\n");

    // b.txt should not exist in the final tree since the commit that added it
    // was dropped.
    let head_commit = test.repo.find_commit(head_oid).unwrap();
    let tree = head_commit.tree().unwrap();
    assert!(
        tree.get_path(std::path::Path::new("b.txt")).is_err(),
        "b.txt should not exist after dropping the commit that added it"
    );
}

#[test]
fn drop_with_multiple_descendants() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "base\n", "base");
    let to_drop = test.commit_file("b.txt", "added\n", "add b.txt");
    let _child1 = test.commit_file("c.txt", "c1\n", "add c.txt");
    let head = test.commit_file("d.txt", "d1\n", "add d.txt");

    let git_repo = test.git_repo();
    let result = git_repo
        .drop_commit(&Oid::from(to_drop), &Oid::from(head))
        .unwrap();

    assert_rebase_complete!(result);

    let commits = test.commits_from_head(base);
    assert_eq!(
        commits.len(),
        2,
        "should have 2 commits above base after dropping middle"
    );

    let head_oid = test.repo.head().unwrap().target().unwrap();
    let head_tree = test.repo.find_commit(head_oid).unwrap().tree().unwrap();
    assert!(head_tree.get_path(std::path::Path::new("c.txt")).is_ok());
    assert!(head_tree.get_path(std::path::Path::new("d.txt")).is_ok());
    assert!(head_tree.get_path(std::path::Path::new("b.txt")).is_err());
}

#[test]
fn drop_preserves_commit_messages() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "base\n", "base");
    let to_drop = test.commit_file("b.txt", "added\n", "add b.txt");
    // Multi-line message: drop must preserve the full body, not just the summary.
    let _child = test.commit_file("c.txt", "c1\n", "important change\n\nDetailed body text.");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();
    git_repo
        .drop_commit(&Oid::from(to_drop), &head_oid)
        .unwrap();

    let commits = test.commits_from_head(base);
    assert_eq!(commits.len(), 1);
    let rebased = test.repo.find_commit(commits[0]).unwrap();
    assert_eq!(
        rebased.message().unwrap(),
        "important change\n\nDetailed body text.",
        "rebased descendant should keep its full commit message"
    );
}
