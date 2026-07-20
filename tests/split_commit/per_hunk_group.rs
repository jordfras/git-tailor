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
fn split_per_hunk_group_one_hunk_spanning_two_columns() {
    // Commit A edits line 1, commit B edits line 2, and commit K rewrites both
    // lines in ONE 0-context hunk.  In the fragmap K's hunk lies on two SPG
    // paths — {A,K} through A's region and {B,K} through B's region — so the
    // matrix shows K in two columns.  The split must track the hunk's parts
    // and produce two commits, one per column, instead of refusing because
    // the whole hunk can only be assigned to one group.
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "A\nB\n", "base");
    test.commit_file("a.txt", "A2\nB\n", "commit A");
    test.commit_file("a.txt", "A2\nB2\n", "commit B");
    let to_split = test.commit_file("a.txt", "A3\nB3\n", "commit K");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    assert_eq!(
        git_repo
            .count_split_per_hunk_group(&Oid::from(to_split), &head_oid, &Oid::from(base))
            .unwrap(),
        2,
        "the count offered to the user must match the two matrix columns"
    );

    git_repo
        .split_commit_per_hunk_group(&Oid::from(to_split), &head_oid, &Oid::from(base))
        .unwrap();

    // 4 commits above base: A + B + K-part1 + K-part2.
    let commits_above_base = test.commits_from_head(base);
    assert_eq!(
        commits_above_base.len(),
        4,
        "expected 4 commits above base (A + B + 2 split parts)"
    );

    // K-part1 carries only the fragment from A's column (line 1).
    let k1_oid = commits_above_base[2];
    assert_file_contents!(&test.repo, k1_oid, "a.txt", "A3\nB2\n");

    // K-part2 completes the commit.
    let k2_oid = commits_above_base[3];
    assert_file_contents!(&test.repo, k2_oid, "a.txt", "A3\nB3\n");
}

#[test]
fn split_per_hunk_group_keeps_a_mixed_relation_hunk_whole() {
    // K's a.txt hunk rewrites A's line 1 and B's line 2 in one hunk, and its
    // b.txt hunk relates to nothing else.  Hunks are grouped whole by their
    // relation set — the a.txt hunk's set is {A,B} — and are only ever cut
    // when the split would otherwise be refused.  Two groups exist here, so
    // the mixed hunk stays intact: slicing it would interleave the resulting
    // commits, and the matrix would then relate them to each other.
    let test = common::TestRepo::new();

    let base = test.commit_files(&[("a.txt", "A\nB\n"), ("b.txt", "X\n")], "base");
    test.commit_file("a.txt", "A2\nB\n", "commit A");
    test.commit_file("a.txt", "A2\nB2\n", "commit B");
    let to_split = test.commit_files(&[("a.txt", "A3\nB3\n"), ("b.txt", "X2\n")], "commit K");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    assert_eq!(
        git_repo
            .count_split_per_hunk_group(&Oid::from(to_split), &head_oid, &Oid::from(base))
            .unwrap(),
        2,
        "two relation sets: {{A,B}} and K-only"
    );

    git_repo
        .split_commit_per_hunk_group(&Oid::from(to_split), &head_oid, &Oid::from(base))
        .unwrap();

    // 4 commits above base: A + B + 2 split parts.
    let commits_above_base = test.commits_from_head(base);
    assert_eq!(
        commits_above_base.len(),
        4,
        "expected 4 commits above base (A + B + 2 split parts)"
    );

    // Part 1: the whole a.txt hunk (relation set {A,B}).
    let k1_oid = commits_above_base[2];
    assert_file_contents!(&test.repo, k1_oid, "a.txt", "A3\nB3\n");
    assert_file_contents!(&test.repo, k1_oid, "b.txt", "X\n");

    // Part 2: completes the commit with the unrelated b.txt change.
    let k2_oid = commits_above_base[3];
    assert_file_contents!(&test.repo, k2_oid, "a.txt", "A3\nB3\n");
    assert_file_contents!(&test.repo, k2_oid, "b.txt", "X2\n");
}

#[test]
fn split_per_hunk_group_one_hunk_partially_consumed_across_a_gap_commit() {
    // Same shape as the one-hunk-two-columns case, but an unrelated commit T
    // (touching only b.txt) sits between K and the later commit C that edits
    // part of K's output.  In the SPG the gap generation bridges K's hunk with
    // a full-span propagated copy, which must not blur the claim: the hunk
    // still has a separable piece consumed by C, so the matrix's two columns
    // must yield two split commits.
    let test = common::TestRepo::new();

    let base = test.commit_files(&[("a.txt", "A\nB\n"), ("b.txt", "X\n")], "base");
    let to_split = test.commit_file("a.txt", "A2\nB2\n", "commit K");
    test.commit_file("b.txt", "X2\n", "commit T");
    test.commit_file("a.txt", "A2\nB3\n", "commit C");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    assert_eq!(
        git_repo
            .count_split_per_hunk_group(&Oid::from(to_split), &head_oid, &Oid::from(base))
            .unwrap(),
        2,
        "the count offered to the user must match the two matrix columns"
    );

    git_repo
        .split_commit_per_hunk_group(&Oid::from(to_split), &head_oid, &Oid::from(base))
        .unwrap();

    // 4 commits above base: K-part1 + K-part2 + T' + C'.
    let commits_above_base = test.commits_from_head(base);
    assert_eq!(
        commits_above_base.len(),
        4,
        "expected 4 commits above base (2 split parts + T' + C')"
    );

    // K-part1 carries the piece C never touches (line 1).
    let k1_oid = commits_above_base[0];
    assert_file_contents!(&test.repo, k1_oid, "a.txt", "A2\nB\n");

    // K-part2 completes the commit with the piece C later edits (line 2).
    let k2_oid = commits_above_base[1];
    assert_file_contents!(&test.repo, k2_oid, "a.txt", "A2\nB2\n");

    // T and C replay cleanly on top.
    let tip = commits_above_base[3];
    assert_file_contents!(&test.repo, tip, "a.txt", "A2\nB3\n");
    assert_file_contents!(&test.repo, tip, "b.txt", "X2\n");
}

