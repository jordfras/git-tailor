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
//! paths, and no tree can hold both sides, so the deletion travels with the
//! addition that replaces it.

use std::path::PathBuf;

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
/// `a`, and changes `b.txt`. Its deltas: `a` added, `a/x` deleted, `b.txt`.
fn commit_directory_becoming_a_file(test: &common::TestRepo, x: &str) -> (git2::Oid, git2::Oid) {
    let base = test.commit_files(&[("a/x", x), ("b.txt", "b1\n")], "base");
    std::fs::remove_dir_all(test.repo.workdir().unwrap().join("a")).unwrap();
    test.write_file("a", "a\n");
    test.write_file("b.txt", "b2\n");
    (base, commit_all(test, "change"))
}

/// A commit that replaces file `a` (holding `x`) with directory `a/`, and
/// changes `b.txt`. Its deltas: `a` deleted, `a/x` added, `b.txt`.
fn commit_file_becoming_a_directory(test: &common::TestRepo, x: &str) -> (git2::Oid, git2::Oid) {
    let base = test.commit_files(&[("a", x), ("b.txt", "b1\n")], "base");
    std::fs::remove_file(test.repo.workdir().unwrap().join("a")).unwrap();
    test.write_file("a/x", "a\n");
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

/// Split per hunk group and check the pieces match the count and none is
/// empty; with fewer than two groups the split must refuse.
fn assert_per_hunk_group_has_no_empty_piece(
    test: &common::TestRepo,
    base: git2::Oid,
    to_split: git2::Oid,
) {
    let mut git_repo = test.git_repo();
    let (commit, base_oid) = (Oid::from(to_split), Oid::from(base));
    let count = git_repo
        .count_split_per_hunk_group(&commit, &commit, &base_oid)
        .unwrap();
    let result = git_repo.split_commit_per_hunk_group(&commit, &commit, &base_oid);

    if count < 2 {
        assert!(result.is_err(), "split with {count} group: {result:?}");
        assert_eq!(test.commits_from_head(base), [to_split]);
        return;
    }
    result.unwrap();
    let pieces = changes_per_piece(test, base);
    assert_eq!(pieces.len(), count, "{pieces:?}");
    assert!(pieces.iter().all(|p| !p.is_empty()), "{pieces:?}");
    assert_eq!(test.head_tree_id(), test.tree_id(to_split));
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
        [vec!["A a", "D a/x"], vec!["M b.txt"]]
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
        [vec!["D a", "A a/x"], vec!["M b.txt"]]
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
        [vec!["A a", "D a/x"], vec!["M b.txt"]]
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
        [vec!["D a", "A a/x"], vec!["M b.txt"]]
    );
}

#[test]
fn split_per_hunk_group_makes_no_empty_piece_when_a_file_replaces_a_directory() {
    let test = common::TestRepo::new();
    let (base, to_split) = commit_directory_becoming_a_file(&test, TEXT);
    assert_per_hunk_group_has_no_empty_piece(&test, base, to_split);
}

#[test]
fn split_per_hunk_group_makes_no_empty_piece_when_a_directory_replaces_a_binary_file() {
    let test = common::TestRepo::new();
    let (base, to_split) = commit_file_becoming_a_directory(&test, BINARY);
    assert_per_hunk_group_has_no_empty_piece(&test, base, to_split);
}

#[test]
fn split_out_hunks_leaves_a_directory_replaced_by_a_file_in_the_remainder() {
    let test = common::TestRepo::new();
    let (base, to_split) = commit_directory_becoming_a_file(&test, TEXT);

    let commit = Oid::from(to_split);
    test.git_repo()
        .split_commit_out_hunks(&commit, &[(2, 0)], &commit, 3)
        .unwrap();

    assert_eq!(
        changes_per_piece(&test, base),
        [vec!["A a", "D a/x"], vec!["M b.txt"]]
    );
}

#[test]
fn split_out_hunks_takes_the_directory_a_picked_file_replaces() {
    let test = common::TestRepo::new();
    let (base, to_split) = commit_directory_becoming_a_file(&test, TEXT);

    let commit = Oid::from(to_split);
    test.git_repo()
        .split_commit_out_hunks(&commit, &[(0, 0)], &commit, 3)
        .unwrap();

    assert_eq!(
        changes_per_piece(&test, base),
        [vec!["M b.txt"], vec!["A a", "D a/x"]]
    );
}

#[test]
fn split_out_hunks_refuses_only_a_directory_a_file_replaces() {
    let test = common::TestRepo::new();
    let (base, to_split) = commit_directory_becoming_a_file(&test, TEXT);

    let commit = Oid::from(to_split);
    let result = test
        .git_repo()
        .split_commit_out_hunks(&commit, &[(1, 0)], &commit, 3);

    assert!(result.is_err(), "{result:?}");
    assert_eq!(test.commits_from_head(base), [to_split]);
}

#[test]
fn split_out_hunks_refuses_only_a_file_a_directory_replaces() {
    let test = common::TestRepo::new();
    let (base, to_split) = commit_file_becoming_a_directory(&test, TEXT);

    let commit = Oid::from(to_split);
    let result = test
        .git_repo()
        .split_commit_out_hunks(&commit, &[(0, 0)], &commit, 3);

    assert!(result.is_err(), "{result:?}");
    assert_eq!(test.commits_from_head(base), [to_split]);
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
        [vec!["M b.txt"], vec!["A a", "D a/x"]]
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
        [vec!["M b.txt"], vec!["D a", "A a/x"]]
    );
}

#[test]
fn split_out_files_refuses_only_a_directory_a_file_replaces() {
    let test = common::TestRepo::new();
    let (base, to_split) = commit_directory_becoming_a_file(&test, TEXT);

    let commit = Oid::from(to_split);
    let result = test
        .git_repo()
        .split_commit_out_files(&commit, &[PathBuf::from("a/x")], &commit);

    assert!(result.is_err(), "{result:?}");
    assert_eq!(test.commits_from_head(base), [to_split]);
}

#[test]
fn split_out_files_refuses_only_a_file_a_directory_replaces() {
    let test = common::TestRepo::new();
    let (base, to_split) = commit_file_becoming_a_directory(&test, TEXT);

    let commit = Oid::from(to_split);
    let result = test
        .git_repo()
        .split_commit_out_files(&commit, &[PathBuf::from("a")], &commit);

    assert!(result.is_err(), "{result:?}");
    assert_eq!(test.commits_from_head(base), [to_split]);
}
