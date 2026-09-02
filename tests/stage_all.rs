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

//! Integration tests for `stage_all` / `unstage_all` and their undo/redo, which
//! are journalled as index-only operations (the branch ref never moves).

#[allow(dead_code)]
mod common;

use common::prelude::*;
use git_tailor::repo::{StageOutcome, UndoOutcome};
use std::path::Path;

/// Open a fresh handle and return `(staged, unstaged)` file paths, isolated from
/// any operating handle's cached index.
fn staged_and_unstaged(gitdir: &Path) -> (Vec<String>, Vec<String>) {
    let repo = git2::Repository::open(gitdir).unwrap();
    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true);
    let statuses = repo.statuses(Some(&mut opts)).unwrap();

    let staged_mask = git2::Status::INDEX_NEW
        | git2::Status::INDEX_MODIFIED
        | git2::Status::INDEX_DELETED
        | git2::Status::INDEX_RENAMED
        | git2::Status::INDEX_TYPECHANGE;
    let unstaged_mask = git2::Status::WT_NEW
        | git2::Status::WT_MODIFIED
        | git2::Status::WT_DELETED
        | git2::Status::WT_RENAMED
        | git2::Status::WT_TYPECHANGE;

    let mut staged = Vec::new();
    let mut unstaged = Vec::new();
    for entry in statuses.iter() {
        let path = entry.path().unwrap_or_default().to_string();
        if entry.status().intersects(staged_mask) {
            staged.push(path.clone());
        }
        if entry.status().intersects(unstaged_mask) {
            unstaged.push(path);
        }
    }
    staged.sort();
    unstaged.sort();
    (staged, unstaged)
}

fn read_workdir(test: &common::TestRepo, path: &str) -> String {
    std::fs::read_to_string(test.repo.workdir().unwrap().join(path)).unwrap()
}

