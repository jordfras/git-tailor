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

//! Integration tests for turning a working-tree row into a temporary source
//! commit, the primitive behind squashing staged/unstaged changes into an
//! existing commit.

#[allow(dead_code)]
mod common;

use common::prelude::*;
use git_tailor::repo::WorktreeSource;

/// Paths touched by one of the synthetic working-tree rows.
fn row_paths(diff: Option<git_tailor::CommitDiff>) -> Vec<String> {
    diff.map(|d| {
        d.files
            .iter()
            .filter_map(|f| f.new_path.clone().or_else(|| f.old_path.clone()))
            .collect()
    })
    .unwrap_or_default()
}

/// The working-tree content of `path`.
fn workdir(test: &common::TestRepo, path: &str) -> String {
    std::fs::read_to_string(test.repo.workdir().unwrap().join(path)).unwrap()
}

/// A repository with `a.txt` staged and `b.txt` modified but unstaged, on top
/// of a two-commit history.
fn mixed_state() -> common::TestRepo {
    let test = common::TestRepo::new();
    test.commit_file("a.txt", "a1\n", "first");
    test.commit_file("b.txt", "b1\n", "second");
    test.write_file("a.txt", "a2\n");
    test.stage_file("a.txt");
    test.write_file("b.txt", "b2\n");
    test
}

/// The staged row becomes a commit of exactly the index, leaving the unstaged
/// edit untouched and still unstaged.
#[test]
fn staged_source_commits_the_index_and_keeps_unstaged_changes() {
    let test = mixed_state();
    let git_repo = test.git_repo();
    let head_before = git_repo.head_oid().unwrap();

    let started = git_repo
        .begin_worktree_source(WorktreeSource::Staged)
        .unwrap()
        .expect("the staged row has changes to fold in");

    assert_eq!(git_repo.head_oid().unwrap(), started.temp_oid);
    assert_eq!(started.snapshot.tip_before, head_before);

    // The temp commit carries only the staged change.
    let diff = git_repo.commit_diff(&started.temp_oid, 3).unwrap();
    assert_eq!(
        diff.files
            .iter()
            .filter_map(|f| f.new_path.clone())
            .collect::<Vec<_>>(),
        vec!["a.txt".to_string()]
    );

    // Nothing is staged any more, and the unstaged edit survived verbatim.
    assert!(git_repo.staged_diff(3).unwrap().is_none());
    assert_eq!(row_paths(git_repo.unstaged_diff(3).unwrap()), ["b.txt"]);
    assert_eq!(workdir(&test, "b.txt"), "b2\n");
}

/// The unstaged row becomes a commit of exactly the unstaged delta — the staged
/// change stays behind, still staged.
#[test]
fn unstaged_source_commits_only_the_unstaged_delta() {
    let test = mixed_state();
    let git_repo = test.git_repo();

    let started = git_repo
        .begin_worktree_source(WorktreeSource::Unstaged)
        .unwrap()
        .expect("the unstaged row has changes to fold in");

    let diff = git_repo.commit_diff(&started.temp_oid, 3).unwrap();
    assert_eq!(
        diff.files
            .iter()
            .filter_map(|f| f.new_path.clone())
            .collect::<Vec<_>>(),
        vec!["b.txt".to_string()]
    );

    // The staged change is still staged and nothing is left unstaged.
    assert_eq!(row_paths(git_repo.staged_diff(3).unwrap()), ["a.txt"]);
    assert!(git_repo.unstaged_diff(3).unwrap().is_none());
    assert_eq!(workdir(&test, "a.txt"), "a2\n");
    assert_eq!(workdir(&test, "b.txt"), "b2\n");
}

/// With nothing staged, the unstaged row needs no patch surgery — the whole
/// working tree becomes the temp commit.
#[test]
fn unstaged_source_without_staged_changes_commits_the_working_tree() {
    let test = common::TestRepo::new();
    test.commit_file("a.txt", "a1\n", "first");
    test.write_file("a.txt", "a2\n");
    let git_repo = test.git_repo();

    let started = git_repo
        .begin_worktree_source(WorktreeSource::Unstaged)
        .unwrap()
        .expect("the unstaged row has changes to fold in");

    assert_eq!(
        test.tree_id(git2::Oid::from(&started.temp_oid)),
        git2::Oid::from(&started.snapshot.worktree_tree)
    );
    assert!(git_repo.staged_diff(3).unwrap().is_none());
    assert!(git_repo.unstaged_diff(3).unwrap().is_none());
}

