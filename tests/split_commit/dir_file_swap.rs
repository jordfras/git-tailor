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

//! Splitting a commit that replaces a directory with a file, or a file with a
//! directory. libgit2 reports that as a deletion and an addition at colliding
//! paths, and no tree can hold both sides, so the swap moves as one: picking
//! any part of it takes all of it.
//!
//! Every commit also changes `0.txt` and `b.txt`, which sort on either side of
//! the swap: the index keeps a replaced directory's files when an entry sorts
//! before the file replacing it, and the swap then sits in a middle piece,
//! where the original tree does not paper over it.

use std::path::{Path, PathBuf};

use crate::common;
use crate::common::prelude::*;

const TEXT: &str = "x\n";
const BINARY: &str = "\0x\0";

fn commit_all(test: &common::TestRepo, message: &str) -> git2::Oid {
    let mut index = test.repo.index().unwrap();
    index.read(true).unwrap();
    index
        .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    index.update_all(["*"], None).unwrap();
    index.write().unwrap();
    test.commit(message)
}

/// A commit that replaces directory `a/` (holding `a/x` with `x`) with file
/// `a`, and changes `0.txt` and `b.txt`.
fn commit_directory_becoming_a_file(test: &common::TestRepo, x: &str) -> (git2::Oid, git2::Oid) {
    let base = test.commit_files(&[("0.txt", "01\n"), ("a/x", x), ("b.txt", "b1\n")], "base");
    std::fs::remove_dir_all(test.repo.workdir().unwrap().join("a")).unwrap();
    test.write_file("a", "a\n");
    test.write_file("0.txt", "02\n");
    test.write_file("b.txt", "b2\n");
    (base, commit_all(test, "change"))
}

/// A commit that replaces file `a` (holding `x`) with directory `a/`, and
/// changes `0.txt` and `b.txt`.
fn commit_file_becoming_a_directory(test: &common::TestRepo, x: &str) -> (git2::Oid, git2::Oid) {
    let base = test.commit_files(&[("0.txt", "01\n"), ("a", x), ("b.txt", "b1\n")], "base");
    std::fs::remove_file(test.repo.workdir().unwrap().join("a")).unwrap();
    test.write_file("a/x", "a\n");
    test.write_file("0.txt", "02\n");
    test.write_file("b.txt", "b2\n");
    (base, commit_all(test, "change"))
}

/// What each commit from `base` up to HEAD changes, oldest first, as
/// "A path", "D path" or "M path".
fn changes_per_piece(test: &common::TestRepo, base: git2::Oid) -> Vec<Vec<String>> {
    test.commits_from_head(base)
        .into_iter()
        .map(|oid| {
            let commit = test.repo.find_commit(oid).unwrap();
            let parent_tree = commit.parent(0).unwrap().tree().unwrap();
            let diff = test
                .repo
                .diff_tree_to_tree(Some(&parent_tree), Some(&commit.tree().unwrap()), None)
                .unwrap();
            diff.deltas()
                .map(|delta| {
                    let path = delta.new_file().path().or(delta.old_file().path());
                    let status = match delta.status() {
                        git2::Delta::Added => 'A',
                        git2::Delta::Deleted => 'D',
                        _ => 'M',
                    };
                    format!("{status} {}", path.unwrap().display())
                })
                .collect()
        })
        .collect()
}

/// The first hunk of `path` in `to_split`, as split out hunks numbers it.
fn hunk_of(test: &common::TestRepo, to_split: git2::Oid, path: &str) -> (usize, usize) {
    let commit = test.repo.find_commit(to_split).unwrap();
    let parent_tree = commit.parent(0).unwrap().tree().unwrap();
    let mut opts = git2::DiffOptions::new();
    opts.context_lines(3);
    let diff = test
        .repo
        .diff_tree_to_tree(
            Some(&parent_tree),
            Some(&commit.tree().unwrap()),
            Some(&mut opts),
        )
        .unwrap();
    let delta_idx = diff
        .deltas()
        .position(|delta| {
            delta.new_file().path().or(delta.old_file().path()) == Some(Path::new(path))
        })
        .unwrap();
    (delta_idx, 0)
}

/// A commit that replaces file `a` with directory `a/` holding `a/x` and
/// `a/y`, and changes `0.txt` and `b.txt`.
fn commit_file_becoming_a_directory_of_two(test: &common::TestRepo) -> (git2::Oid, git2::Oid) {
    let base = test.commit_files(&[("0.txt", "01\n"), ("a", TEXT), ("b.txt", "b1\n")], "base");
    std::fs::remove_file(test.repo.workdir().unwrap().join("a")).unwrap();
    test.write_file("a/x", "ax\n");
    test.write_file("a/y", "ay\n");
    test.write_file("0.txt", "02\n");
    test.write_file("b.txt", "b2\n");
    (base, commit_all(test, "change"))
}

