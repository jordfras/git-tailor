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

//! Splitting a commit that moves a file. Every strategy treats the move as one
//! change, so no piece loses the file or holds it under both names, and the
//! pickers' numbering of changes is the numbering the split goes by.

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

    let mut git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    assert_eq!(
        git_repo
            .count_split_per_hunk_group(
                &Oid::from(to_split),
                &head_oid,
                &Oid::from(base),
                &mut |_| true
            )
            .unwrap(),
        2,
        "line 1 (A-related) and line 20 (unrelated) are separable groups"
    );

    git_repo
        .split_commit_per_hunk_group(
            &Oid::from(to_split),
            &head_oid,
            &Oid::from(base),
            &mut |_| true,
        )
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
    // The fragmap carries the file through both renames as one file; this
    // exercises a rename chain of two hops instead of one.
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

    let mut git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    assert_eq!(
        git_repo
            .count_split_per_hunk_group(
                &Oid::from(to_split),
                &head_oid,
                &Oid::from(base),
                &mut |_| true
            )
            .unwrap(),
        2,
        "line 1 (A-related, through 2 renames) and line 20 (unrelated) are separable"
    );

    git_repo
        .split_commit_per_hunk_group(
            &Oid::from(to_split),
            &head_oid,
            &Oid::from(base),
            &mut |_| true,
        )
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

#[test]
fn split_rename_last_piece_has_original_tree() {
    // Cheap regression guard: per-hunk-group commits the last piece from
    // `commit_tree` directly, so this is structural today.
    let test = common::TestRepo::new();

    let content = make_content();
    let base = test.commit_file("foo.txt", &content, "base");
    let a_content = content.replacen("line 1\n", "line 1 edited by A\n", 1);
    test.commit_file("foo.txt", &a_content, "A: edit line 1");
    let k_content = a_content
        .replacen("line 1 edited by A\n", "line 1 reworked by K\n", 1)
        .replacen("line 20\n", "line 20 edited by K\n", 1);
    let to_split = test.rename_file("foo.txt", "bar.txt", Some(&k_content), "K: rename and edit");

    let original_tree = test.tree_id(to_split);

    let mut git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();
    git_repo
        .split_commit_per_hunk_group(
            &Oid::from(to_split),
            &head_oid,
            &Oid::from(base),
            &mut |_| true,
        )
        .unwrap();

    assert_eq!(
        test.head_tree_id(),
        original_tree,
        "the last split piece must reproduce the original commit's tree"
    );
}

/// Moves a.txt to z.txt, with `edit` applied to its twenty lines, and when
/// `other_edit` changes m.txt alongside. Returns the base and the commit.
fn commit_a_move(
    edit: impl Fn(String) -> String,
    other_edit: bool,
) -> (common::TestRepo, git2::Oid, git2::Oid) {
    let test = common::TestRepo::new();
    let base = test.commit_files(&[("a.txt", &make_content()), ("m.txt", "m\n")], "base");
    if other_edit {
        test.write_file("m.txt", "m edited\n");
        test.stage_file("m.txt");
    }
    let moved = test.rename_file("a.txt", "z.txt", Some(&edit(make_content())), "move a");
    (test, base, moved)
}

/// Each commit from `base` to HEAD holds the file under exactly one name.
fn assert_never_lost_or_doubled(test: &common::TestRepo, base: git2::Oid) {
    for oid in test.commits_from_head(base) {
        let tree = test.repo.find_commit(oid).unwrap().tree().unwrap();
        let names: Vec<&str> = ["a.txt", "z.txt"]
            .into_iter()
            .filter(|name| tree.get_path(std::path::Path::new(name)).is_ok())
            .collect();
        assert_eq!(
            names.len(),
            1,
            "piece {oid} holds the moved file as {names:?}"
        );
    }
}

#[test]
fn split_per_file_keeps_a_move_in_one_piece() {
    let (test, base, moved) = commit_a_move(|content| content, true);
    let original_tree = test.tree_id(moved);
    let mut git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    assert_eq!(git_repo.count_split_per_file(&Oid::from(moved)).unwrap(), 2);
    git_repo
        .split_commit_per_file(&Oid::from(moved), &head_oid)
        .unwrap();

    assert_eq!(test.commits_from_head(base).len(), 2);
    assert_never_lost_or_doubled(&test, base);
    assert_eq!(test.head_tree_id(), original_tree);
}