fn head_oid(test: &common::TestRepo) -> git2::Oid {
    test.repo.head().unwrap().target().unwrap()
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

/// A base commit with `a.txt` and `b.txt`, then a mixed dirty working tree: a
/// modification to `a.txt`, an untracked `c.txt`, and a deleted `b.txt`.
fn dirty_repo() -> common::TestRepo {
    let test = common::TestRepo::new();
    test.commit_files(&[("a.txt", "a\n"), ("b.txt", "b\n")], "base");
    test.write_file("a.txt", "a changed\n");
    test.write_file("c.txt", "c\n");
    std::fs::remove_file(test.repo.workdir().unwrap().join("b.txt")).unwrap();
    test
}

#[test]
fn stage_all_stages_modifications_and_deletions_but_not_untracked_files() {
    let test = dirty_repo();
    let git_repo = test.git_repo();

    assert_eq!(git_repo.stage_all().unwrap(), StageOutcome::Changed);

    let (staged, unstaged) = staged_and_unstaged(test.repo.path());
    assert_eq!(staged, vec!["a.txt", "b.txt"]);
    assert_eq!(
        unstaged,
        vec!["c.txt"],
        "an untracked file must not be staged"
    );
}

#[test]
fn unstage_all_resets_index_to_head() {
    let test = dirty_repo();
    let git_repo = test.git_repo();
    git_repo.stage_all().unwrap();

    assert_eq!(git_repo.unstage_all().unwrap(), StageOutcome::Changed);

    let (staged, unstaged) = staged_and_unstaged(test.repo.path());
    assert!(staged.is_empty(), "nothing should remain staged");
    assert_eq!(unstaged, vec!["a.txt", "b.txt", "c.txt"]);
    // The working tree is untouched by (un)staging.
    assert_eq!(read_workdir(&test, "a.txt"), "a changed\n");
}

#[test]
fn stage_all_is_noop_on_clean_tree() {
    let test = common::TestRepo::new();
    test.commit_file("a.txt", "a\n", "base");
    let git_repo = test.git_repo();

    assert_eq!(git_repo.stage_all().unwrap(), StageOutcome::NoOp);
    assert!(
        undo_pins(&test).is_empty(),
        "a no-op must not record an undo entry"
    );
}

#[test]
fn stage_all_is_noop_when_already_staged() {
    let test = dirty_repo();
    let git_repo = test.git_repo();
    assert_eq!(git_repo.stage_all().unwrap(), StageOutcome::Changed);

    assert_eq!(git_repo.stage_all().unwrap(), StageOutcome::NoOp);
}

#[test]
fn unstage_all_is_noop_when_nothing_staged() {
    let test = common::TestRepo::new();
    test.commit_file("a.txt", "a\n", "base");
    test.write_file("a.txt", "a changed\n"); // unstaged only
    let git_repo = test.git_repo();

    assert_eq!(git_repo.unstage_all().unwrap(), StageOutcome::NoOp);
}

#[test]
fn undo_stage_all_moves_changes_back_to_unstaged_without_touching_worktree() {
    let test = dirty_repo();
    let git_repo = test.git_repo();
    git_repo.stage_all().unwrap();
    let tip = head_oid(&test);

    assert_eq!(expect_done(git_repo.undo().unwrap()), "Stage all");

    let (staged, unstaged) = staged_and_unstaged(test.repo.path());
    assert!(staged.is_empty(), "undo should clear the staged changes");
    assert_eq!(unstaged, vec!["a.txt", "b.txt", "c.txt"]);
    assert_eq!(head_oid(&test), tip, "undo must not move the branch ref");
    assert_eq!(read_workdir(&test, "a.txt"), "a changed\n");

    // Redo re-stages the tracked changes.
    assert_eq!(expect_done(git_repo.redo().unwrap()), "Stage all");
    let (staged, unstaged) = staged_and_unstaged(test.repo.path());
    assert_eq!(staged, vec!["a.txt", "b.txt"]);
    assert_eq!(unstaged, vec!["c.txt"]);
}

#[test]
fn undo_unstage_all_restages() {
    let test = dirty_repo();
    let git_repo = test.git_repo();
    git_repo.stage_all().unwrap();
    git_repo.unstage_all().unwrap();

    assert_eq!(expect_done(git_repo.undo().unwrap()), "Unstage all");

    let (staged, unstaged) = staged_and_unstaged(test.repo.path());
    assert_eq!(staged, vec!["a.txt", "b.txt"]);
    assert_eq!(unstaged, vec!["c.txt"]);
}

#[test]
fn stage_all_pins_the_index_tree_against_gc() {
    let test = dirty_repo();
    let git_repo = test.git_repo();
    git_repo.stage_all().unwrap();

    let pins = undo_pins(&test);
    assert!(!pins.is_empty(), "the index trees should be pinned");
    // Every pin must resolve to a real tree object (so gc keeps it and its blobs).
    for oid in &pins {
        assert!(
            test.repo.find_tree(*oid).is_ok(),
            "pinned object {oid} should be a reachable tree"
        );
    }
    // The post-op index tree is among the pins.
    let mut index = test.repo.index().unwrap();
    index.read(true).unwrap();
    let staged_tree = index.write_tree().unwrap();
    assert!(pins.contains(&staged_tree));
}

#[test]
fn index_and_ref_ops_undo_in_lifo_order() {
    let test = common::TestRepo::new();
    let base = test.commit_file("a.txt", "a\n", "base");
    let c1 = test.commit_file("b.txt", "b\n", "add b");
    test.commit_file("c.txt", "c\n", "add c");
    let git_repo = test.git_repo();

    // Ref-moving op first: drop the independent middle commit.
    assert_rebase_complete!(
        git_repo
            .drop_commit(&Oid::from(c1), &Oid::from(head_oid(&test)))
            .unwrap()
    );
    assert_eq!(test.commits_from_head(base).len(), 1);

    // Index-only op next.
    test.write_file("a.txt", "a changed\n");
    assert_eq!(git_repo.stage_all().unwrap(), StageOutcome::Changed);

    // Undo pops the index op first.
    assert_eq!(expect_done(git_repo.undo().unwrap()), "Stage all");
    let (staged, _) = staged_and_unstaged(test.repo.path());
    assert!(staged.is_empty(), "the staged a.txt should be reverted");

    // Undoing the stage leaves the change unstaged, and the ref undo below
    // refuses a dirty tree, so put the file back the way it was.
    test.write_file("a.txt", "a\n");

    // Then the ref op, restoring the dropped commit.
    assert_eq!(expect_done(git_repo.undo().unwrap()), "Drop");
    assert_eq!(test.commits_from_head(base).len(), 2);
    assert_file_contents_at_head!(&test.repo, "b.txt", "b\n");
}

#[test]
fn undo_is_stale_when_index_changed_externally() {
    let test = dirty_repo();
    let git_repo = test.git_repo();
    git_repo.stage_all().unwrap();

    // Stage an additional change outside this undo history.
    test.write_file("e.txt", "e\n");
    test.stage_file("e.txt");

    assert!(matches!(git_repo.undo().unwrap(), UndoOutcome::Stale));
    assert!(
        undo_pins(&test).is_empty(),
        "a stale stack is discarded along with its pins"
    );
}

#[test]
fn redo_is_stale_when_index_changed_externally() {
    let test = dirty_repo();
    let git_repo = test.git_repo();
    git_repo.stage_all().unwrap();
    git_repo.undo().unwrap(); // now the op sits on the redo stack

    // Stage something outside this history, so the index no longer matches the
    // pre-op tree the redo expects to restore from.
    test.write_file("e.txt", "e\n");
    test.stage_file("e.txt");

    assert!(matches!(git_repo.redo().unwrap(), UndoOutcome::Stale));
    assert!(undo_pins(&test).is_empty());
}

#[test]
fn stage_all_undo_persists_across_reopen() {
    let test = dirty_repo();
    {
        let git_repo = test.git_repo();
        git_repo.stage_all().unwrap();
    }

    // A brand-new handle (simulating a restart) reads the journal and can still
    // undo the index-only operation.
    let git_repo = test.git_repo();
    assert_eq!(expect_done(git_repo.undo().unwrap()), "Stage all");
    let (staged, unstaged) = staged_and_unstaged(test.repo.path());
    assert!(staged.is_empty());
    assert_eq!(unstaged, vec!["a.txt", "b.txt", "c.txt"]);
}

#[test]
fn prune_keeps_index_only_undo_across_reopen() {
    let test = dirty_repo();
    let git_repo = test.git_repo();
    git_repo.stage_all().unwrap();
    let pins_before = undo_pins(&test).len();
    assert!(pins_before >= 1);

    // The ref never moved, so a startup-time prune keeps the index-only record
    // (and its tree pins) and undo still works.
    git_repo.prune_stale_journal().unwrap();
    assert_eq!(undo_pins(&test).len(), pins_before);
    assert_eq!(expect_done(git_repo.undo().unwrap()), "Stage all");
}

#[test]
fn stage_all_errors_on_conflicted_index() {
    let test = common::TestRepo::new();
    let _state = test.make_drop_conflict();
    let git_repo = test.git_repo();

    assert!(
        git_repo.stage_all().is_err(),
        "staging must refuse a conflicted index"
    );
}
