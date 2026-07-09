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

//! Integration tests for `commit_staged` and its undo/redo, which is journalled
//! as a soft ref move (`reset --soft`) — undo leaves the index and working tree
//! alone, so the committed changes reappear as staged.

#[allow(dead_code)]
mod common;

use common::prelude::*;
use git_tailor::repo::{CommitOutcome, UndoOutcome};

fn head_commit(test: &common::TestRepo) -> git2::Commit<'_> {
    test.repo.head().unwrap().peel_to_commit().unwrap()
}

fn undo_pins(test: &common::TestRepo) -> Vec<git2::Oid> {
    test.repo
        .references_glob("refs/git-tailor/undo/*")
        .unwrap()
        .filter_map(|r| r.ok())
        .filter_map(|r| r.target())
        .collect()
}

fn expect_done(outcome: UndoOutcome) -> String {
    match outcome {
        UndoOutcome::Done { label } => label,
        other => panic!("expected Done, got {other:?}"),
    }
}

/// Base commit with `a.txt` and `b.txt`, then a staged change to `a.txt` and an
/// unstaged change to `b.txt`.
fn repo_with_staged_and_unstaged() -> common::TestRepo {
    let test = common::TestRepo::new();
    test.commit_files(&[("a.txt", "a\n"), ("b.txt", "b\n")], "base");
    test.write_file("a.txt", "a changed\n");
    test.stage_file("a.txt");
    test.write_file("b.txt", "b changed\n"); // unstaged
    test
}

#[test]
fn commit_staged_creates_a_commit_from_the_index() {
    let test = repo_with_staged_and_unstaged();
    let git_repo = test.git_repo();
    let parent = head_commit(&test).id();
    let staged_tree = {
        let mut index = test.repo.index().unwrap();
        index.read(true).unwrap();
        index.write_tree().unwrap()
    };

    assert_eq!(
        git_repo.commit_staged("add a change\n").unwrap(),
        CommitOutcome::Committed
    );

    let new = head_commit(&test);
    assert_ne!(new.id(), parent, "HEAD should advance");
    assert_eq!(new.parent(0).unwrap().id(), parent);
    assert_eq!(new.message().unwrap(), "add a change\n");
    assert_eq!(new.tree().unwrap().id(), staged_tree);

    // The staged change is now committed; the unstaged change remains.
    assert!(git_repo.staged_diff(3).unwrap().is_none());
    assert!(git_repo.unstaged_diff(3).unwrap().is_some());
}

#[test]
fn commit_staged_with_nothing_staged_is_a_noop() {
    let test = common::TestRepo::new();
    test.commit_file("a.txt", "a\n", "base");
    test.write_file("a.txt", "a changed\n"); // unstaged only
    let git_repo = test.git_repo();
    let head = head_commit(&test).id();

    assert_eq!(
        git_repo.commit_staged("nope\n").unwrap(),
        CommitOutcome::NothingStaged
    );
    assert_eq!(head_commit(&test).id(), head, "HEAD must not move");
    assert!(undo_pins(&test).is_empty(), "a no-op records no undo entry");
}

#[test]
fn undo_commit_is_a_soft_reset_and_redo_recommits() {
    let test = repo_with_staged_and_unstaged();
    let git_repo = test.git_repo();
    let parent = head_commit(&test).id();
    git_repo.commit_staged("add a change\n").unwrap();
    let committed = head_commit(&test).id();

    // Undo soft-resets to the parent: the committed change is staged again and
    // the unstaged change is untouched.
    assert_eq!(expect_done(git_repo.undo().unwrap()), "Commit");
    assert_eq!(head_commit(&test).id(), parent);
    assert!(git_repo.staged_diff(3).unwrap().is_some());
    assert!(git_repo.unstaged_diff(3).unwrap().is_some());
    assert_eq!(
        std::fs::read_to_string(test.repo.workdir().unwrap().join("b.txt")).unwrap(),
        "b changed\n",
        "the unstaged working-tree change must be preserved"
    );

    // Redo re-creates the same commit.
    assert_eq!(expect_done(git_repo.redo().unwrap()), "Commit");
    assert_eq!(head_commit(&test).id(), committed);
    assert!(git_repo.staged_diff(3).unwrap().is_none());
}

#[test]
fn undo_commit_is_stale_when_head_moved_externally() {
    let test = repo_with_staged_and_unstaged();
    let git_repo = test.git_repo();
    git_repo.commit_staged("add a change\n").unwrap();

    // Another commit lands outside git-tailor.
    test.commit_file("c.txt", "c\n", "external commit");

    assert!(matches!(git_repo.undo().unwrap(), UndoOutcome::Stale));
    assert!(undo_pins(&test).is_empty());
}

#[test]
fn redo_commit_is_stale_when_head_moved_externally() {
    let test = repo_with_staged_and_unstaged();
    let git_repo = test.git_repo();
    git_repo.commit_staged("add a change\n").unwrap();
    git_repo.undo().unwrap(); // commit now sits on the redo stack, HEAD at parent

    // HEAD moves outside git-tailor, so the redo target no longer matches.
    test.commit_file("c.txt", "c\n", "external commit");

    assert!(matches!(git_repo.redo().unwrap(), UndoOutcome::Stale));
    assert!(undo_pins(&test).is_empty());
}

#[test]
fn commit_staged_errors_on_conflicted_index() {
    let test = common::TestRepo::new();
    let _state = test.make_drop_conflict();
    let git_repo = test.git_repo();

    assert!(
        git_repo.commit_staged("x\n").is_err(),
        "committing must refuse a conflicted index"
    );
}
