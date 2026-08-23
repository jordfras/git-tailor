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
    assert_eq!(started.snapshot.temp_oid, started.temp_oid);
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

/// The snapshot names the commit the branch was left on, which is what tells a
/// later run whether the record still describes the repository or something that
/// has long moved on.
#[test]
fn the_snapshot_names_the_commit_the_branch_was_left_on() {
    let test = mixed_state();
    let git_repo = test.git_repo();
    let started = git_repo
        .begin_worktree_source(WorktreeSource::Unstaged)
        .unwrap()
        .unwrap();

    assert_eq!(started.snapshot.temp_oid, git_repo.head_oid().unwrap());

    // Once the branch moves elsewhere the record no longer matches it.
    git_repo.abort_worktree_source(&started.snapshot).unwrap();
    assert_ne!(started.snapshot.temp_oid, git_repo.head_oid().unwrap());
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

/// Separating the unstaged row from the staged one must not be limited to
/// changes that can be expressed as a text patch. A binary file the row rewrites
/// has no hunks to apply, and a staged submodule pointer none either.
#[test]
fn a_row_separates_from_staged_binary_and_submodule_changes() {
    let test = common::TestRepo::new();
    let first = test.commit_file("a.txt", "a1\n", "base");
    std::fs::write(
        test.repo.workdir().unwrap().join("bin.dat"),
        [0u8, 1, 2, 0, 3],
    )
    .unwrap();
    test.stage_file("bin.dat");
    test.commit("add binary");

    // Unstaged: rewrite the binary. Staged: a text edit and a submodule pointer.
    std::fs::write(
        test.repo.workdir().unwrap().join("bin.dat"),
        [0u8, 9, 9, 0, 7],
    )
    .unwrap();
    test.write_file("a.txt", "a2\n");
    test.stage_file("a.txt");
    test.stage_gitlink("sub", first);
    let git_repo = test.git_repo();

    let started = git_repo
        .begin_worktree_source(WorktreeSource::Unstaged)
        .unwrap()
        .expect("the binary rewrite is the unstaged row");

    // Only the binary went into the temporary commit.
    let diff = git_repo.commit_diff(&started.temp_oid, 3).unwrap();
    assert_eq!(
        diff.files
            .iter()
            .filter_map(|f| f.new_path.clone())
            .collect::<Vec<_>>(),
        vec!["bin.dat".to_string()]
    );
    let mut staged = row_paths(git_repo.staged_diff(3).unwrap());
    staged.sort();
    assert_eq!(staged, ["a.txt", "sub"]);
    assert_eq!(
        std::fs::read(test.repo.workdir().unwrap().join("bin.dat")).unwrap(),
        [0u8, 9, 9, 0, 7]
    );
}

/// A staged submodule pointer must survive a fold of the staged row.
///
/// Submodule contents belong to neither row, so a fold has no business dropping
/// the pointer — nor tripping over the directory it points at.
#[test]
fn a_staged_submodule_pointer_survives_a_fold() {
    let test = common::TestRepo::new();
    let first = test.commit_file("a.txt", "a1\n", "base");
    test.commit_file("t.txt", "t\n", "second");
    test.write_file("a.txt", "a2\n");
    test.stage_file("a.txt");
    test.stage_gitlink("sub", first);
    let git_repo = test.git_repo();

    let started = git_repo
        .begin_worktree_source(WorktreeSource::Staged)
        .unwrap()
        .expect("the staged row has changes to fold in");

    assert!(
        test.repo
            .find_tree(git2::Oid::from(&started.snapshot.worktree_tree))
            .unwrap()
            .get_path(std::path::Path::new("sub"))
            .is_ok(),
        "the recorded working tree must keep the submodule pointer"
    );
    git_repo.abort_worktree_source(&started.snapshot).unwrap();
    assert!(
        test.repo.workdir().unwrap().join("sub").exists(),
        "unwinding must not trip over the submodule directory"
    );
    let mut staged = row_paths(git_repo.staged_diff(3).unwrap());
    staged.sort();
    assert_eq!(staged, ["a.txt", "sub"]);
}

/// A snapshot that is no longer the operation in hand must not be applied to
/// whatever operation *is*.
///
/// Completing an operation consults the recorded snapshot, because that is how a
/// fold resumed after a conflict finishes. A record stranded by something else
/// would otherwise be restored over an unrelated operation's result, and
/// recorded as that operation's undo entry.
#[test]
fn a_stale_snapshot_is_not_applied_to_another_operation() {
    let test = common::TestRepo::new();
    let base = test.commit_file("keep.txt", "keep\n", "base");
    let victim = test.commit_file("victim.txt", "v\n", "victim");
    test.commit_file("top.txt", "t\n", "top");
    test.write_file("keep.txt", "staged\n");
    test.stage_file("keep.txt");
    let git_repo = test.git_repo();

    let started = git_repo
        .begin_worktree_source(WorktreeSource::Staged)
        .unwrap()
        .unwrap();

    // Strand the record: the branch goes back but nothing clears it, as a failed
    // unwind or a failed recovery would leave things.
    let tip_before = git2::Oid::from(&started.snapshot.tip_before);
    test.repo
        .reference("refs/heads/main", tip_before, true, "strand")
        .unwrap();
    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.force();
    test.repo
        .checkout_tree(
            &test.repo.find_commit(tip_before).unwrap().into_object(),
            Some(&mut checkout),
        )
        .unwrap();
    test.repo
        .index()
        .unwrap()
        .read_tree(&test.repo.find_commit(tip_before).unwrap().tree().unwrap())
        .unwrap();

    let head = git_repo.head_oid().unwrap();
    assert_rebase_complete!(git_repo.drop_commit(&Oid::from(victim), &head).unwrap());

    assert_history!(&test, base, &["top"]);
    assert_eq!(
        workdir(&test, "keep.txt"),
        "keep\n",
        "the stale snapshot's working tree must not be restored over the drop"
    );
    assert_eq!(
        undo_kinds(&test),
        ["RefMove"],
        "the drop must record its own undo entry, not a fold's"
    );
}

/// Discarding a journal must discard the snapshot with it.
///
/// A snapshot left behind is not inert: it tells every later operation that the
/// dirty working tree is accounted for, so the guard that protects uncommitted
/// changes stops firing — and the next startup would "recover" it by resetting
/// the branch and working tree to a state that has nothing to do with the
/// repository any more.
#[test]
fn discarding_the_journal_discards_the_snapshot() {
    let test = common::TestRepo::new();
    let base = test.commit_file("base.txt", "base\n", "base");
    test.commit_files(&[("a.txt", "a1\n"), ("b.txt", "b1\n")], "second");
    test.write_file("a.txt", "a2\n");
    test.stage_file("a.txt");
    test.write_file("b.txt", "b2\n");
    let git_repo = test.git_repo();
    git_repo
        .begin_worktree_source(WorktreeSource::Staged)
        .unwrap()
        .unwrap();

    git_repo.clear_journal().unwrap();

    assert!(
        matches!(
            git_repo.read_journal().unwrap(),
            git_tailor::repo::JournalStatus::None
        ),
        "a cleared journal must not still offer the snapshot for recovery"
    );

    // The guard on uncommitted changes must be back in force.
    let head = git_repo.head_oid().unwrap();
    let victim = git_repo.list_commits(&head, &Oid::from(base)).unwrap()[0]
        .oid
        .expect_real_oid();
    let err = git_repo
        .drop_commit(&victim, &head)
        .expect_err("a dirty working tree must still refuse a rewrite");
    assert!(
        format!("{err:#}").contains("staged or unstaged changes"),
        "got: {err:#}"
    );
}

/// A stale snapshot from some earlier run must not disable the guard either.
/// The snapshot only makes the dirty tree safe while it still describes it.
#[test]
fn a_snapshot_that_no_longer_matches_the_working_tree_does_not_disable_the_guard() {
    let test = common::TestRepo::new();
    let base = test.commit_file("base.txt", "base\n", "base");
    test.commit_files(&[("a.txt", "a1\n"), ("b.txt", "b1\n")], "second");
    test.write_file("a.txt", "a2\n");
    test.stage_file("a.txt");
    test.write_file("b.txt", "b2\n");
    let git_repo = test.git_repo();
    git_repo
        .begin_worktree_source(WorktreeSource::Staged)
        .unwrap()
        .unwrap();

    // The user carries on working, so the recorded working tree is history.
    test.write_file("b.txt", "b3\n");

    let head = git_repo.head_oid().unwrap();
    let victim = git_repo.list_commits(&head, &Oid::from(base)).unwrap()[0]
        .oid
        .expect_real_oid();
    let err = git_repo
        .drop_commit(&victim, &head)
        .expect_err("a working tree the snapshot no longer covers must refuse a rewrite");
    assert!(
        format!("{err:#}").contains("staged or unstaged changes"),
        "got: {err:#}"
    );
}

/// Pruning must leave a fold in flight alone. Its temporary commit puts the
/// branch somewhere the undo stack does not expect, which is not the same thing
/// as history having been changed behind git-tailor's back.
#[test]
fn pruning_keeps_the_undo_history_of_a_fold_in_flight() {
    let test = common::TestRepo::new();
    test.commit_file("base.txt", "base\n", "base");
    let second = test.commit_file("a.txt", "a1\n", "second");
    test.commit_file("b.txt", "b1\n", "third");
    let git_repo = test.git_repo();

    // An earlier, completed operation to have some history worth keeping.
    let head = git_repo.head_oid().unwrap();
    git_repo
        .reword_commit(&Oid::from(second), "second reworded", &head)
        .unwrap();
    assert!(matches!(
        git_repo.read_journal().unwrap(),
        git_tailor::repo::JournalStatus::None
    ));

    test.write_file("a.txt", "a2\n");
    test.stage_file("a.txt");
    let started = git_repo
        .begin_worktree_source(WorktreeSource::Staged)
        .unwrap()
        .unwrap();

    git_repo.prune_stale_journal().unwrap();

    assert_eq!(
        undo_labels(&test),
        ["Reword"],
        "the undo stack must survive"
    );
    git_repo.abort_worktree_source(&started.snapshot).unwrap();
    assert_eq!(undo_labels(&test), ["Reword"]);
}

/// The record kinds on the recorded undo stack, oldest first.
fn undo_kinds(test: &common::TestRepo) -> Vec<String> {
    journal_doc(test)["undo"]
        .as_array()
        .map(|records| {
            records
                .iter()
                .map(|r| r["kind"].as_str().unwrap_or_default().to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// The parsed journal document, or null when there is none.
fn journal_doc(test: &common::TestRepo) -> serde_json::Value {
    let path = test.repo.path().join("git-tailor").join("journal.json");
    match std::fs::read_to_string(&path) {
        Ok(raw) => serde_json::from_str(&raw).unwrap(),
        Err(_) => serde_json::Value::Null,
    }
}

/// The labels on the recorded undo stack, oldest first.
fn undo_labels(test: &common::TestRepo) -> Vec<String> {
    journal_doc(test)["undo"]
        .as_array()
        .map(|records| {
            records
                .iter()
                .map(|r| r["label"].as_str().unwrap_or_default().to_string())
                .collect()
        })
        .unwrap_or_default()
}
