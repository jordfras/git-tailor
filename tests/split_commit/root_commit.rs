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

/// Split the root commit (no parent) per file. The first split piece must
/// become a new orphan root and subsequent pieces must stack on top of it.
#[test]
fn split_root_commit_per_file() {
    let test = common::TestRepo::new();

    let root = test.commit_files(&[("a.txt", "alpha\n"), ("b.txt", "beta\n")], "root commit");

    let git_repo = test.git_repo();
    // head_oid == root since there's only one commit
    let head_oid = git_repo.head_oid().unwrap();
    assert_eq!(head_oid, Oid::from(root));

    git_repo
        .split_commit_per_file(&Oid::from(root), &head_oid)
        .unwrap();

    let new_head = test.repo.head().unwrap().target().unwrap();
    let mut revwalk = test.repo.revwalk().unwrap();
    revwalk.push(new_head).unwrap();
    let mut all_oids: Vec<git2::Oid> = revwalk.collect::<Result<_, _>>().unwrap();
    all_oids.reverse(); // oldest first

    assert_eq!(all_oids.len(), 2, "expected 2 split commits");

    let split1 = test.repo.find_commit(all_oids[0]).unwrap();
    let split2 = test.repo.find_commit(all_oids[1]).unwrap();

    assert_eq!(
        split1.parent_count(),
        0,
        "first split commit must be an orphan root"
    );
    assert_eq!(split2.parent_count(), 1);
    assert_eq!(split2.parent_id(0).unwrap(), all_oids[0]);

    assert!(
        split1
            .summary()
            .ok()
            .flatten()
            .unwrap_or("")
            .contains("(1/2)"),
        "got: {}",
        split1.summary().ok().flatten().unwrap_or("")
    );
    assert!(
        split2
            .summary()
            .ok()
            .flatten()
            .unwrap_or("")
            .contains("(2/2)"),
        "got: {}",
        split2.summary().ok().flatten().unwrap_or("")
    );

    assert_file_contents_at_head!(&test.repo, "a.txt", "alpha\n");
    assert_file_contents_at_head!(&test.repo, "b.txt", "beta\n");
}

/// Same as above but with a descendant commit that must be rebased on top of
/// the split result.
#[test]
fn split_root_commit_per_file_with_descendants() {
    let test = common::TestRepo::new();

    let root = test.commit_files(&[("a.txt", "alpha\n"), ("b.txt", "beta\n")], "root commit");
    test.commit_file("c.txt", "gamma\n", "descendant");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    git_repo
        .split_commit_per_file(&Oid::from(root), &head_oid)
        .unwrap();

    let new_head = test.repo.head().unwrap().target().unwrap();
    let mut revwalk = test.repo.revwalk().unwrap();
    revwalk.push(new_head).unwrap();
    let mut all_oids: Vec<git2::Oid> = revwalk.collect::<Result<_, _>>().unwrap();
    all_oids.reverse();

    // root_part1 + root_part2 + rebased descendant
    assert_eq!(all_oids.len(), 3, "expected 3 commits");

    let first = test.repo.find_commit(all_oids[0]).unwrap();
    assert_eq!(
        first.parent_count(),
        0,
        "first split commit must be orphan root"
    );

    // All files present at final HEAD
    assert_file_contents_at_head!(&test.repo, "a.txt", "alpha\n");
    assert_file_contents_at_head!(&test.repo, "b.txt", "beta\n");
    assert_file_contents_at_head!(&test.repo, "c.txt", "gamma\n");
}

/// Split the root commit per hunk. Two file introductions = 2 hunks.
/// The first split piece becomes the orphan root.
#[test]
fn split_root_commit_per_hunk() {
    let test = common::TestRepo::new();

    // The root commit introduces two files; each file introduction is one hunk.
    let root = test.commit_files(&[("a.txt", "alpha\n"), ("b.txt", "beta\n")], "root commit");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    git_repo
        .split_commit_per_hunk(&Oid::from(root), &head_oid)
        .unwrap();

    let new_head = test.repo.head().unwrap().target().unwrap();
    let mut revwalk = test.repo.revwalk().unwrap();
    revwalk.push(new_head).unwrap();
    let mut all_oids: Vec<git2::Oid> = revwalk.collect::<Result<_, _>>().unwrap();
    all_oids.reverse();

    assert_eq!(all_oids.len(), 2, "expected 2 split commits");

    let split1 = test.repo.find_commit(all_oids[0]).unwrap();
    assert_eq!(
        split1.parent_count(),
        0,
        "first split commit must be an orphan root"
    );

    assert_file_contents_at_head!(&test.repo, "a.txt", "alpha\n");
    assert_file_contents_at_head!(&test.repo, "b.txt", "beta\n");
}

/// Split the root commit per hunk group. The root touches 2 files and a
/// subsequent commit touches one of them, placing them in different fragmap
/// clusters — so there are 2 hunk groups and the split produces 2 pieces.
/// In `--all` mode reference_oid == root_oid.
#[test]
fn split_root_commit_per_hunk_group() {
    let test = common::TestRepo::new();

    // root introduces a.txt and b.txt
    let root = test.commit_files(&[("a.txt", "A\n"), ("b.txt", "B\n")], "root commit");
    // commit A touches a.txt, placing root's a.txt hunk in a different cluster
    // than root's b.txt hunk
    test.commit_file("a.txt", "A2\n", "commit A");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();
    // In --all mode reference_oid is the root commit OID
    let reference_oid = Oid::from(root);

    git_repo
        .split_commit_per_hunk_group(&Oid::from(root), &head_oid, &reference_oid)
        .unwrap();

    let new_head = test.repo.head().unwrap().target().unwrap();
    let mut revwalk = test.repo.revwalk().unwrap();
    revwalk.push(new_head).unwrap();
    let mut all_oids: Vec<git2::Oid> = revwalk.collect::<Result<_, _>>().unwrap();
    all_oids.reverse();

    // root_part1 (orphan) + root_part2 + A' (rebased)
    assert_eq!(
        all_oids.len(),
        3,
        "expected 3 commits: root_part1, root_part2, A'"
    );

    let first = test.repo.find_commit(all_oids[0]).unwrap();
    assert_eq!(
        first.parent_count(),
        0,
        "first split commit must be an orphan root"
    );

    // Final HEAD has both original files (with A's change to a.txt on top)
    assert_file_contents_at_head!(&test.repo, "a.txt", "A2\n");
    assert_file_contents_at_head!(&test.repo, "b.txt", "B\n");
}

#[test]
fn split_root_per_file_last_piece_has_original_tree() {
    let test = common::TestRepo::new();

    let root = test.commit_files(&[("a.txt", "alpha\n"), ("b.txt", "beta\n")], "root commit");
    let original_tree = test.tree_id(root);

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();
    git_repo
        .split_commit_per_file(&Oid::from(root), &head_oid)
        .unwrap();

    assert_eq!(
        test.head_tree_id(),
        original_tree,
        "the last split piece must reproduce the original root commit's tree"
    );
}
