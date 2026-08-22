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

//! Squashing the staged or unstaged working-tree row into an existing commit,
//! end to end: the fold itself, what is left in the index and working tree
//! afterwards, and undo/redo of the whole thing as one operation.

use crate::common;
use crate::common::prelude::*;
use git_tailor::repo::{UndoOutcome, WorktreeSource};

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

fn workdir(test: &common::TestRepo, path: &str) -> String {
    std::fs::read_to_string(test.repo.workdir().unwrap().join(path)).unwrap()
}

/// A branch whose target commit introduces both `a.txt` and `b.txt`, with
/// `a.txt` then edited and staged and `b.txt` edited but left unstaged — so
/// either row can be folded into the target.
fn mixed_state() -> (common::TestRepo, git2::Oid, git2::Oid) {
    let test = common::TestRepo::new();
    let base = test.commit_file("base.txt", "base\n", "base");
    let target = test.commit_files(&[("a.txt", "a1\n"), ("b.txt", "b1\n")], "target commit");
    test.commit_file("c.txt", "c1\n", "later commit");
    test.write_file("a.txt", "a2\n");
    test.stage_file("a.txt");
    test.write_file("b.txt", "b2\n");
    (test, base, target)
}

/// Fold the whole row into `target`, as the fixup key does.
fn fixup_row(
    git_repo: &Git2Repo,
    source: WorktreeSource,
    target: git2::Oid,
) -> git_tailor::repo::RebaseOutcome {
    let started = git_repo
        .begin_worktree_source(source)
        .unwrap()
        .expect("the row has changes to fold in");
    git_repo
        .squash_commits(
            &started.temp_oid,
            &Oid::from(target),
            "target commit",
            &started.temp_oid,
        )
        .unwrap()
}

/// The staged changes land in the target commit and stop being staged; the
/// unstaged edit is untouched and still unstaged.
#[test]
fn staged_row_folds_into_the_target_and_leaves_unstaged_changes() {
    let (test, base, target) = mixed_state();
    let git_repo = test.git_repo();

    assert_rebase_complete!(fixup_row(&git_repo, WorktreeSource::Staged, target));

    assert_history!(&test, base, &["target commit", "later commit"]);
    assert_file_contents_at_head!(&test.repo, "a.txt", "a2\n");
    assert!(git_repo.staged_diff(3).unwrap().is_none());
    assert_eq!(row_paths(git_repo.unstaged_diff(3).unwrap()), ["b.txt"]);
    assert_eq!(workdir(&test, "b.txt"), "b2\n");
}

/// The unstaged changes land in the target commit; the staged edit stays
/// staged.
#[test]
fn unstaged_row_folds_into_the_target_and_leaves_staged_changes() {
    let test = common::TestRepo::new();
    let base = test.commit_file("base.txt", "base\n", "base");
    let target = test.commit_file("b.txt", "b1\n", "target commit");
    test.commit_file("c.txt", "c1\n", "later commit");
    test.write_file("a.txt", "a1\n");
    test.stage_file("a.txt");
    test.write_file("b.txt", "b2\n");
    let git_repo = test.git_repo();

    assert_rebase_complete!(fixup_row(&git_repo, WorktreeSource::Unstaged, target));

    assert_history!(&test, base, &["target commit", "later commit"]);
    assert_file_contents_at_head!(&test.repo, "b.txt", "b2\n");
    assert_eq!(row_paths(git_repo.staged_diff(3).unwrap()), ["a.txt"]);
    assert!(git_repo.unstaged_diff(3).unwrap().is_none());
    assert_eq!(workdir(&test, "a.txt"), "a1\n");
}

/// With a clean index the unstaged row folds in and leaves nothing behind.
#[test]
fn unstaged_row_folds_in_with_a_clean_index() {
    let test = common::TestRepo::new();
    let base = test.commit_file("base.txt", "base\n", "base");
    let target = test.commit_file("a.txt", "a1\n", "target commit");
    test.commit_file("b.txt", "b1\n", "later commit");
    test.write_file("a.txt", "a2\n");
    let git_repo = test.git_repo();

    assert_rebase_complete!(fixup_row(&git_repo, WorktreeSource::Unstaged, target));

    assert_history!(&test, base, &["target commit", "later commit"]);
    assert_file_contents_at_head!(&test.repo, "a.txt", "a2\n");
    assert!(git_repo.staged_diff(3).unwrap().is_none());
    assert!(git_repo.unstaged_diff(3).unwrap().is_none());
}

