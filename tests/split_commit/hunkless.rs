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

//! Splitting commits whose changes include some with no hunks: a binary file,
//! an empty file, a mode change.

use crate::common;
use crate::common::prelude::*;

/// What each commit from `base` up to HEAD changes, oldest first: a path for a
/// content change, the path with " +x" for a mode change.
fn changes_per_commit(test: &common::TestRepo, base: git2::Oid) -> Vec<Vec<String>> {
    let mut before = test.repo.find_commit(base).unwrap().tree().unwrap();
    test.commits_from_head(base)
        .into_iter()
        .map(|oid| {
            let after = test.repo.find_commit(oid).unwrap().tree().unwrap();
            let diff = test
                .repo
                .diff_tree_to_tree(Some(&before), Some(&after), None)
                .unwrap();
            let mut changes = Vec::new();
            for delta in diff.deltas() {
                let path = delta
                    .new_file()
                    .path()
                    .or(delta.old_file().path())
                    .unwrap()
                    .display()
                    .to_string();
                if delta.old_file().id() != delta.new_file().id() {
                    changes.push(path.clone());
                }
                if delta.old_file().mode() != delta.new_file().mode()
                    && delta.status() == git2::Delta::Modified
                {
                    changes.push(format!("{path} +x"));
                }
            }
            before = after;
            changes
        })
        .collect()
}

#[test]
fn split_per_file_handles_a_binary_file_that_is_not_last() {
    let test = common::TestRepo::new();
    let base = test.commit_files(
        &[("a.txt", "a1\n"), ("bin.dat", "\0old\0"), ("z.txt", "z1\n")],
        "base",
    );
    let to_split = test.commit_files(
        &[("a.txt", "a2\n"), ("bin.dat", "\0new\0"), ("z.txt", "z2\n")],
        "change",
    );

    let mut git_repo = test.git_repo();
    git_repo
        .split_commit_per_file(&Oid::from(to_split), &Oid::from(to_split))
        .unwrap();

    assert_eq!(
        changes_per_commit(&test, base),
        [vec!["a.txt"], vec!["bin.dat"], vec!["z.txt"]]
    );
    assert_eq!(test.head_tree_id(), test.tree_id(to_split));
}

/// Stage `path` as executable. Git records the mode on every platform; on Unix
/// the file on disk has to agree too, or the split's dirty-overlap check sees
/// the working tree disagreeing with the commit.
fn make_executable(test: &common::TestRepo, path: &str) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let full = test.repo.workdir().unwrap().join(path);
        std::fs::set_permissions(full, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let mut index = test.repo.index().unwrap();
    index.read(true).unwrap();
    let mut entry = index.get_path(std::path::Path::new(path), 0).unwrap();
    entry.mode = 0o100755;
    index.add(&entry).unwrap();
    index.write().unwrap();
}

#[test]
fn split_out_hunks_keeps_binary_and_empty_file_changes_in_the_remainder() {
    let test = common::TestRepo::new();
    let base = test.commit_files(
        &[("a.txt", "a1\n"), ("b.txt", "b1\n"), ("bin.dat", "\0old\0")],
        "base",
    );
    let to_split = test.commit_files(
        &[
            ("a.txt", "a2\n"),
            ("b.txt", "b2\n"),
            ("bin.dat", "\0new\0"),
            ("empty.txt", ""),
        ],
        "change",
    );

    let mut git_repo = test.git_repo();
    git_repo
        .split_commit_out_hunks(&Oid::from(to_split), &[(0, 0)], &Oid::from(to_split), 0)
        .unwrap();

    assert_eq!(
        changes_per_commit(&test, base),
        [vec!["b.txt", "bin.dat", "empty.txt"], vec!["a.txt"]]
    );
    assert_eq!(test.head_tree_id(), test.tree_id(to_split));
}

#[test]
fn split_out_hunks_keeps_a_mode_only_change_in_the_remainder() {
    let test = common::TestRepo::new();
    let base = test.commit_files(
        &[("a.txt", "a1\n"), ("b.txt", "b1\n"), ("run.sh", "run\n")],
        "base",
    );
    test.write_file("a.txt", "a2\n");
    test.write_file("b.txt", "b2\n");
    test.stage_file("a.txt");
    test.stage_file("b.txt");
    make_executable(&test, "run.sh");
    let to_split = test.commit("change");

    let mut git_repo = test.git_repo();
    git_repo
        .split_commit_out_hunks(&Oid::from(to_split), &[(0, 0)], &Oid::from(to_split), 0)
        .unwrap();

    assert_eq!(
        changes_per_commit(&test, base),
        [vec!["b.txt", "run.sh +x"], vec!["a.txt"]]
    );
    assert_eq!(test.head_tree_id(), test.tree_id(to_split));
}

/// Picking every hunk still leaves the hunkless changes behind, so there is a
/// remainder to keep and the split is not empty.
#[test]
fn split_out_hunks_allows_picking_every_hunk_when_a_hunkless_change_remains() {
    let test = common::TestRepo::new();
    let base = test.commit_files(
        &[("a.txt", "a1\n"), ("b.txt", "b1\n"), ("bin.dat", "\0old\0")],
        "base",
    );
    let to_split = test.commit_files(
        &[("a.txt", "a2\n"), ("b.txt", "b2\n"), ("bin.dat", "\0new\0")],
        "change",
    );

    let mut git_repo = test.git_repo();
    git_repo
        .split_commit_out_hunks(
            &Oid::from(to_split),
            &[(0, 0), (1, 0)],
            &Oid::from(to_split),
            0,
        )
        .unwrap();

    assert_eq!(
        changes_per_commit(&test, base),
        [vec!["bin.dat"], vec!["a.txt", "b.txt"]]
    );
}
