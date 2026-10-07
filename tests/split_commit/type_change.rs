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

//! Splitting a commit that turns a symlink into a regular file, or a binary
//! file into a symlink. libgit2 reports that as a deletion and an addition at
//! the same path, and a piece that applies both must apply them in that order.
//!
//! Unix only: creating a symlink on Windows needs developer mode or admin
//! rights.

#![cfg(unix)]

use crate::common;
use crate::common::prelude::*;

/// A split can apply the two changes in either order, and did so at random,
/// so a single run proves little.
const RUNS: usize = 16;

/// Commit `link` as a symlink with `a.txt` and `bin.dat`, then a commit that
/// replaces the symlink with a regular file and changes the other two.
fn commit_symlink_becoming_a_file(test: &common::TestRepo) -> (git2::Oid, git2::Oid) {
    let workdir = test.repo.workdir().unwrap().to_path_buf();
    test.write_file("target", "t\n");
    test.write_file("a.txt", "a1\n");
    test.write_file("bin.dat", "\0old\0");
    std::os::unix::fs::symlink("target", workdir.join("link")).unwrap();
    for path in ["target", "a.txt", "bin.dat", "link"] {
        test.stage_file(path);
    }
    let base = test.commit("base");

    std::fs::remove_file(workdir.join("link")).unwrap();
    test.write_file("link", "now a file\n");
    test.write_file("a.txt", "a2\n");
    test.write_file("bin.dat", "\0new\0");
    for path in ["link", "a.txt", "bin.dat"] {
        test.stage_file(path);
    }
    (base, test.commit("change"))
}

/// The mode of `link` in each commit from `base` up to HEAD, oldest first, or
/// `None` where it is missing.
fn link_modes(test: &common::TestRepo, base: git2::Oid) -> Vec<Option<i32>> {
    test.commits_from_head(base)
        .into_iter()
        .map(|oid| {
            let tree = test.repo.find_commit(oid).unwrap().tree().unwrap();
            tree.get_path(std::path::Path::new("link"))
                .ok()
                .map(|entry| entry.filemode())
        })
        .collect()
}

#[test]
fn split_out_hunks_keeps_a_file_that_replaced_a_symlink_in_the_remainder() {
    for _ in 0..RUNS {
        let test = common::TestRepo::new();
        let (base, to_split) = commit_symlink_becoming_a_file(&test);

        // a.txt sorts first, so it is delta 0.
        let mut git_repo = test.git_repo();
        git_repo
            .split_commit_out_hunks(&Oid::from(to_split), &[(0, 0)], &Oid::from(to_split), 0)
            .unwrap();

        assert_eq!(
            link_modes(&test, base),
            [Some(0o100644), Some(0o100644)],
            "the remainder turns the symlink into the file"
        );
    }
}

#[test]
fn split_per_hunk_group_never_loses_a_file_that_replaced_a_symlink() {
    for _ in 0..RUNS {
        let test = common::TestRepo::new();
        let (base, to_split) = commit_symlink_becoming_a_file(&test);

        let mut git_repo = test.git_repo();
        let head = git_repo.head_oid().unwrap();
        git_repo
            .split_commit_per_hunk_group(&Oid::from(to_split), &head, &Oid::from(base), &mut |_| {
                true
            })
            .unwrap();

        let modes = link_modes(&test, base);
        assert!(
            modes.iter().all(Option::is_some),
            "link went missing from a piece: {modes:?}"
        );
    }
}

/// Commit a binary `link` with `a.txt`, then a commit that replaces `link`
/// with a symlink and changes `a.txt`. Each `(path, before, after)` in `also`
/// is committed and changed alongside.
fn commit_binary_becoming_a_symlink(
    test: &common::TestRepo,
    also: &[(&str, &str, &str)],
) -> (git2::Oid, git2::Oid) {
    let workdir = test.repo.workdir().unwrap().to_path_buf();
    let mut files = vec![("a.txt", "a1\n"), ("link", "\0old\0")];
    files.extend(also.iter().map(|&(path, before, _)| (path, before)));
    let base = test.commit_files(&files, "base");

    std::fs::remove_file(workdir.join("link")).unwrap();
    std::os::unix::fs::symlink("a.txt", workdir.join("link")).unwrap();
    test.write_file("a.txt", "a2\n");
    for &(path, _, after) in also {
        test.write_file(path, after);
    }
    for path in ["link", "a.txt"]
        .into_iter()
        .chain(also.iter().map(|f| f.0))
    {
        test.stage_file(path);
    }
    (base, test.commit("change"))
}

#[test]
fn split_out_hunks_keeps_a_symlink_that_replaced_a_binary_file_in_the_remainder() {
    let test = common::TestRepo::new();
    let (base, to_split) = commit_binary_becoming_a_symlink(&test, &[]);

    // a.txt sorts first, so it is delta 0.
    let mut git_repo = test.git_repo();
    git_repo
        .split_commit_out_hunks(&Oid::from(to_split), &[(0, 0)], &Oid::from(to_split), 0)
        .unwrap();

    assert_eq!(
        link_modes(&test, base),
        [Some(0o120000), Some(0o120000)],
        "the remainder turns the binary file into the symlink"
    );
}

