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

//! Splitting a commit that changes a submodule pointer. libgit2 shows the
//! change as one "Subproject commit" hunk, but the pointer is a commit id, not
//! text, so applying that hunk has to write the id itself.

use crate::common;
use crate::common::prelude::*;

/// Commit `files` and stage `sub` at `pointer`, with an empty `sub/` in the
/// working tree as an uninitialized submodule leaves it.
fn commit_with_pointer(
    test: &common::TestRepo,
    files: &[(&str, &str)],
    pointer: git2::Oid,
    message: &str,
) -> git2::Oid {
    for &(path, content) in files {
        test.write_file(path, content);
        test.stage_file(path);
    }
    test.stage_gitlink("sub", pointer);
    std::fs::create_dir_all(test.repo.workdir().unwrap().join("sub")).unwrap();
    test.commit(message)
}

/// Where `sub` points in each commit from `base` up to HEAD, oldest first, or
/// `None` where it is missing. Panics on an entry that is not a gitlink.
fn pointers(test: &common::TestRepo, base: git2::Oid) -> Vec<Option<git2::Oid>> {
    test.commits_from_head(base)
        .into_iter()
        .map(|oid| {
            let tree = test.repo.find_commit(oid).unwrap().tree().unwrap();
            let entry = tree.get_path(std::path::Path::new("sub")).ok()?;
            assert_eq!(entry.filemode(), 0o160000, "sub is not a gitlink");
            Some(entry.id())
        })
        .collect()
}

#[test]
fn split_out_hunks_leaves_an_added_pointer_intact_in_the_remainder() {
    let test = common::TestRepo::new();
    let base = test.commit_files(&[("a.txt", "a1\n"), ("b.txt", "b1\n")], "base");
    let to_split = commit_with_pointer(
        &test,
        &[("a.txt", "a2\n"), ("b.txt", "b2\n")],
        base,
        "change",
    );

    // a.txt sorts first, so it is delta 0.
    let commit = Oid::from(to_split);
    test.git_repo()
        .split_commit_out_hunks(&commit, &[(0, 0)], &commit, 3)
        .unwrap();

    assert_eq!(pointers(&test, base), [Some(base), Some(base)]);
}

#[test]
fn split_out_hunks_splits_out_a_changed_pointer() {
    let test = common::TestRepo::new();
    let first = test.commit_file("a.txt", "a1\n", "first");
    commit_with_pointer(&test, &[], first, "add sub");
    let second = test.commit_file("b.txt", "b1\n", "second");
    let to_split = commit_with_pointer(&test, &[("a.txt", "a2\n")], second, "change");

    // a.txt sorts first, so sub is delta 1.
    let commit = Oid::from(to_split);
    test.git_repo()
        .split_commit_out_hunks(&commit, &[(1, 0)], &commit, 3)
        .unwrap();

    assert_eq!(
        pointers(&test, second),
        [Some(first), Some(second)],
        "the remainder keeps the old pointer, the split-out commit moves it"
    );
}

#[test]
fn split_out_hunks_leaves_a_changed_pointer_intact_in_the_remainder() {
    let test = common::TestRepo::new();
    let first = test.commit_file("a.txt", "a1\n", "first");
    commit_with_pointer(&test, &[], first, "add sub");
    let second = test.commit_file("b.txt", "b1\n", "second");
    let to_split = commit_with_pointer(&test, &[("a.txt", "a2\n")], second, "change");

    let commit = Oid::from(to_split);
    test.git_repo()
        .split_commit_out_hunks(&commit, &[(0, 0)], &commit, 3)
        .unwrap();

    assert_eq!(pointers(&test, second), [Some(second), Some(second)]);
}

#[test]
fn split_per_hunk_writes_a_pointer_changed_in_a_middle_piece() {
    let test = common::TestRepo::new();
    let first = test.commit_files(&[("a.txt", "a1\n"), ("z.txt", "z1\n")], "first");
    commit_with_pointer(&test, &[], first, "add sub");
    let second = test.commit_file("b.txt", "b1\n", "second");
    let to_split = commit_with_pointer(
        &test,
        &[("a.txt", "a2\n"), ("z.txt", "z2\n")],
        second,
        "change",
    );

    let commit = Oid::from(to_split);
    test.git_repo()
        .split_commit_per_hunk(&commit, &commit)
        .unwrap();

    assert_eq!(
        pointers(&test, second),
        [Some(first), Some(second), Some(second)],
        "a.txt's piece, then sub's, then z.txt's"
    );
}