/// The whole fold is one undoable operation: one undo restores the branch and
/// the exact staged/unstaged split, and redo puts it back.
#[test]
fn undo_and_redo_restore_the_staged_unstaged_split() {
    for source in [WorktreeSource::Staged, WorktreeSource::Unstaged] {
        let (test, _base, target) = mixed_state();
        let git_repo = test.git_repo();
        let head_before = git_repo.head_oid().unwrap();

        assert_rebase_complete!(fixup_row(&git_repo, source, target));
        let head_after = git_repo.head_oid().unwrap();

        match git_repo.undo().unwrap() {
            UndoOutcome::Done { .. } => {}
            other => panic!("expected the fold to be undoable, got {other:?} for {source:?}"),
        }
        assert_eq!(git_repo.head_oid().unwrap(), head_before, "{source:?}");
        assert_eq!(row_paths(git_repo.staged_diff(3).unwrap()), ["a.txt"]);
        assert_eq!(row_paths(git_repo.unstaged_diff(3).unwrap()), ["b.txt"]);
        assert_eq!(workdir(&test, "a.txt"), "a2\n");
        assert_eq!(workdir(&test, "b.txt"), "b2\n");

        match git_repo.redo().unwrap() {
            UndoOutcome::Done { .. } => {}
            other => panic!("expected a redo, got {other:?} for {source:?}"),
        }
        assert_eq!(git_repo.head_oid().unwrap(), head_after, "{source:?}");
    }
}

/// The fold leaves exactly one undo entry — the temporary commit is an
/// implementation detail, not a step the user has to undo separately.
#[test]
fn the_fold_is_a_single_undo_entry() {
    let (test, _base, target) = mixed_state();
    let git_repo = test.git_repo();
    let head_before = git_repo.head_oid().unwrap();

    assert_rebase_complete!(fixup_row(&git_repo, WorktreeSource::Staged, target));
    git_repo.undo().unwrap();
    assert_eq!(git_repo.head_oid().unwrap(), head_before);

    match git_repo.undo().unwrap() {
        UndoOutcome::Empty => {}
        other => panic!("expected nothing more to undo, got {other:?}"),
    }
}

/// A folded row leaves no in-progress record behind for startup recovery to
/// trip over.
#[test]
fn a_completed_fold_clears_the_journal() {
    let (test, _base, target) = mixed_state();
    let git_repo = test.git_repo();
    assert_rebase_complete!(fixup_row(&git_repo, WorktreeSource::Staged, target));

    let reopened = test.git_repo();
    assert!(matches!(
        reopened.read_journal().unwrap(),
        git_tailor::repo::JournalStatus::None
    ));
}

/// Aborting a conflicted fold puts the branch, index and working tree back
/// exactly as they were — including the changes lifted into the temporary
/// commit.
#[test]
fn aborting_a_conflicted_fold_restores_everything() {
    let test = common::TestRepo::new();
    let base = test.commit_file("a.txt", "one\ntwo\n", "base");
    let target = test.commit_file("a.txt", "one\nTARGET\n", "target commit");
    test.commit_file("a.txt", "one\nLATER\n", "later commit");
    test.write_file("a.txt", "one\nWIP\n");
    test.write_file("b.txt", "b1\n");
    test.stage_file("b.txt");
    let git_repo = test.git_repo();
    let head_before = git_repo.head_oid().unwrap();

    let started = git_repo
        .begin_worktree_source(WorktreeSource::Unstaged)
        .unwrap()
        .unwrap();
    let state = expect_rebase_conflict!(
        git_repo
            .squash_commits(
                &started.temp_oid,
                &Oid::from(target),
                "target commit",
                &started.temp_oid,
            )
            .unwrap()
    );

    git_repo.rebase_abort(&state).unwrap();

    assert_eq!(git_repo.head_oid().unwrap(), head_before);
    assert_history!(&test, base, &["target commit", "later commit"]);
    assert_eq!(workdir(&test, "a.txt"), "one\nWIP\n");
    assert_eq!(row_paths(git_repo.staged_diff(3).unwrap()), ["b.txt"]);
    assert_eq!(row_paths(git_repo.unstaged_diff(3).unwrap()), ["a.txt"]);

    let reopened = test.git_repo();
    assert!(matches!(
        reopened.read_journal().unwrap(),
        git_tailor::repo::JournalStatus::None
    ));
}
