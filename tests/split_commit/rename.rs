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

//! Regression tests for split-per-hunk-group across a commit that both
//! renames a file and edits it.
//!
//! `assign_hunk_groups` internally keys its per-file grouping by each file's
//! canonical (earliest) name, needed to attribute a renamed file's hunks
//! across its history — but its public output must be keyed by the split
//! commit's own path, and the tree surgery that actually builds the split
//! pieces must resolve content at the pre-rename path in the parent tree and
//! drop the stale old-path entry, or the rename is silently mishandled.

use crate::common;
use crate::common::prelude::*;

fn make_content() -> String {
    (1..=20).map(|i| format!("line {i}\n")).collect()
}

#[test]
fn split_per_hunk_group_rename_and_edit_in_the_same_commit() {
    // History:
    //   base — creates foo.txt (20 lines)
    //   A    — edits line 1 of foo.txt
    //   K    — renames foo.txt -> bar.txt, reworks A's line 1 AND edits the
    //          unrelated line 20, both in the same commit  <-- split target
    let test = common::TestRepo::new();

    let content = make_content();
    let base = test.commit_file("foo.txt", &content, "base");

    let a_content = content.replacen("line 1\n", "line 1 edited by A\n", 1);
    test.commit_file("foo.txt", &a_content, "A: edit line 1");

    let k_content = a_content
        .replacen("line 1 edited by A\n", "line 1 reworked by K\n", 1)
        .replacen("line 20\n", "line 20 edited by K\n", 1);
    test.rename_file(
        "foo.txt",
        "bar.txt",
        Some(&k_content),
        "K: rename + rework both lines",
    );
    let to_split = test.repo.head().unwrap().target().unwrap();

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    assert_eq!(
        git_repo
            .count_split_per_hunk_group(&Oid::from(to_split), &head_oid, &Oid::from(base))
            .unwrap(),
        2,
        "line 1 (A-related) and line 20 (unrelated) are separable groups"
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

    // Each split piece must carry ONLY bar.txt (the rename applied), never a
    // leftover foo.txt from the parent tree.
    for &oid in &commits_above_base[1..] {
        let tree = test.repo.find_commit(oid).unwrap().tree().unwrap();
        assert!(
            tree.get_path(std::path::Path::new("bar.txt")).is_ok(),
            "split piece {oid} should contain bar.txt"
        );
        assert!(
            tree.get_path(std::path::Path::new("foo.txt")).is_err(),
            "split piece {oid} should not still contain foo.txt"
        );
    }

    // The final piece matches K's full state.
    let tip = commits_above_base[2];
    assert_file_contents!(&test.repo, tip, "bar.txt", k_content);
}

#[test]
fn split_per_hunk_group_survives_a_two_hop_rename_chain() {
    // History:
    //   base  — creates alpha.py (20 lines)
    //   A     — edits line 1 of alpha.py
    //   rename1 — alpha.py -> beta.py
    //   mid   — edits an unrelated line of beta.py
    //   rename2 — beta.py -> gamma.py
    //   K     — reworks A's line (reached through BOTH renames) and edits a
    //           fresh line, under the file's third name  <-- split target
    //
    // `assign_hunk_groups` collapses a rename chain to one canonical
    // (earliest) key via `build_rename_map`'s transitive lookup; this
    // exercises that collapsing over two hops instead of one, plus the
    // re-keying back to K's own current path (gamma.py) added alongside the
    // single-rename fix.
    let test = common::TestRepo::new();

    let content = make_content();
    let base = test.commit_file("alpha.py", &content, "base");

    let a_content = content.replacen("line 1\n", "line 1 edited by A\n", 1);
    test.commit_file("alpha.py", &a_content, "A: edit line 1");

    test.rename_file("alpha.py", "beta.py", None, "rename1: alpha.py -> beta.py");

    let mid_content = a_content.replacen("line 10\n", "line 10 edited by mid\n", 1);
    test.commit_file("beta.py", &mid_content, "mid: edit an unrelated line");

    test.rename_file("beta.py", "gamma.py", None, "rename2: beta.py -> gamma.py");

    let k_content = mid_content
        .replacen("line 1 edited by A\n", "line 1 reworked by K\n", 1)
        .replacen("line 20\n", "line 20 edited by K\n", 1);
    test.commit_file(
        "gamma.py",
        &k_content,
        "K: rework A's line through 2 renames",
    );
    let to_split = test.repo.head().unwrap().target().unwrap();

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    assert_eq!(
        git_repo
            .count_split_per_hunk_group(&Oid::from(to_split), &head_oid, &Oid::from(base))
            .unwrap(),
        2,
        "line 1 (A-related, through 2 renames) and line 20 (unrelated) are separable"
    );

    git_repo
        .split_commit_per_hunk_group(&Oid::from(to_split), &head_oid, &Oid::from(base))
        .unwrap();

    // 6 commits above base: A + rename1 + mid + rename2 + 2 split parts.
    let commits_above_base = test.commits_from_head(base);
    assert_eq!(
        commits_above_base.len(),
        6,
        "expected 6 commits above base (A + rename1 + mid + rename2 + 2 split parts)"
    );

    // Each split piece must carry ONLY gamma.py, never a leftover alpha.py or
    // beta.py from either rename hop.
    for &oid in &commits_above_base[4..] {
        let tree = test.repo.find_commit(oid).unwrap().tree().unwrap();
        assert!(
            tree.get_path(std::path::Path::new("gamma.py")).is_ok(),
            "split piece {oid} should contain gamma.py"
        );
        assert!(
            tree.get_path(std::path::Path::new("alpha.py")).is_err(),
            "split piece {oid} should not still contain alpha.py"
        );
        assert!(
            tree.get_path(std::path::Path::new("beta.py")).is_err(),
            "split piece {oid} should not still contain beta.py"
        );
    }

    // The final piece matches K's full state.
    let tip = commits_above_base[5];
    assert_file_contents!(&test.repo, tip, "gamma.py", k_content);
}
