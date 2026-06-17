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

//! Integration tests for undo/redo of history-rewriting operations.

#[allow(dead_code)]
mod common;

use common::prelude::*;
use git_tailor::repo::UndoOutcome;

fn head_oid(test: &common::TestRepo) -> git2::Oid {
    test.repo.head().unwrap().target().unwrap()
}

fn expect_done(outcome: UndoOutcome) -> String {
    match outcome {
        UndoOutcome::Done { label } => label,
        other => panic!("expected Done, got {other:?}"),
    }
}

fn summaries(test: &common::TestRepo, base: git2::Oid) -> Vec<String> {
    test.commits_from_head(base)
        .iter()
        .map(|oid| {
            let commit = test.repo.find_commit(*oid).unwrap();
            commit.summary().unwrap().unwrap_or("").to_string()
        })
        .collect()
}

fn undo_pin_count(test: &common::TestRepo) -> usize {
    test.repo
        .references_glob("refs/git-tailor/undo/*")
        .unwrap()
        .count()
}

#[test]
fn undo_restores_history_and_redo_reapplies() {
    let test = common::TestRepo::new();
    let base = test.commit_file("a.txt", "a\n", "base");
    let c1 = test.commit_file("b.txt", "b\n", "add b");
    test.commit_file("c.txt", "c\n", "add c");

    let git_repo = test.git_repo();

    // Drop the middle commit (independent file — no conflict).
    assert_rebase_complete!(
        git_repo
            .drop_commit(&Oid::from(c1), &Oid::from(head_oid(&test)))
            .unwrap()
    );
    assert_eq!(test.commits_from_head(base).len(), 1);
    assert!(
        undo_pin_count(&test) >= 1,
        "tips should be pinned after a recorded op"
    );

    // Undo restores the dropped commit and its file.
    assert_eq!(expect_done(git_repo.undo().unwrap()), "Drop");
    assert_eq!(test.commits_from_head(base).len(), 2);
    assert_file_contents_at_head!(&test.repo, "b.txt", "b\n");

    // Redo drops it again.
    assert_eq!(expect_done(git_repo.redo().unwrap()), "Drop");
    assert_eq!(test.commits_from_head(base).len(), 1);
}

#[test]
fn multi_level_undo_redo() {
    let test = common::TestRepo::new();
    let base = test.commit_file("a.txt", "a\n", "base");
    let c1 = test.commit_file("b.txt", "b\n", "add b");
    let c2 = test.commit_file("c.txt", "c\n", "add c");

    let git_repo = test.git_repo();

    git_repo
        .reword_commit(&Oid::from(c2), "c reworded", &Oid::from(head_oid(&test)))
        .unwrap();
    git_repo
        .reword_commit(&Oid::from(c1), "b reworded", &Oid::from(head_oid(&test)))
        .unwrap();

    // Undo both rewords back to the original summaries.
    assert_eq!(expect_done(git_repo.undo().unwrap()), "Reword");
    assert_eq!(expect_done(git_repo.undo().unwrap()), "Reword");
    let msgs = summaries(&test, base);
    assert!(
        msgs.iter().any(|m| m == "add b") && msgs.iter().any(|m| m == "add c"),
        "expected original summaries, got {msgs:?}"
    );

    // Redo both.
    assert_eq!(expect_done(git_repo.redo().unwrap()), "Reword");
    assert_eq!(expect_done(git_repo.redo().unwrap()), "Reword");
    let msgs = summaries(&test, base);
    assert!(
        msgs.iter().any(|m| m == "b reworded") && msgs.iter().any(|m| m == "c reworded"),
        "expected reworded summaries, got {msgs:?}"
    );
}

#[test]
fn new_operation_clears_redo() {
    let test = common::TestRepo::new();
    let base = test.commit_file("a.txt", "a\n", "base");
    let c1 = test.commit_file("b.txt", "b\n", "add b");
    let c2 = test.commit_file("c.txt", "c\n", "add c");

    let git_repo = test.git_repo();

    git_repo
        .drop_commit(&Oid::from(c1), &Oid::from(head_oid(&test)))
        .unwrap();
    expect_done(git_repo.undo().unwrap()); // redo now holds the drop

    // A fresh operation must clear the redo stack.
    git_repo
        .drop_commit(&Oid::from(c2), &Oid::from(head_oid(&test)))
        .unwrap();
    assert!(
        matches!(git_repo.redo().unwrap(), UndoOutcome::Empty),
        "a new operation should clear redo"
    );
    let _ = base;
}

#[test]
fn external_history_change_invalidates_stack() {
    let test = common::TestRepo::new();
    test.commit_file("a.txt", "a\n", "base");
    let c1 = test.commit_file("b.txt", "b\n", "add b");
    test.commit_file("c.txt", "c\n", "add c");

    let git_repo = test.git_repo();
    git_repo
        .drop_commit(&Oid::from(c1), &Oid::from(head_oid(&test)))
        .unwrap();

    // Simulate the user committing outside git-tailor.
    test.commit_file("d.txt", "d\n", "external commit");

    assert!(matches!(git_repo.undo().unwrap(), UndoOutcome::Stale));
    // Stack discarded: a second undo finds nothing, and pins are gone.
    assert!(matches!(git_repo.undo().unwrap(), UndoOutcome::Empty));
    assert_eq!(
        undo_pin_count(&test),
        0,
        "stale invalidation should drop pins"
    );
}

#[test]
fn undo_persists_across_reopen() {
    let test = common::TestRepo::new();
    let base = test.commit_file("a.txt", "a\n", "base");
    let c1 = test.commit_file("b.txt", "b\n", "add b");
    test.commit_file("c.txt", "c\n", "add c");

    {
        let git_repo = test.git_repo();
        git_repo
            .drop_commit(&Oid::from(c1), &Oid::from(head_oid(&test)))
            .unwrap();
    }

    // A brand-new handle (simulating a restart) can still undo.
    let git_repo = test.git_repo();
    assert_eq!(expect_done(git_repo.undo().unwrap()), "Drop");
    assert_eq!(test.commits_from_head(base).len(), 2);
}

#[test]
fn undo_refuses_when_working_tree_dirty() {
    let test = common::TestRepo::new();
    test.commit_file("a.txt", "a\n", "base");
    let c1 = test.commit_file("b.txt", "b\n", "add b");
    test.commit_file("c.txt", "c\n", "add c");

    let git_repo = test.git_repo();
    git_repo
        .drop_commit(&Oid::from(c1), &Oid::from(head_oid(&test)))
        .unwrap();

    // Unstaged modification to a tracked file.
    test.write_file("a.txt", "dirty\n");
    assert!(
        git_repo.undo().is_err(),
        "undo must refuse to discard a dirty working tree"
    );
}

#[test]
fn undo_and_redo_empty_with_no_history() {
    let test = common::TestRepo::new();
    test.commit_file("a.txt", "a\n", "base");

    let git_repo = test.git_repo();
    assert!(matches!(git_repo.undo().unwrap(), UndoOutcome::Empty));
    assert!(matches!(git_repo.redo().unwrap(), UndoOutcome::Empty));
}
