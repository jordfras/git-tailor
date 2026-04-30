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

use crate::assert_file_contents;
use crate::common;
use git_tailor::{Oid, repo::GitRepo};

#[test]
fn split_per_hunk_group_multi_path_two_groups() {
    // Commit A modifies a.txt line 2 and c.txt line 1.
    // Commit K modifies a.txt lines 2-4 (overlapping A *and* extending beyond)
    // and c.txt line 1 (overlapping A).
    //
    // In the SPG for a.txt, K's active node has two predecessors (A's active
    // node and a surviving span), creating two paths with different commit_oids:
    //   path 1: {A, K}   path 2: {K}
    //
    // c.txt has only one path: {A, K}.
    //
    // After dedup there are 2 groups: {A,K} and {K}.
    //
    // Without coverage-aware assignment, K's a.txt hunk would be assigned to
    // the {A,K} group (first cluster wins) and c.txt's hunk also goes to
    // {A,K}. The {K} group would have zero hunks → the split would refuse
    // with "fewer than 2 hunk groups".
    //
    // The fix ensures K's a.txt hunk is reassigned to {K} so both groups are
    // covered, producing 2 split commits.
    let test = common::TestRepo::new();

    let base = test.commit_files(
        &[("a.txt", "L1\nL2\nL3\nL4\nL5\n"), ("c.txt", "C1\n")],
        "base",
    );
    test.commit_files(
        &[("a.txt", "L1\nA2\nL3\nL4\nL5\n"), ("c.txt", "CA1\n")],
        "commit A",
    );
    let to_split = test.commit_files(
        &[("a.txt", "L1\nK2\nK3\nK4\nL5\n"), ("c.txt", "CK1\n")],
        "commit K",
    );

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    git_repo
        .split_commit_per_hunk_group(&Oid::from(to_split), &head_oid, &Oid::from(base))
        .unwrap();

    // 3 commits above base: A' + K-part1 + K-part2
    let commits_above_base = test.commits_from_head(base);
    assert_eq!(
        commits_above_base.len(),
        3,
        "expected 3 commits above base (A + 2 split parts), got {}",
        commits_above_base.len()
    );

    let split1 = test.repo.find_commit(commits_above_base[1]).unwrap();
    let split2 = test.repo.find_commit(commits_above_base[2]).unwrap();
    assert!(
        split1.summary().unwrap_or("").contains("(1/2)"),
        "expected (1/2) in: {}",
        split1.summary().unwrap_or("")
    );
    assert!(
        split2.summary().unwrap_or("").contains("(2/2)"),
        "expected (2/2) in: {}",
        split2.summary().unwrap_or("")
    );

    // Final split commit must match K's full tree.
    let tip = *commits_above_base.last().unwrap();
    assert_file_contents!(&test.repo, tip, "a.txt", "L1\nK2\nK3\nK4\nL5\n");
    assert_file_contents!(&test.repo, tip, "c.txt", "CK1\n");
}

#[test]
fn split_per_hunk_group_multi_path_three_groups() {
    // Three context commits create three distinct fragmap groups for K.
    //
    //   A modifies a.txt line 2 and c.txt line 1.
    //   K modifies a.txt lines 2-4 (overlap + extension), b.txt line 1, c.txt.
    //   B (after K) modifies b.txt line 1 (overlaps K).
    //
    // SPG produces:
    //   a.txt: two paths {A,K} and {K} — K's hunk is on both (multi-path)
    //   b.txt: one path {K,B}
    //   c.txt: one path {A,K}
    //
    // Dedup groups: {A,K}=0, {K}=1, {K,B}=2.
    // Without coverage-aware assignment: K's a.txt hunk → group 0 (first-wins),
    // c.txt → group 0, b.txt → group 2.  Group 1 ({K}) has no hunk → only 2
    // commits instead of 3.
    //
    // Coverage-aware fix: a.txt hunk reassigned to group 1, giving 3 commits.
    let test = common::TestRepo::new();

    let base = test.commit_files(
        &[
            ("a.txt", "L1\nL2\nL3\nL4\nL5\n"),
            ("b.txt", "B1\n"),
            ("c.txt", "C1\n"),
        ],
        "base",
    );
    test.commit_files(
        &[("a.txt", "L1\nA2\nL3\nL4\nL5\n"), ("c.txt", "CA1\n")],
        "commit A",
    );
    let to_split = test.commit_files(
        &[
            ("a.txt", "L1\nK2\nK3\nK4\nL5\n"),
            ("b.txt", "KB1\n"),
            ("c.txt", "CK1\n"),
        ],
        "commit K",
    );
    test.commit_file("b.txt", "BB1\n", "commit B");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    git_repo
        .split_commit_per_hunk_group(&Oid::from(to_split), &head_oid, &Oid::from(base))
        .unwrap();

    // 5 commits above base: A' + K-part1 + K-part2 + K-part3 + B'
    let commits_above_base = test.commits_from_head(base);
    assert_eq!(
        commits_above_base.len(),
        5,
        "expected 5 commits above base (A + 3 split parts + B'), got {}",
        commits_above_base.len()
    );

    let split1 = test.repo.find_commit(commits_above_base[1]).unwrap();
    let split3 = test.repo.find_commit(commits_above_base[3]).unwrap();
    assert!(
        split1.summary().unwrap_or("").contains("(1/3)"),
        "expected (1/3) in: {}",
        split1.summary().unwrap_or("")
    );
    assert!(
        split3.summary().unwrap_or("").contains("(3/3)"),
        "expected (3/3) in: {}",
        split3.summary().unwrap_or("")
    );

    // Last split commit (K-part3) must match K's full tree.
    let k3_oid = commits_above_base[3];
    assert_file_contents!(&test.repo, k3_oid, "a.txt", "L1\nK2\nK3\nK4\nL5\n");
    assert_file_contents!(&test.repo, k3_oid, "b.txt", "KB1\n");
    assert_file_contents!(&test.repo, k3_oid, "c.txt", "CK1\n");

    // Rebased B' should have its b.txt content.
    let b_prime_oid = commits_above_base[4];
    assert_file_contents!(&test.repo, b_prime_oid, "b.txt", "BB1\n");
}
