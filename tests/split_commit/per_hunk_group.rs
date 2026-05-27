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
fn split_per_hunk_group_two_groups_shared_context() {
    // Commit A only modifies a.txt. Commit K modifies a.txt (in the same region
    // as A) and b.txt (only K). The fragmap assigns K's a.txt hunk to the cluster
    // with commit_oids={A,K} and K's b.txt hunk to commit_oids={K}. Those are
    // two different dedup columns → split produces 2 commits.
    let test = common::TestRepo::new();

    let base = test.commit_files(&[("a.txt", "A\n"), ("b.txt", "B\n")], "base");
    test.commit_file("a.txt", "A2\n", "commit A");
    let to_split = test.commit_files(&[("a.txt", "A3\n"), ("b.txt", "B2\n")], "commit K");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    git_repo
        .split_commit_per_hunk_group(&Oid::from(to_split), &head_oid, &Oid::from(base))
        .unwrap();

    // 3 commits above base: commit A + K-part1 + K-part2
    let commits_above_base = test.commits_from_head(base);
    assert_eq!(
        commits_above_base.len(),
        3,
        "expected 3 commits above base (A + 2 split parts)"
    );

    let split1 = test.repo.find_commit(commits_above_base[1]).unwrap();
    let split2 = test.repo.find_commit(commits_above_base[2]).unwrap();
    assert!(
        split1
            .summary()
            .ok()
            .flatten()
            .unwrap_or("")
            .contains("(1/2)"),
        "expected (1/2) in: {}",
        split1.summary().ok().flatten().unwrap_or("")
    );
    assert!(
        split2
            .summary()
            .ok()
            .flatten()
            .unwrap_or("")
            .contains("(2/2)"),
        "expected (2/2) in: {}",
        split2.summary().ok().flatten().unwrap_or("")
    );

    // Intermediate commit (K-part1) should have applied a.txt but not b.txt.
    let k1_oid = commits_above_base[1];
    assert_file_contents!(&test.repo, k1_oid, "a.txt", "A3\n");
    assert_file_contents!(&test.repo, k1_oid, "b.txt", "B\n");

    // Final commit (K-part2) should match K's full state.
    let tip = commits_above_base[2];
    assert_file_contents!(&test.repo, tip, "a.txt", "A3\n");
    assert_file_contents!(&test.repo, tip, "b.txt", "B2\n");
}

#[test]
fn split_per_hunk_group_three_commits_two_groups() {
    // A → K → B, where both A and B are context commits that give K's two hunks
    // different fragmap columns.  A only touches a.txt; B only touches b.txt; K
    // touches both.  Result: K's a.txt hunk goes into the {A,K} cluster and
    // K's b.txt hunk goes into the {K,B} cluster → 2 groups → 2 split commits.
    let test = common::TestRepo::new();

    let base = test.commit_files(&[("a.txt", "A\n"), ("b.txt", "B\n")], "base");
    test.commit_file("a.txt", "A2\n", "commit A");
    let to_split = test.commit_files(&[("a.txt", "A3\n"), ("b.txt", "B2\n")], "commit K");
    test.commit_file("b.txt", "B3\n", "commit B");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    git_repo
        .split_commit_per_hunk_group(&Oid::from(to_split), &head_oid, &Oid::from(base))
        .unwrap();

    // 4 commits above base: A + K-part1 + K-part2 + B' (rebased B)
    let commits_above_base = test.commits_from_head(base);
    assert_eq!(
        commits_above_base.len(),
        4,
        "expected 4 commits above base (A + K1 + K2 + B')"
    );

    let split1 = test.repo.find_commit(commits_above_base[1]).unwrap();
    let split2 = test.repo.find_commit(commits_above_base[2]).unwrap();
    assert!(
        split1
            .summary()
            .ok()
            .flatten()
            .unwrap_or("")
            .contains("(1/2)"),
        "expected (1/2) in: {}",
        split1.summary().ok().flatten().unwrap_or("")
    );
    assert!(
        split2
            .summary()
            .ok()
            .flatten()
            .unwrap_or("")
            .contains("(2/2)"),
        "expected (2/2) in: {}",
        split2.summary().ok().flatten().unwrap_or("")
    );

    // K-part1 should have applied only the a.txt group.
    let k1_oid = commits_above_base[1];
    assert_file_contents!(&test.repo, k1_oid, "a.txt", "A3\n");
    assert_file_contents!(&test.repo, k1_oid, "b.txt", "B\n");

    // K-part2 should match K's full state.
    let k2_oid = commits_above_base[2];
    assert_file_contents!(&test.repo, k2_oid, "a.txt", "A3\n");
    assert_file_contents!(&test.repo, k2_oid, "b.txt", "B2\n");

    // B' (rebased) should have b.txt = B3.
    let b_prime_oid = commits_above_base[3];
    assert_file_contents!(&test.repo, b_prime_oid, "b.txt", "B3\n");
}