/// Ten lines, the last one `last`: enough for rename detection to pair two
/// versions that differ only there.
fn ten_lines(last: &str) -> String {
    let mut text: String = (1..10).map(|n| format!("line {n}\n")).collect();
    text.push_str(last);
    text
}

/// Whether one piece holds every change in `changes`.
fn one_piece_holds(pieces: &[Vec<String>], changes: &[&str]) -> bool {
    pieces.iter().any(|piece| {
        changes
            .iter()
            .all(|change| piece.iter().any(|c| c == change))
    })
}

/// Split per hunk group, with a later commit changing `b.txt` so that it forms
/// a group apart from the swap, check the pieces match the count and none is
/// empty, and return them.
fn assert_per_hunk_group_has_no_empty_piece(
    test: &common::TestRepo,
    base: git2::Oid,
    to_split: git2::Oid,
) -> Vec<Vec<String>> {
    let head = test.commit_file("b.txt", "b3\n", "later");
    let mut git_repo = test.git_repo();
    let (commit, head, base_oid) = (Oid::from(to_split), Oid::from(head), Oid::from(base));
    let count = git_repo
        .count_split_per_hunk_group(&commit, &head, &base_oid)
        .unwrap();
    git_repo
        .split_commit_per_hunk_group(&commit, &head, &base_oid)
        .unwrap();

    let mut pieces = changes_per_piece(test, base);
    pieces.pop();
    assert_eq!(pieces.len(), count, "{pieces:?}");
    assert!(pieces.iter().all(|p| !p.is_empty()), "{pieces:?}");
    pieces
}

#[test]
fn split_per_file_replaces_a_directory_with_a_file_in_one_piece() {
    let test = common::TestRepo::new();
    let (base, to_split) = commit_directory_becoming_a_file(&test, TEXT);

    let commit = Oid::from(to_split);
    test.git_repo()
        .split_commit_per_file(&commit, &commit)
        .unwrap();

    assert_eq!(
        changes_per_piece(&test, base),
        [vec!["M 0.txt"], vec!["A a", "D a/x"], vec!["M b.txt"]]
    );
}

#[test]
fn split_per_file_replaces_a_file_with_a_directory_in_one_piece() {
    let test = common::TestRepo::new();
    let (base, to_split) = commit_file_becoming_a_directory(&test, TEXT);

    let commit = Oid::from(to_split);
    test.git_repo()
        .split_commit_per_file(&commit, &commit)
        .unwrap();

    assert_eq!(
        changes_per_piece(&test, base),
        [vec!["M 0.txt"], vec!["D a", "A a/x"], vec!["M b.txt"]]
    );
}

#[test]
fn split_per_hunk_replaces_a_directory_with_a_file_in_one_piece() {
    let test = common::TestRepo::new();
    let (base, to_split) = commit_directory_becoming_a_file(&test, TEXT);

    let commit = Oid::from(to_split);
    test.git_repo()
        .split_commit_per_hunk(&commit, &commit)
        .unwrap();

    assert_eq!(
        changes_per_piece(&test, base),
        [vec!["M 0.txt"], vec!["A a", "D a/x"], vec!["M b.txt"]]
    );
}

#[test]
fn split_per_hunk_replaces_a_binary_file_with_a_directory_in_one_piece() {
    let test = common::TestRepo::new();
    let (base, to_split) = commit_file_becoming_a_directory(&test, BINARY);

    let commit = Oid::from(to_split);
    test.git_repo()
        .split_commit_per_hunk(&commit, &commit)
        .unwrap();

    assert_eq!(
        changes_per_piece(&test, base),
        [vec!["M 0.txt"], vec!["D a", "A a/x"], vec!["M b.txt"]]
    );
}

#[test]
fn split_per_hunk_group_makes_no_empty_piece_when_a_file_replaces_a_directory() {
    let test = common::TestRepo::new();
    let (base, to_split) = commit_directory_becoming_a_file(&test, TEXT);
    let pieces = assert_per_hunk_group_has_no_empty_piece(&test, base, to_split);
    assert!(one_piece_holds(&pieces, &["A a", "D a/x"]), "{pieces:?}");
}

