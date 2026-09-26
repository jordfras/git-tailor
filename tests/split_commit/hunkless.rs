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

/// Each change without a hunk is an indivisible unit, like a hunk, so it gets a
/// piece of its own — after the hunks, in path order.
#[test]
fn split_per_hunk_gives_each_hunkless_change_its_own_piece() {
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
    assert_eq!(
        git_repo.count_split_per_hunk(&Oid::from(to_split)).unwrap(),
        4
    );
    git_repo
        .split_commit_per_hunk(&Oid::from(to_split), &Oid::from(to_split))
        .unwrap();

    assert_eq!(
        changes_per_commit(&test, base),
        [
            vec!["a.txt"],
            vec!["b.txt"],
            vec!["bin.dat"],
            vec!["empty.txt"]
        ]
    );
    assert_eq!(test.head_tree_id(), test.tree_id(to_split));
}

#[test]
fn split_per_hunk_gives_a_mode_only_change_its_own_piece() {
    let test = common::TestRepo::new();
    let base = test.commit_files(&[("a.txt", "a1\n"), ("run.sh", "run\n")], "base");
    test.write_file("a.txt", "a2\n");
    test.stage_file("a.txt");
    make_executable(&test, "run.sh");
    let to_split = test.commit("change");

    let mut git_repo = test.git_repo();
    assert_eq!(
        git_repo.count_split_per_hunk(&Oid::from(to_split)).unwrap(),
        2
    );
    git_repo
        .split_commit_per_hunk(&Oid::from(to_split), &Oid::from(to_split))
        .unwrap();

    assert_eq!(
        changes_per_commit(&test, base),
        [vec!["a.txt"], vec!["run.sh +x"]]
    );
}

/// No fragmap column claims a change without hunks, so no group does either;
/// together they make one extra piece after the groups.
#[test]
fn split_per_hunk_group_puts_hunkless_changes_in_one_extra_piece() {
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
    // Touching b.txt again sets its hunk apart from a.txt's: two groups.
    test.commit_file("b.txt", "b3\n", "later");

    let mut git_repo = test.git_repo();
    let head = git_repo.head_oid().unwrap();
    assert_eq!(
        git_repo
            .count_split_per_hunk_group(&Oid::from(to_split), &head, &Oid::from(base))
            .unwrap(),
        3
    );
    git_repo
        .split_commit_per_hunk_group(&Oid::from(to_split), &head, &Oid::from(base))
        .unwrap();

    assert_eq!(
        changes_per_commit(&test, base),
        [
            vec!["a.txt"],
            vec!["b.txt"],
            vec!["bin.dat", "empty.txt"],
            vec!["b.txt"]
        ]
    );
}

/// One group beside a hunkless change is still two pieces.
#[test]
fn split_per_hunk_group_splits_one_group_from_a_mode_only_change() {
    let test = common::TestRepo::new();
    let base = test.commit_files(&[("a.txt", "a1\n"), ("run.sh", "run\n")], "base");
    test.write_file("a.txt", "a2\n");
    test.stage_file("a.txt");
    make_executable(&test, "run.sh");
    let to_split = test.commit("change");

    let mut git_repo = test.git_repo();
    let head = git_repo.head_oid().unwrap();
    assert_eq!(
        git_repo
            .count_split_per_hunk_group(&Oid::from(to_split), &head, &Oid::from(base))
            .unwrap(),
        2
    );
    git_repo
        .split_commit_per_hunk_group(&Oid::from(to_split), &head, &Oid::from(base))
        .unwrap();

    assert_eq!(
        changes_per_commit(&test, base),
        [vec!["a.txt"], vec!["run.sh +x"]]
    );
}

/// Commit a change to `a.txt`, `run.sh` and `z.txt` that also makes `run.sh`
/// executable, and return (base, the commit).
fn commit_text_and_mode_change(test: &common::TestRepo) -> (git2::Oid, git2::Oid) {
    let base = test.commit_files(
        &[("a.txt", "a1\n"), ("run.sh", "run1\n"), ("z.txt", "z1\n")],
        "base",
    );
    for (path, content) in [("a.txt", "a2\n"), ("run.sh", "run2\n"), ("z.txt", "z2\n")] {
        test.write_file(path, content);
        test.stage_file(path);
    }
    make_executable(test, "run.sh");
    (base, test.commit("change"))
}

#[test]
fn split_per_hunk_keeps_a_mode_change_with_the_files_first_hunk() {
    let test = common::TestRepo::new();
    let (base, to_split) = commit_text_and_mode_change(&test);

    let mut git_repo = test.git_repo();
    git_repo
        .split_commit_per_hunk(&Oid::from(to_split), &Oid::from(to_split))
        .unwrap();

    assert_eq!(
        changes_per_commit(&test, base),
        [vec!["a.txt"], vec!["run.sh", "run.sh +x"], vec!["z.txt"]]
    );
}

#[test]
fn split_per_hunk_group_keeps_a_mode_change_with_the_files_first_hunk() {
    let test = common::TestRepo::new();
    let (base, to_split) = commit_text_and_mode_change(&test);
    // Touching z.txt again sets its hunk apart: two groups.
    test.commit_file("z.txt", "z3\n", "later");

    let mut git_repo = test.git_repo();
    let head = git_repo.head_oid().unwrap();
    git_repo
        .split_commit_per_hunk_group(&Oid::from(to_split), &head, &Oid::from(base))
        .unwrap();

    assert_eq!(
        changes_per_commit(&test, base),
        [
            vec!["a.txt", "run.sh", "run.sh +x"],
            vec!["z.txt"],
            vec!["z.txt"]
        ]
    );
}

#[test]
fn split_out_hunks_keeps_a_mode_change_with_the_files_unpicked_hunks() {
    let test = common::TestRepo::new();
    let (base, to_split) = commit_text_and_mode_change(&test);

    let mut git_repo = test.git_repo();
    git_repo
        .split_commit_out_hunks(&Oid::from(to_split), &[(0, 0)], &Oid::from(to_split), 0)
        .unwrap();

    assert_eq!(
        changes_per_commit(&test, base),
        [vec!["run.sh", "run.sh +x", "z.txt"], vec!["a.txt"]]
    );
}

/// With every hunk of the file picked, nothing of it is left to keep the mode
/// change company, so it goes along.
#[test]
fn split_out_hunks_takes_a_mode_change_along_with_all_of_the_files_hunks() {
    let test = common::TestRepo::new();
    let (base, to_split) = commit_text_and_mode_change(&test);

    let mut git_repo = test.git_repo();
    git_repo
        .split_commit_out_hunks(&Oid::from(to_split), &[(1, 0)], &Oid::from(to_split), 0)
        .unwrap();

    assert_eq!(
        changes_per_commit(&test, base),
        [vec!["a.txt", "z.txt"], vec!["run.sh", "run.sh +x"]]
    );
}