#[test]
fn split_per_hunk_replaces_a_binary_file_with_a_symlink_in_one_piece() {
    let test = common::TestRepo::new();
    let (base, to_split) =
        commit_binary_becoming_a_symlink(&test, &[("z.bin", "\0old\0", "\0new\0")]);

    let mut git_repo = test.git_repo();
    git_repo
        .split_commit_per_hunk(&Oid::from(to_split), &Oid::from(to_split))
        .unwrap();

    assert_eq!(
        link_modes(&test, base),
        [Some(0o100644), Some(0o120000), Some(0o120000)],
        "a.txt's piece, then the symlink's, then z.bin's"
    );
}

/// Without other commits to relate them, `a.txt` and the symlink share one
/// group, and the binary's deletion is part of the symlink replacing it.
#[test]
fn split_per_hunk_group_has_nothing_to_split_when_one_group_replaces_a_binary_file_with_a_symlink()
{
    let test = common::TestRepo::new();
    let (base, to_split) = commit_binary_becoming_a_symlink(&test, &[]);

    let mut git_repo = test.git_repo();
    let head = git_repo.head_oid().unwrap();
    let count = git_repo
        .count_split_per_hunk_group(&Oid::from(to_split), &head, &Oid::from(base), &mut |_| true)
        .unwrap();
    let result = git_repo.split_commit_per_hunk_group(
        &Oid::from(to_split),
        &head,
        &Oid::from(base),
        &mut |_| true,
    );

    assert_eq!(count, 1);
    assert!(result.is_err(), "split with nothing to split: {result:?}");
    assert_eq!(test.commits_from_head(base), [to_split]);
}

/// The paths each commit from `base` up to HEAD changes, oldest first.
fn paths_per_commit(test: &common::TestRepo, base: git2::Oid) -> Vec<Vec<String>> {
    test.commits_from_head(base)
        .into_iter()
        .map(|oid| {
            let commit = test.repo.find_commit(oid).unwrap();
            let parent_tree = commit.parent(0).unwrap().tree().unwrap();
            let diff = test
                .repo
                .diff_tree_to_tree(Some(&parent_tree), Some(&commit.tree().unwrap()), None)
                .unwrap();
            let mut paths: Vec<String> = diff
                .deltas()
                .map(|delta| delta.new_file().path().unwrap().display().to_string())
                .collect();
            paths.dedup();
            paths
        })
        .collect()
}

/// The fragmap files a symlink's deletion and the file replacing it under one
/// path. Here the file's hunk relates to the later commit as a.txt's does,
/// while the symlink's does not, so the swap belongs with a.txt.
#[test]
fn split_per_hunk_group_places_a_symlink_replaced_by_a_file_by_both_its_hunks() {
    let test = common::TestRepo::new();
    let workdir = test.repo.workdir().unwrap().to_path_buf();
    test.write_file("a.txt", "a1\n");
    test.write_file("b.txt", "b1\n");
    std::os::unix::fs::symlink("a.txt", workdir.join("link")).unwrap();
    for path in ["a.txt", "b.txt", "link"] {
        test.stage_file(path);
    }
    let base = test.commit("base");

    std::fs::remove_file(workdir.join("link")).unwrap();
    test.write_file("link", "l1\n");
    test.write_file("a.txt", "a2\n");
    test.write_file("b.txt", "b2\n");
    for path in ["link", "a.txt", "b.txt"] {
        test.stage_file(path);
    }
    let to_split = test.commit("change");
    let later = test.commit_files(&[("a.txt", "a3\n"), ("link", "l2\n")], "later");

    let mut git_repo = test.git_repo();
    git_repo
        .split_commit_per_hunk_group(
            &Oid::from(to_split),
            &Oid::from(later),
            &Oid::from(base),
            &mut |_| true,
        )
        .unwrap();

    let mut pieces = paths_per_commit(&test, base);
    pieces.pop();
    assert_eq!(pieces, [vec!["a.txt", "link"], vec!["b.txt"]]);
}

#[test]
fn split_out_hunks_names_a_file_replaced_by_a_symlink_once() {
    let test = common::TestRepo::new();
    let base = test.commit_files(&[("f", "text\n"), ("g.txt", "g1\n")], "base");
    let workdir = test.repo.workdir().unwrap().to_path_buf();
    std::fs::remove_file(workdir.join("f")).unwrap();
    std::os::unix::fs::symlink("g.txt", workdir.join("f")).unwrap();
    test.write_file("g.txt", "g2\n");
    for path in ["f", "g.txt"] {
        test.stage_file(path);
    }
    let to_split = test.commit("change");

    // f's deletion sorts first, so it is delta 0; picking it takes the swap.
    let commit = Oid::from(to_split);
    test.git_repo()
        .split_commit_out_hunks(&commit, &[(0, 0)], &commit, 3)
        .unwrap();

    let split_out = *test.commits_from_head(base).last().unwrap();
    let summary = test.repo.find_commit(split_out).unwrap();
    assert_eq!(summary.summary().unwrap().unwrap(), "change (f)");
}
