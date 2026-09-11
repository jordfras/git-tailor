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

//! The undo pins, verified against the thing they exist to survive.
//!
//! git-tailor never shells out to the `git` CLI (see CLAUDE.md) — these tests
//! deliberately do. The pins under `refs/git-tailor/` have exactly one job:
//! stop `git gc` collecting the commits the undo stack still needs while
//! nothing else references them. `gc` is git's, not libgit2's, so no amount of
//! exercising git-tailor can show whether the pins work. Only running the real
//! thing can, and the cost of being wrong is the user's history.
//!
//! This is also not theoretical. The per-worktree ref namespace looks like the
//! obvious way to stop one worktree's run disturbing another's pins, and a
//! probe here showed `git gc` prunes straight through it.

#[allow(dead_code)]
mod common;

use common::prelude::*;
use git_tailor::repo::{UndoOutcome, WorktreeSource};
use std::path::Path;

/// Run `git gc --prune=now` in `dir`, the most aggressive form a user can
/// invoke, and the one a pin has to survive.
fn git_gc(dir: &Path) {
    let out = std::process::Command::new("git")
        .args(["gc", "--prune=now"])
        .current_dir(dir)
        .output()
        .expect("git must be on PATH to run these tests");
    assert!(
        out.status.success(),
        "git gc failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The whole point of the pins: an aggressive gc must not cost the user their
/// undo history.
#[test]
fn undo_survives_an_aggressive_gc() {
    let test = common::TestRepo::new();
    let base = test.commit_file("a.txt", "v1\n", "base");
    let c1 = test.commit_file("b.txt", "b\n", "c1");
    let c2 = test.commit_file("c.txt", "c\n", "c2");
    let head = test.commit_file("d.txt", "d\n", "head");
    let _ = base;

    let mut git_repo = test.git_repo();
    assert_rebase_complete!(
        git_repo
            .drop_commit(&Oid::from(c1), &Oid::from(head))
            .unwrap()
    );
    let head_after_first = git_repo.head_oid().unwrap();
    assert_rebase_complete!(
        git_repo
            .drop_commit(&Oid::from(c2), &head_after_first)
            .unwrap()
    );
    let tip = git_repo.head_oid().unwrap();

    git_gc(test.repo.workdir().unwrap());

    // A fresh handle, as the next run of git-tailor would be.
    let mut after = test.git_repo();
    assert!(matches!(after.undo().unwrap(), UndoOutcome::Done { .. }));
    assert_eq!(after.head_oid().unwrap(), head_after_first);
    assert!(matches!(after.undo().unwrap(), UndoOutcome::Done { .. }));
    assert_ne!(after.head_oid().unwrap(), tip);
}

/// Two working trees on one repository. Each keeps its own journal, so each has
/// its own undo history — and one of them running an operation must not leave
/// the other's history to be collected.
#[test]
fn a_second_worktree_does_not_unpin_the_first() {
    let test = common::TestRepo::new();
    let base = test.commit_file("a.txt", "v1\n", "base");
    let c1 = test.commit_file("b.txt", "b\n", "c1");
    let head = test.commit_file("c.txt", "c\n", "head");
    let _ = base;

    let wt_dir = std::env::temp_dir().join(format!("gt-wt-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&wt_dir);
    let branch = test
        .repo
        .branch("second", &test.repo.find_commit(head).unwrap(), false)
        .unwrap();
    let mut opts = git2::WorktreeAddOptions::new();
    let reference = branch.into_reference();
    opts.reference(Some(&reference));
    test.repo.worktree("gtwt", &wt_dir, Some(&opts)).unwrap();

    // An operation in the main working tree, leaving undo history behind.
    let mut main_repo = test.git_repo();
    assert_rebase_complete!(
        main_repo
            .drop_commit(&Oid::from(c1), &Oid::from(head))
            .unwrap()
    );
    let main_before_undo = main_repo.head_oid().unwrap();

    // An operation in the linked working tree, which syncs its own pins.
    let mut linked = Git2Repo::open(wt_dir.clone()).unwrap();
    let linked_head = linked.head_oid().unwrap();
    assert_rebase_complete!(linked.drop_commit(&Oid::from(c1), &linked_head).unwrap());

    git_gc(test.repo.workdir().unwrap());

    // The main working tree's undo history is still there to use.
    let mut after = test.git_repo();
    assert!(
        matches!(after.undo().unwrap(), UndoOutcome::Done { .. }),
        "the other working tree's run must not cost this one its undo history"
    );
    assert_ne!(after.head_oid().unwrap(), main_before_undo);

    let _ = std::fs::remove_dir_all(&wt_dir);
}

/// The undo tips above are also reachable through the branch reflog, so the
/// pins are belt-and-braces for them. These are not: a lift records the whole
/// working tree as a tree object that no commit and no reflog ever names. The
/// pin is the only thing between it and `git gc`, and the thing it holds is the
/// user's uncommitted work.
#[test]
fn an_interrupted_folds_working_tree_survives_an_aggressive_gc() {
    let test = common::TestRepo::new();
    let _base = test.commit_file("base.txt", "base\n", "base");
    let target = test.commit_files(&[("a.txt", "a1\n"), ("b.txt", "b1\n")], "target commit");
    test.commit_file("c.txt", "c1\n", "later commit");
    let _ = target;

    test.write_file("a.txt", "a2 STAGED\n");
    test.stage_file("a.txt");
    test.write_file("b.txt", "b2 UNSTAGED\n");

    let mut git_repo = test.git_repo();
    let lifted = git_repo
        .lift_worktree_row(WorktreeSource::Staged)
        .unwrap()
        .expect("the staged row has changes");

    // Killed here, mid-fold: the branch sits on the temporary commit and the
    // journal holds the trees needed to unwind.
    git_gc(test.repo.workdir().unwrap());

    let mut after = test.git_repo();
    after
        .restore_lifted_row(&lifted)
        .expect("the recorded working tree must still be reachable after a gc");

    assert_eq!(
        std::fs::read_to_string(test.repo.workdir().unwrap().join("b.txt")).unwrap(),
        "b2 UNSTAGED\n",
        "the unstaged row is only reachable through the pin"
    );
}

/// The same, with another working tree's run in between — which is where the
/// shared pin namespace bites.
#[test]
fn a_second_worktree_does_not_unpin_an_interrupted_fold() {
    let test = common::TestRepo::new();
    let _base = test.commit_file("base.txt", "base\n", "base");
    test.commit_files(&[("a.txt", "a1\n"), ("b.txt", "b1\n")], "target commit");
    let c1 = test.commit_file("c.txt", "c1\n", "later commit");
    let head = test.commit_file("d.txt", "d1\n", "head");

    let wt_dir = std::env::temp_dir().join(format!("gt-fold-wt-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&wt_dir);
    let branch = test
        .repo
        .branch("second", &test.repo.find_commit(head).unwrap(), false)
        .unwrap();
    let mut opts = git2::WorktreeAddOptions::new();
    let reference = branch.into_reference();
    opts.reference(Some(&reference));
    test.repo.worktree("gtfold", &wt_dir, Some(&opts)).unwrap();

    test.write_file("a.txt", "a2 STAGED\n");
    test.stage_file("a.txt");
    test.write_file("b.txt", "b2 UNSTAGED\n");

    let mut git_repo = test.git_repo();
    let lifted = git_repo
        .lift_worktree_row(WorktreeSource::Staged)
        .unwrap()
        .expect("the staged row has changes");

    // The other working tree runs an operation, syncing its own pins.
    let mut linked = Git2Repo::open(wt_dir.clone()).unwrap();
    let linked_head = linked.head_oid().unwrap();
    assert_rebase_complete!(linked.drop_commit(&Oid::from(c1), &linked_head).unwrap());

    git_gc(test.repo.workdir().unwrap());

    let mut after = test.git_repo();
    after
        .restore_lifted_row(&lifted)
        .expect("another working tree's run must not cost this one its uncommitted work");
    assert_eq!(
        std::fs::read_to_string(test.repo.workdir().unwrap().join("b.txt")).unwrap(),
        "b2 UNSTAGED\n"
    );

    let _ = std::fs::remove_dir_all(&wt_dir);
}
