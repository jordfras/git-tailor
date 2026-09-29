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
            .split_commit_per_hunk_group(&Oid::from(to_split), &head, &Oid::from(base))
            .unwrap();

        let modes = link_modes(&test, base);
        assert!(
            modes.iter().all(Option::is_some),
            "link went missing from a piece: {modes:?}"
        );
    }
}

/// Commit a binary `link` with `a.txt`, then a commit that replaces `link`
/// with a symlink and changes `a.txt`.
fn commit_binary_becoming_a_symlink(test: &common::TestRepo) -> (git2::Oid, git2::Oid) {
    let workdir = test.repo.workdir().unwrap().to_path_buf();
    let base = test.commit_files(&[("a.txt", "a1\n"), ("link", "\0old\0")], "base");

    std::fs::remove_file(workdir.join("link")).unwrap();
    std::os::unix::fs::symlink("a.txt", workdir.join("link")).unwrap();
    test.write_file("a.txt", "a2\n");
    for path in ["link", "a.txt"] {
        test.stage_file(path);
    }
    (base, test.commit("change"))
}

#[test]
fn split_out_hunks_keeps_a_symlink_that_replaced_a_binary_file_in_the_remainder() {
    let test = common::TestRepo::new();
    let (base, to_split) = commit_binary_becoming_a_symlink(&test);

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
