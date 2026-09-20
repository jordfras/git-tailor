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

//! Integration tests for `--clean-journal` (`GitRepo::clean_journal`): wiping
//! the journal file and every `refs/git-tailor/*` ref, including stray refs.

#[allow(dead_code)]
mod common;

use common::prelude::*;

fn head_oid(test: &common::TestRepo) -> git2::Oid {
    test.repo.head().unwrap().target().unwrap()
}

fn git_tailor_ref_count(test: &common::TestRepo) -> usize {
    test.repo
        .references_glob("refs/git-tailor/*")
        .unwrap()
        .count()
}

fn journal_path(test: &common::TestRepo) -> std::path::PathBuf {
    test.repo.path().join("git-tailor").join("journal.json")
}

#[test]
fn clean_removes_journal_file_and_all_refs_including_stray() {
    let test = common::TestRepo::new();
    let base = test.commit_file("a.txt", "a\n", "base");
    let c1 = test.commit_file("b.txt", "b\n", "add b");
    test.commit_file("c.txt", "c\n", "add c");
    let mut git_repo = test.git_repo();

    // A real op seeds journal.json + undo pins.
    git_repo
        .drop_commit(&Oid::from(c1), &Oid::from(head_oid(&test)))
        .unwrap();
    // A stray ref not referenced by the journal.
    test.repo
        .reference("refs/git-tailor/undo/999", base, true, "stray")
        .unwrap();

    assert!(journal_path(&test).exists(), "journal file should exist");
    let before = git_tailor_ref_count(&test);
    assert!(before >= 2, "expected undo pins plus the stray ref");

    let summary = git_repo.clean_journal().unwrap();

    assert!(summary.journal_removed);
    assert_eq!(summary.refs_removed, before);
    assert!(!journal_path(&test).exists(), "journal file should be gone");
    assert_eq!(
        git_tailor_ref_count(&test),
        0,
        "every refs/git-tailor/* ref should be removed"
    );
}

/// Rescue refs are somebody's uncommitted work, kept when git-tailor had to
/// discard the record that named it. `--clean-journal` clears this working
/// tree's recovery state; these are repository-wide and are the only copy, so
/// removing them has to be asked for separately.
#[test]
fn clean_keeps_rescued_working_trees_and_says_how_many() {
    let test = common::TestRepo::new();
    let base = test.commit_file("a.txt", "a\n", "base");
    let c1 = test.commit_file("b.txt", "b\n", "add b");
    test.commit_file("c.txt", "c\n", "add c");
    let mut git_repo = test.git_repo();

    git_repo
        .drop_commit(&Oid::from(c1), &Oid::from(head_oid(&test)))
        .unwrap();

    let tree = test.repo.find_commit(base).unwrap().tree().unwrap().id();
    let rescue = format!("refs/git-tailor/rescue/{tree}");
    test.repo
        .reference(&rescue, base, true, "rescued working tree")
        .unwrap();

    let summary = git_repo.clean_journal().unwrap();

    assert_eq!(
        summary.rescue_refs_kept, 1,
        "the user has to be told what was left behind"
    );
    assert!(
        test.repo.find_reference(&rescue).is_ok(),
        "the rescued working tree must survive"
    );
    assert_eq!(
        git_tailor_ref_count(&test),
        1,
        "and nothing else may survive"
    );
}

#[test]
fn clean_on_a_pristine_repo_is_a_noop() {
    let test = common::TestRepo::new();
    test.commit_file("a.txt", "a\n", "base");
    let mut git_repo = test.git_repo();

    let summary = git_repo.clean_journal().unwrap();

    assert!(!summary.journal_removed);
    assert_eq!(summary.refs_removed, 0);
}

#[test]
fn clean_removes_refs_even_with_no_journal_file() {
    // The core promise: refs are found by namespace, not from the journal, so
    // they are cleared even when no journal file exists (deleted/corrupt/never
    // written).
    let test = common::TestRepo::new();
    let base = test.commit_file("a.txt", "a\n", "base");
    let mut git_repo = test.git_repo();
    test.repo
        .reference("refs/git-tailor/undo/0", base, true, "stray")
        .unwrap();
    test.repo
        .reference("refs/git-tailor/orig", base, true, "stray")
        .unwrap();
    assert!(!journal_path(&test).exists());

    let summary = git_repo.clean_journal().unwrap();

    assert!(!summary.journal_removed, "no journal file was present");
    assert_eq!(summary.refs_removed, 2);
    assert_eq!(git_tailor_ref_count(&test), 0);
}

/// `--clean-journal` in one working tree must not touch another's pins: a
/// paused conflict there is reachable only through its own `wt/<id>/orig`
/// pin, and refs are shared across every working tree of one repository.
#[test]
fn clean_journal_in_one_worktree_leaves_another_worktrees_pin_alone() {
    let test = common::TestRepo::new();
    let base = test.commit_file("a.txt", "base\n", "base");
    let to_drop = test.commit_file("a.txt", "base\ndropped\n", "add dropped");
    let head = test.commit_file("a.txt", "base\ndropped\nhead\n", "add head");
    let _ = base;

    let wt_dir = std::env::temp_dir().join(format!("gt-wt-clean-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&wt_dir);
    let branch = test
        .repo
        .branch("second", &test.repo.find_commit(head).unwrap(), false)
        .unwrap();
    let mut opts = git2::WorktreeAddOptions::new();
    let reference = branch.into_reference();
    opts.reference(Some(&reference));
    test.repo.worktree("gtwt", &wt_dir, Some(&opts)).unwrap();

    // The linked working tree pauses on a conflict, pinning its original tip
    // under its own `wt/<id>/orig`.
    let mut linked = Git2Repo::open(wt_dir.clone()).unwrap();
    let linked_head = linked.head_oid().unwrap();
    expect_rebase_conflict!(
        linked
            .drop_commit(&Oid::from(to_drop), &linked_head)
            .unwrap()
    );
    let wt_refs_before: Vec<String> = test
        .repo
        .references_glob("refs/git-tailor/wt/*")
        .unwrap()
        .filter_map(|r| r.ok().and_then(|r| r.name().ok().map(String::from)))
        .collect();
    assert!(
        !wt_refs_before.is_empty(),
        "the linked working tree's conflict should have pinned its original tip"
    );

    // Run --clean-journal in the *main* working tree, which has no journal of
    // its own.
    let mut main_repo = test.git_repo();
    main_repo.clean_journal().unwrap();

    let wt_refs_after: Vec<String> = test
        .repo
        .references_glob("refs/git-tailor/wt/*")
        .unwrap()
        .filter_map(|r| r.ok().and_then(|r| r.name().ok().map(String::from)))
        .collect();
    assert_eq!(
        wt_refs_before, wt_refs_after,
        "cleaning the main working tree's journal must not touch the linked \
         working tree's pin"
    );

    // The linked working tree can still recover its paused conflict.
    let mut linked = Git2Repo::open(wt_dir.clone()).unwrap();
    assert!(
        matches!(
            linked.read_journal().unwrap(),
            git_tailor::repo::JournalStatus::Recovered(_)
        ),
        "the linked working tree's paused conflict must still be recoverable"
    );

    let _ = std::fs::remove_dir_all(&wt_dir);
}