/// Aborting puts the branch, the index and the working tree back exactly as
/// they were — for either row.
#[test]
fn abort_restores_the_branch_index_and_working_tree() {
    for source in [WorktreeSource::Staged, WorktreeSource::Unstaged] {
        let test = mixed_state();
        let git_repo = test.git_repo();
        let head_before = git_repo.head_oid().unwrap();

        let started = git_repo.begin_worktree_source(source).unwrap().unwrap();
        git_repo.abort_worktree_source(&started.snapshot).unwrap();

        assert_eq!(git_repo.head_oid().unwrap(), head_before, "{source:?}");
        assert_eq!(row_paths(git_repo.staged_diff(3).unwrap()), ["a.txt"]);
        assert_eq!(row_paths(git_repo.unstaged_diff(3).unwrap()), ["b.txt"]);
        assert_eq!(workdir(&test, "a.txt"), "a2\n");
        assert_eq!(workdir(&test, "b.txt"), "b2\n");
    }
}

/// A file the unstaged row deletes must come back on abort, not linger as a
/// leftover of the temp commit's checkout.
#[test]
fn abort_restores_a_file_the_row_deleted() {
    let test = common::TestRepo::new();
    test.commit_files(&[("a.txt", "a1\n"), ("gone.txt", "g1\n")], "first");
    std::fs::remove_file(test.repo.workdir().unwrap().join("gone.txt")).unwrap();
    let git_repo = test.git_repo();

    let started = git_repo
        .begin_worktree_source(WorktreeSource::Unstaged)
        .unwrap()
        .unwrap();
    git_repo.abort_worktree_source(&started.snapshot).unwrap();

    assert!(
        !test.repo.workdir().unwrap().join("gone.txt").exists(),
        "the deletion is part of the row and must be restored as a deletion"
    );
    assert_eq!(row_paths(git_repo.unstaged_diff(3).unwrap()), ["gone.txt"]);
}

/// An untracked file belongs to neither row, so it survives both the temp
/// commit and the abort.
#[test]
fn untracked_files_are_left_alone() {
    let test = mixed_state();
    test.write_file("untracked.txt", "u1\n");
    let git_repo = test.git_repo();

    let started = git_repo
        .begin_worktree_source(WorktreeSource::Unstaged)
        .unwrap()
        .unwrap();
    assert_eq!(workdir(&test, "untracked.txt"), "u1\n");

    git_repo.abort_worktree_source(&started.snapshot).unwrap();
    assert_eq!(workdir(&test, "untracked.txt"), "u1\n");
}

/// An empty row has nothing to fold in and is reported as such rather than
/// producing an empty commit.
#[test]
fn an_empty_row_yields_nothing() {
    let test = common::TestRepo::new();
    test.commit_file("a.txt", "a1\n", "first");
    let git_repo = test.git_repo();

    assert!(
        git_repo
            .begin_worktree_source(WorktreeSource::Staged)
            .unwrap()
            .is_none()
    );
    assert!(
        git_repo
            .begin_worktree_source(WorktreeSource::Unstaged)
            .unwrap()
            .is_none()
    );
}

/// The unstaged row cannot be separated when its edits sit on the same lines as
/// the staged ones. That is refused outright, leaving the repository untouched.
#[test]
fn overlapping_staged_and_unstaged_edits_are_refused() {
    let test = common::TestRepo::new();
    test.commit_file("a.txt", "one\ntwo\nthree\n", "first");
    test.write_file("a.txt", "one\nSTAGED\nthree\n");
    test.stage_file("a.txt");
    test.write_file("a.txt", "one\nUNSTAGED\nthree\n");
    let git_repo = test.git_repo();
    let head_before = git_repo.head_oid().unwrap();

    let err = git_repo
        .begin_worktree_source(WorktreeSource::Unstaged)
        .expect_err("overlapping edits cannot be separated");
    assert!(
        format!("{err:#}").contains("staged"),
        "the error must point at the staged changes, got: {err:#}"
    );

    assert_eq!(git_repo.head_oid().unwrap(), head_before);
    assert_eq!(row_paths(git_repo.staged_diff(3).unwrap()), ["a.txt"]);
    assert_eq!(workdir(&test, "a.txt"), "one\nUNSTAGED\nthree\n");
}

/// A crash between the temp commit and the squash must be recoverable, so the
/// snapshot is journalled write-ahead.
#[test]
fn beginning_records_the_snapshot_in_the_journal() {
    let test = mixed_state();
    let git_repo = test.git_repo();
    let started = git_repo
        .begin_worktree_source(WorktreeSource::Staged)
        .unwrap()
        .unwrap();

    // A fresh handle (simulating a restart) sees the paused operation.
    let reopened = test.git_repo();
    match reopened.read_journal().unwrap() {
        git_tailor::repo::JournalStatus::Recovered(record) => match *record {
            git_tailor::repo::InProgress::WorktreeSquash(snapshot) => {
                assert_eq!(snapshot, started.snapshot)
            }
            other => panic!("expected a worktree-source record, got {other:?}"),
        },
        other => panic!("expected Recovered, got {other:?}"),
    }
}