#[test]
fn split_per_hunk_group_makes_no_empty_piece_when_a_directory_replaces_a_binary_file() {
    let test = common::TestRepo::new();
    let (base, to_split) = commit_file_becoming_a_directory(&test, BINARY);
    let pieces = assert_per_hunk_group_has_no_empty_piece(&test, base, to_split);
    assert!(one_piece_holds(&pieces, &["D a", "A a/x"]), "{pieces:?}");
}

#[test]
fn split_out_hunks_leaves_a_directory_replaced_by_a_file_in_the_remainder() {
    let test = common::TestRepo::new();
    let (base, to_split) = commit_directory_becoming_a_file(&test, TEXT);

    let commit = Oid::from(to_split);
    test.git_repo()
        .split_commit_out_hunks(&commit, &[hunk_of(&test, to_split, "b.txt")], &commit, 3)
        .unwrap();

    assert_eq!(
        changes_per_piece(&test, base),
        [vec!["M 0.txt", "A a", "D a/x"], vec!["M b.txt"]]
    );
}

#[test]
fn split_out_hunks_takes_the_directory_a_picked_file_replaces() {
    let test = common::TestRepo::new();
    let (base, to_split) = commit_directory_becoming_a_file(&test, TEXT);

    let commit = Oid::from(to_split);
    test.git_repo()
        .split_commit_out_hunks(&commit, &[hunk_of(&test, to_split, "a")], &commit, 3)
        .unwrap();

    assert_eq!(
        changes_per_piece(&test, base),
        [vec!["M 0.txt", "M b.txt"], vec!["A a", "D a/x"]]
    );
}

#[test]
fn split_out_files_takes_the_directory_a_picked_file_replaces() {
    let test = common::TestRepo::new();
    let (base, to_split) = commit_directory_becoming_a_file(&test, TEXT);

    let commit = Oid::from(to_split);
    test.git_repo()
        .split_commit_out_files(&commit, &[PathBuf::from("a")], &commit)
        .unwrap();

    assert_eq!(
        changes_per_piece(&test, base),
        [vec!["M 0.txt", "M b.txt"], vec!["A a", "D a/x"]]
    );
}

#[test]
fn split_out_files_takes_the_file_a_picked_directory_replaces() {
    let test = common::TestRepo::new();
    let (base, to_split) = commit_file_becoming_a_directory(&test, TEXT);

    let commit = Oid::from(to_split);
    test.git_repo()
        .split_commit_out_files(&commit, &[PathBuf::from("a/x")], &commit)
        .unwrap();

    assert_eq!(
        changes_per_piece(&test, base),
        [vec!["M 0.txt", "M b.txt"], vec!["D a", "A a/x"]]
    );
}

#[test]
fn split_out_hunks_takes_the_whole_swap_for_the_directory_a_file_replaces() {
    let test = common::TestRepo::new();
    let (base, to_split) = commit_directory_becoming_a_file(&test, TEXT);

    let commit = Oid::from(to_split);
    test.git_repo()
        .split_commit_out_hunks(&commit, &[hunk_of(&test, to_split, "a/x")], &commit, 3)
        .unwrap();

    assert_eq!(
        changes_per_piece(&test, base),
        [vec!["M 0.txt", "M b.txt"], vec!["A a", "D a/x"]]
    );
}

#[test]
fn split_out_hunks_takes_the_whole_swap_for_the_file_a_directory_replaces() {
    let test = common::TestRepo::new();
    let (base, to_split) = commit_file_becoming_a_directory(&test, TEXT);

    let commit = Oid::from(to_split);
    test.git_repo()
        .split_commit_out_hunks(&commit, &[hunk_of(&test, to_split, "a")], &commit, 3)
        .unwrap();

    assert_eq!(
        changes_per_piece(&test, base),
        [vec!["M 0.txt", "M b.txt"], vec!["D a", "A a/x"]]
    );
}

#[test]
fn split_out_hunks_takes_the_whole_swap_beside_other_picked_hunks() {
    let test = common::TestRepo::new();
    let (base, to_split) = commit_directory_becoming_a_file(&test, TEXT);

    let commit = Oid::from(to_split);
    let picked = [
        hunk_of(&test, to_split, "a/x"),
        hunk_of(&test, to_split, "b.txt"),
    ];
    test.git_repo()
        .split_commit_out_hunks(&commit, &picked, &commit, 3)
        .unwrap();

    assert_eq!(
        changes_per_piece(&test, base),
        [vec!["M 0.txt"], vec!["A a", "D a/x", "M b.txt"]]
    );
}