#[test]
fn split_per_hunk_moves_a_file_with_its_first_hunk() {
    let (test, base, moved) = commit_a_move(
        |content| {
            content
                .replacen("line 1\n", "line one\n", 1)
                .replacen("line 20\n", "line twenty\n", 1)
        },
        false,
    );
    let mut git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    assert_eq!(git_repo.count_split_per_hunk(&Oid::from(moved)).unwrap(), 2);
    git_repo
        .split_commit_per_hunk(&Oid::from(moved), &head_oid)
        .unwrap();

    let pieces = test.commits_from_head(base);
    assert_eq!(pieces.len(), 2);
    assert_never_lost_or_doubled(&test, base);
    assert_file_contents!(
        &test.repo,
        pieces[0],
        "z.txt",
        make_content().replacen("line 1\n", "line one\n", 1)
    );
}

#[test]
fn split_per_hunk_gives_a_pure_move_a_piece_of_its_own() {
    let (test, base, moved) = commit_a_move(|content| content, true);
    let mut git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    assert_eq!(git_repo.count_split_per_hunk(&Oid::from(moved)).unwrap(), 2);
    git_repo
        .split_commit_per_hunk(&Oid::from(moved), &head_oid)
        .unwrap();

    assert_eq!(test.commits_from_head(base).len(), 2);
    assert_never_lost_or_doubled(&test, base);
}

#[test]
fn split_out_files_takes_the_whole_move() {
    let (test, base, moved) = commit_a_move(|content| content, true);
    let mut git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    git_repo
        .split_commit_out_files(&Oid::from(moved), &["z.txt".into()], &head_oid)
        .unwrap();

    let pieces = test.commits_from_head(base);
    assert_eq!(pieces.len(), 2);
    assert_never_lost_or_doubled(&test, base);
    assert_file_contents!(&test.repo, pieces[0], "a.txt", make_content());
    assert_file_contents!(&test.repo, pieces[0], "m.txt", "m edited\n");
}

/// The picker lists hunks from `commit_diff`, and the split must take the hunk
/// at the position it lists.
#[test]
fn split_out_hunks_takes_the_hunk_the_picker_lists() {
    let (test, base, moved) = commit_a_move(
        |content| {
            content
                .replacen("line 1\n", "line one\n", 1)
                .replacen("line 20\n", "line twenty\n", 1)
        },
        false,
    );
    let mut git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();
    let diff = git_repo.commit_diff(&Oid::from(moved), 3).unwrap();
    let picked: Vec<(usize, usize)> = diff
        .files
        .iter()
        .enumerate()
        .flat_map(|(delta_idx, file)| {
            file.hunks
                .iter()
                .enumerate()
                .map(move |(hunk_idx, hunk)| (delta_idx, hunk_idx, hunk))
        })
        .filter(|(_, _, hunk)| {
            hunk.lines
                .iter()
                .any(|line| line.content == "line twenty\n")
        })
        .map(|(delta_idx, hunk_idx, _)| (delta_idx, hunk_idx))
        .collect();

    git_repo
        .split_commit_out_hunks(&Oid::from(moved), &picked, &head_oid, 3)
        .unwrap();

    let pieces = test.commits_from_head(base);
    assert_eq!(pieces.len(), 2);
    assert_never_lost_or_doubled(&test, base);
    assert_file_contents!(
        &test.repo,
        pieces[0],
        "z.txt",
        make_content().replacen("line 1\n", "line one\n", 1)
    );
}

/// A staged move takes its file away from where the commit changed it, so the
/// split must not write that path.
#[test]
fn split_refuses_when_a_staged_move_takes_a_file_it_touches() {
    let test = common::TestRepo::new();
    test.commit_files(&[("a.txt", &make_content()), ("m.txt", "m\n")], "base");
    let to_split = test.commit_files(
        &[
            (
                "a.txt",
                &make_content().replacen("line 1\n", "line one\n", 1),
            ),
            ("m.txt", "m2\n"),
        ],
        "edit both",
    );
    let workdir = test.repo.workdir().unwrap();
    std::fs::rename(workdir.join("a.txt"), workdir.join("z.txt")).unwrap();
    let mut index = test.repo.index().unwrap();
    index.remove_path(std::path::Path::new("a.txt")).unwrap();
    index.add_path(std::path::Path::new("z.txt")).unwrap();
    index.write().unwrap();

    let mut git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();
    let err = git_repo
        .split_commit_per_file(&Oid::from(to_split), &head_oid)
        .unwrap_err();

    assert!(err.to_string().contains("a.txt"), "{err:#}");
}
