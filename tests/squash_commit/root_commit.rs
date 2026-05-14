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

/// Squash a second commit (source) into the root commit (target).
/// The result must be an orphan commit whose tree contains both files.
#[test]
fn squash_into_root_commit() {
    let test = common::TestRepo::new();

    let root = test.commit_file("a.txt", "root\n", "root commit");
    let source = test.commit_file("b.txt", "source\n", "source commit");

    let git_repo = test.git_repo();
    let result = git_repo
        .squash_commits(
            &Oid::from(source),
            &Oid::from(root),
            "squashed message",
            &Oid::from(source),
        )
        .unwrap();

    assert_rebase_complete!(result);

    // Exactly one commit: the combined orphan root.
    let new_head = test.repo.head().unwrap().target().unwrap();
    let mut revwalk = test.repo.revwalk().unwrap();
    revwalk.push(new_head).unwrap();
    let all_oids: Vec<git2::Oid> = revwalk.collect::<Result<_, _>>().unwrap();
    assert_eq!(
        all_oids.len(),
        1,
        "expected exactly one commit after squash"
    );

    // The new commit must be an orphan.
    let new_root = test.repo.find_commit(new_head).unwrap();
    assert_eq!(
        new_root.parent_count(),
        0,
        "squashed root commit must be an orphan"
    );
    assert_eq!(new_root.message().unwrap(), "squashed message");

    // Both files must be present.
    assert_file_contents_at_head!(&test.repo, "a.txt", "root\n");
    assert_file_contents_at_head!(&test.repo, "b.txt", "source\n");
}

/// Fixup into the root commit: result keeps the root commit's message.
#[test]
fn fixup_into_root_commit() {
    let test = common::TestRepo::new();

    let root = test.commit_file("a.txt", "root\n", "root commit");
    let source = test.commit_file("b.txt", "source\n", "source commit");

    let git_repo = test.git_repo();
    let result = git_repo
        .squash_commits(
            &Oid::from(source),
            &Oid::from(root),
            "root commit", // fixup: caller passes only the target message
            &Oid::from(source),
        )
        .unwrap();

    assert_rebase_complete!(result);

    let new_head = test.repo.head().unwrap().target().unwrap();
    let new_root = test.repo.find_commit(new_head).unwrap();
    assert_eq!(new_root.parent_count(), 0);
    assert_eq!(new_root.message().unwrap(), "root commit");

    assert_file_contents_at_head!(&test.repo, "a.txt", "root\n");
    assert_file_contents_at_head!(&test.repo, "b.txt", "source\n");
}

/// Squash a non-adjacent commit into the root when there are intermediate
/// commits. The intermediate commits must be correctly rebased on top of
/// the new orphan root.
#[test]
fn squash_into_root_with_descendants() {
    let test = common::TestRepo::new();

    let root = test.commit_file("a.txt", "root\n", "root commit");
    let middle = test.commit_file("c.txt", "middle\n", "middle commit");
    let source = test.commit_file("b.txt", "source\n", "source commit");

    let git_repo = test.git_repo();
    let result = git_repo
        .squash_commits(
            &Oid::from(source),
            &Oid::from(root),
            "squashed",
            &Oid::from(source),
        )
        .unwrap();

    assert_rebase_complete!(result);

    // Two commits: new orphan root + rebased middle.
    let new_head = test.repo.head().unwrap().target().unwrap();
    let mut revwalk = test.repo.revwalk().unwrap();
    revwalk.push(new_head).unwrap();
    let all_oids: Vec<git2::Oid> = revwalk.collect::<Result<_, _>>().unwrap();
    assert_eq!(all_oids.len(), 2);

    // Oldest commit is an orphan root.
    let new_root_oid = *all_oids.last().unwrap();
    let new_root = test.repo.find_commit(new_root_oid).unwrap();
    assert_eq!(new_root.parent_count(), 0);
    assert_eq!(new_root.message().unwrap(), "squashed");

    // Middle commit was rebased (new OID) and retains its message.
    let rebased_middle_oid = all_oids[0];
    let rebased_middle = test.repo.find_commit(rebased_middle_oid).unwrap();
    assert_eq!(rebased_middle.message().unwrap(), "middle commit");
    assert_ne!(
        rebased_middle_oid, middle,
        "middle should have a new OID after rebase"
    );

    // All files present at HEAD.
    assert_file_contents_at_head!(&test.repo, "a.txt", "root\n");
    assert_file_contents_at_head!(&test.repo, "b.txt", "source\n");
    assert_file_contents_at_head!(&test.repo, "c.txt", "middle\n");
}