#[test]
fn split_out_hunks_takes_every_file_of_a_directory_that_replaced_a_file() {
    let test = common::TestRepo::new();
    let (base, to_split) = commit_file_becoming_a_directory_of_two(&test);

    let commit = Oid::from(to_split);
    test.git_repo()
        .split_commit_out_hunks(&commit, &[hunk_of(&test, to_split, "a/y")], &commit, 3)
        .unwrap();

    assert_eq!(
        changes_per_piece(&test, base),
        [vec!["M 0.txt", "M b.txt"], vec!["D a", "A a/x", "A a/y"]]
    );
}

#[test]
fn split_out_files_takes_the_whole_swap_for_the_directory_a_file_replaces() {
    let test = common::TestRepo::new();
    let (base, to_split) = commit_directory_becoming_a_file(&test, TEXT);

    let commit = Oid::from(to_split);
    test.git_repo()
        .split_commit_out_files(&commit, &[PathBuf::from("a/x")], &commit)
        .unwrap();

    assert_eq!(
        changes_per_piece(&test, base),
        [vec!["M 0.txt", "M b.txt"], vec!["A a", "D a/x"]]
    );
}

#[test]
fn split_out_files_takes_the_whole_swap_for_the_file_a_directory_replaces() {
    let test = common::TestRepo::new();
    let (base, to_split) = commit_file_becoming_a_directory(&test, TEXT);

    let commit = Oid::from(to_split);
    test.git_repo()
        .split_commit_out_files(&commit, &[PathBuf::from("a")], &commit)
        .unwrap();

    assert_eq!(
        changes_per_piece(&test, base),
        [vec!["M 0.txt", "M b.txt"], vec!["D a", "A a/x"]]
    );
}

#[test]
fn split_out_files_takes_every_file_of_a_directory_that_replaced_a_file() {
    let test = common::TestRepo::new();
    let (base, to_split) = commit_file_becoming_a_directory_of_two(&test);

    let commit = Oid::from(to_split);
    test.git_repo()
        .split_commit_out_files(&commit, &[PathBuf::from("a/x")], &commit)
        .unwrap();

    assert_eq!(
        changes_per_piece(&test, base),
        [vec!["M 0.txt", "M b.txt"], vec!["D a", "A a/x", "A a/y"]]
    );
}

#[test]
fn split_per_file_keeps_every_file_of_a_directory_that_replaced_a_file_in_one_piece() {
    let test = common::TestRepo::new();
    let (base, to_split) = commit_file_becoming_a_directory_of_two(&test);

    let commit = Oid::from(to_split);
    test.git_repo()
        .split_commit_per_file(&commit, &commit)
        .unwrap();

    assert_eq!(
        changes_per_piece(&test, base),
        [
            vec!["M 0.txt"],
            vec!["D a", "A a/x", "A a/y"],
            vec!["M b.txt"]
        ]
    );
}

#[test]
fn split_per_hunk_group_keeps_a_file_renamed_over_a_directory_with_it() {
    let test = common::TestRepo::new();
    let base = test.commit_files(
        &[
            ("0.txt", "01\n"),
            ("d/y", "y\n"),
            ("b.txt", "b1\n"),
            ("z", &ten_lines("old\n")),
        ],
        "base",
    );
    let workdir = test.repo.workdir().unwrap().to_path_buf();
    std::fs::remove_dir_all(workdir.join("d")).unwrap();
    std::fs::remove_file(workdir.join("z")).unwrap();
    test.write_file("d", &ten_lines("new\n"));
    test.write_file("0.txt", "02\n");
    test.write_file("b.txt", "b2\n");
    let to_split = commit_all(&test, "change");

    let pieces = assert_per_hunk_group_has_no_empty_piece(&test, base, to_split);
    assert!(
        one_piece_holds(&pieces, &["A d", "D d/y", "D z"]),
        "{pieces:?}"
    );
}

#[test]
fn split_per_hunk_group_keeps_a_file_renamed_out_of_a_directory_a_file_replaces_with_it() {
    let test = common::TestRepo::new();
    let base = test.commit_files(
        &[
            ("0.txt", "01\n"),
            ("d/x", &ten_lines("old\n")),
            ("b.txt", "b1\n"),
        ],
        "base",
    );
    std::fs::remove_dir_all(test.repo.workdir().unwrap().join("d")).unwrap();
    test.write_file("d", "d\n");
    test.write_file("e/x", &ten_lines("new\n"));
    test.write_file("0.txt", "02\n");
    test.write_file("b.txt", "b2\n");
    let to_split = commit_all(&test, "change");

    let pieces = assert_per_hunk_group_has_no_empty_piece(&test, base, to_split);
    assert!(
        one_piece_holds(&pieces, &["A d", "D d/x", "A e/x"]),
        "{pieces:?}"
    );
}