#[test]
fn split_per_hunk_group_cross_related_lines_yield_fewer_commits_than_columns() {
    // The documented "rare occasion" where a hunk cannot be fully pulled
    // apart: K's one hunk rewrites line 1 (earlier edited by A) and line 2
    // (later edited by C).  Each line relates to a different commit in a
    // different direction, and the matrix multiplies the pairings into FOUR
    // columns for K ({A,K}, {A,K,C}, {K}, {K,C}) even though the hunk has
    // only two physically separable pieces.  The split must not fail: it
    // produces the two physically correct commits — fewer than the matrix's
    // columns — and the pieces still compose to the original commit.
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "A\nB\n", "base");
    test.commit_file("a.txt", "A2\nB\n", "commit A");
    let to_split = test.commit_file("a.txt", "A3\nB3\n", "commit K");
    test.commit_file("a.txt", "A3\nB4\n", "commit C");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    assert_eq!(
        git_repo
            .count_split_per_hunk_group(&Oid::from(to_split), &head_oid, &Oid::from(base))
            .unwrap(),
        2,
        "only the two physically separable pieces can become commits"
    );

    git_repo
        .split_commit_per_hunk_group(&Oid::from(to_split), &head_oid, &Oid::from(base))
        .unwrap();

    // 4 commits above base: A + K-part1 + K-part2 + C' (rebased C).
    let commits_above_base = test.commits_from_head(base);
    assert_eq!(
        commits_above_base.len(),
        4,
        "expected 4 commits above base (A + 2 split parts + C')"
    );

    // K-part1 carries the piece related to A (line 1).
    let k1_oid = commits_above_base[1];
    assert_file_contents!(&test.repo, k1_oid, "a.txt", "A3\nB\n");

    // K-part2 completes the commit with the piece C later edits (line 2).
    let k2_oid = commits_above_base[2];
    assert_file_contents!(&test.repo, k2_oid, "a.txt", "A3\nB3\n");

    // C replays cleanly on top.
    let c_prime_oid = commits_above_base[3];
    assert_file_contents!(&test.repo, c_prime_oid, "a.txt", "A3\nB4\n");
}

#[test]
fn split_per_hunk_group_sandwiched_relation_is_still_splittable() {
    // K's one hunk rewrites three lines: line 1 and line 3 both rework a
    // region A produced, and line 2 in between relates to nobody. A cut that
    // only ever compares "the first line" against "the union of everything
    // else" cannot separate this hunk: the union of {line2, line3} is still
    // {A} (line 3 is A-related), so it looks identical to line 1's own
    // pattern and no cut is found — even though the middle line is genuinely
    // unrelated and separable. Each fragment must be routed by its own
    // pattern, not by a prefix/suffix comparison.
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "1\n2\n3\n", "base");
    test.commit_file("a.txt", "1A\n2\n3A\n", "commit A");
    let to_split = test.commit_file("a.txt", "1K\n2K\n3K\n", "commit K");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    assert_eq!(
        git_repo
            .count_split_per_hunk_group(&Oid::from(to_split), &head_oid, &Oid::from(base))
            .unwrap(),
        2,
        "the A-related ends and the unrelated middle are separable"
    );

    git_repo
        .split_commit_per_hunk_group(&Oid::from(to_split), &head_oid, &Oid::from(base))
        .unwrap();

    // 3 commits above base: A + 2 split parts.
    let commits_above_base = test.commits_from_head(base);
    assert_eq!(
        commits_above_base.len(),
        3,
        "expected 3 commits above base (A + 2 split parts)"
    );

    // One piece carries only the A-related ends (line 2 untouched).
    let piece1 = test.repo.find_commit(commits_above_base[1]).unwrap();
    let tree1 = piece1.tree().unwrap();
    let blob1 = tree1.get_path(std::path::Path::new("a.txt")).unwrap();
    let content1 = test.repo.find_blob(blob1.id()).unwrap();
    assert_eq!(
        std::str::from_utf8(content1.content()).unwrap(),
        "1K\n2\n3K\n",
        "the first piece should touch only the A-related lines"
    );

    // The final piece matches K's full state.
    let tip = commits_above_base[2];
    assert_file_contents!(&test.repo, tip, "a.txt", "1K\n2K\n3K\n");
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