#[test]
fn split_per_hunk_group_refuses_single_group() {
    // K is the only branch commit, touching two separate regions of one file.
    // Both hunks have commit_oids={K} in the fragmap; dedup collapses them to
    // 1 column → split is refused.
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "A\nPAD1\nPAD2\nPAD3\nPAD4\nPAD5\nX\n", "base");
    let only_one_group = test.commit_file(
        "a.txt",
        "A2\nPAD1\nPAD2\nPAD3\nPAD4\nPAD5\nX2\n",
        "two changes no context",
    );

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    let result = git_repo.split_commit_per_hunk_group(
        &Oid::from(only_one_group),
        &head_oid,
        &Oid::from(base),
    );
    assert!(
        result.is_err(),
        "should fail when all hunks collapse to 1 hunk group"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("fewer than 2 hunk groups"),
        "unexpected error message: {}",
        msg
    );
}

#[test]
fn split_per_hunk_pure_insertions() {
    // Commit that only inserts lines (old_count == 0 hunks).
    // Verifies the off-by-one fix for insertion splice position.
    let test = common::TestRepo::new();

    let base = test.commit_file(
        "a.txt",
        "line1\nline2\nline3\nPAD1\nPAD2\nPAD3\nPAD4\nPAD5\nline6\n",
        "base",
    );

    // Insert "NEW\n" after line1 and "OTHER\n" after line6 — two separate pure-insertion hunks.
    let to_split = test.commit_file(
        "a.txt",
        "line1\nNEW\nline2\nline3\nPAD1\nPAD2\nPAD3\nPAD4\nPAD5\nline6\nOTHER\n",
        "two insertions",
    );

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    git_repo
        .split_commit_per_hunk(&Oid::from(to_split), &head_oid)
        .unwrap();

    let commits_above_base = test.commits_from_head(base);
    assert_eq!(commits_above_base.len(), 2, "expected 2 split commits");

    // First split commit: only the first insertion applied.
    let tree1 = test
        .repo
        .find_commit(commits_above_base[0])
        .unwrap()
        .tree()
        .unwrap();
    let blob1 = tree1.get_path(std::path::Path::new("a.txt")).unwrap();
    let b1 = test.repo.find_blob(blob1.id()).unwrap();
    assert_eq!(
        std::str::from_utf8(b1.content()).unwrap(),
        "line1\nNEW\nline2\nline3\nPAD1\nPAD2\nPAD3\nPAD4\nPAD5\nline6\n",
        "first split commit should have only the first insertion"
    );

    // Final commit: both insertions.
    assert_file_contents!(
        &test.repo,
        *commits_above_base.last().unwrap(),
        "a.txt",
        "line1\nNEW\nline2\nline3\nPAD1\nPAD2\nPAD3\nPAD4\nPAD5\nline6\nOTHER\n"
    );
}

#[test]
fn split_per_hunk_group_preserves_commit_message_body() {
    let test = common::TestRepo::new();

    let base = test.commit_files(&[("a.txt", "A\n"), ("b.txt", "B\n")], "base");
    test.commit_file("a.txt", "A2\n", "commit A");
    let to_split = test.commit_files(
        &[("a.txt", "A3\n"), ("b.txt", "B2\n")],
        "commit K\n\nThis body should survive the split.",
    );

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();
    git_repo
        .split_commit_per_hunk_group(&Oid::from(to_split), &head_oid, &Oid::from(base))
        .unwrap();

    let commits_above_base = test.commits_from_head(base);
    // commit A + 2 split parts
    assert_eq!(commits_above_base.len(), 3);
    for oid in &commits_above_base[1..] {
        let commit = test.repo.find_commit(*oid).unwrap();
        let msg = commit.message().unwrap_or("");
        assert!(
            msg.contains("This body should survive the split."),
            "expected body in split commit, got: {:?}",
            msg
        );
    }
}
