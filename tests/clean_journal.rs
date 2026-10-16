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

/// The pins and the journal are the only things naming the tree a lift recorded,
/// and `--clean-journal` removes both — while being what main.rs recommends
/// after an interrupted upgrade, which is raised for exactly a fold.
#[test]
fn clean_rescues_an_in_flight_folds_working_tree() {
    let test = common::TestRepo::new();
    test.commit_files(&[("a.txt", "a1\n"), ("b.txt", "b1\n")], "base");
    test.write_file("a.txt", "STAGED\n");
    test.stage_file("a.txt");
    test.write_file("b.txt", "UNSTAGED\n");

    let mut git_repo = test.git_repo();
    let lifted = common::lift_at_head(&mut git_repo, git_tailor::repo::WorktreeSource::Staged)
        .unwrap()
        .expect("the staged row has changes");

    let summary = git_repo.clean_journal().unwrap();

    let rescue = format!("refs/git-tailor/rescue/{}", lifted.worktree_tree);
    assert!(
        test.repo.find_reference(&rescue).is_ok(),
        "the fold's working tree must be kept, not left as garbage"
    );
    assert_eq!(
        summary.rescue_refs_kept, 1,
        "and be counted once, not once for rescuing and again for skipping it"
    );
}

/// What `--drop-rescued` is for: the rescued trees are listed with enough to
/// get them back, and only then removed.
#[test]
fn rescued_trees_are_listed_with_their_size_and_can_be_dropped() {
    let test = common::TestRepo::new();
    test.commit_files(&[("a.txt", "a1\n"), ("b.txt", "b1\n")], "base");
    test.write_file("a.txt", "STAGED\n");
    test.stage_file("a.txt");
    test.write_file("b.txt", "UNSTAGED\n");

    let mut git_repo = test.git_repo();
    common::lift_at_head(&mut git_repo, git_tailor::repo::WorktreeSource::Staged)
        .unwrap()
        .expect("the staged row has changes");
    git_repo.clean_journal().unwrap();

    let rescued = git_repo.rescued_trees().unwrap();
    assert_eq!(rescued.len(), 1, "the fold's tree was kept");
    assert_eq!(
        rescued[0].file_count,
        Some(2),
        "the listing must say how much is at stake"
    );
    assert!(
        rescued[0].tree.is_some(),
        "and resolve the tree the restore command needs"
    );

    let refnames: Vec<String> = rescued.iter().map(|t| t.refname.clone()).collect();
    assert_eq!(git_repo.drop_rescued_trees(&refnames).unwrap(), 1);
    assert_eq!(
        git_tailor_ref_count(&test),
        0,
        "and nothing of git-tailor's is left"
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

/// `--clean-journal` is what gt sends the user to when it refuses to start on an
/// unreadable journal, so it has to keep their work without being able to read
/// the record naming it. The pins are what is left: the tree ones name loose
/// objects nothing else reaches.
#[test]
fn clean_keeps_the_pinned_trees_of_a_journal_it_cannot_read() {
    let test = common::TestRepo::new();
    test.commit_files(&[("a.txt", "a1\n"), ("b.txt", "b1\n")], "base");
    test.write_file("a.txt", "STAGED\n");
    test.stage_file("a.txt");
    test.write_file("b.txt", "UNSTAGED\n");

    let mut git_repo = test.git_repo();
    let lifted = common::lift_at_head(&mut git_repo, git_tailor::repo::WorktreeSource::Staged)
        .unwrap()
        .expect("the staged row has changes");
    std::fs::write(journal_path(&test), "{ this is not valid json").unwrap();

    let summary = git_repo.clean_journal().unwrap();

    let rescue = format!("refs/git-tailor/rescue/{}", lifted.worktree_tree);
    assert!(
        test.repo.find_reference(&rescue).is_ok(),
        "the fold's working tree must be kept, not left as garbage"
    );
    assert_eq!(
        summary.rescue_refs_kept, 1,
        "and counted, since nothing else will tell the user"
    );

    let rescued = git_repo.rescued_trees().unwrap();
    assert_eq!(rescued.len(), 1);
    assert_eq!(
        rescued[0].file_count,
        Some(2),
        "`--list-rescued` must be able to offer the restore command"
    );
}

/// A commit pin names something the branch reflog still names, so keeping it
/// would present history as rescued work. The distinction is the ref's own
/// target: peeling a commit yields its tree, which would keep every one.
#[test]
fn clean_keeps_no_rescue_for_the_commit_pins_of_an_unreadable_journal() {
    let test = common::TestRepo::new();
    test.commit_file("a.txt", "a\n", "base");
    let c1 = test.commit_file("b.txt", "b\n", "add b");
    test.commit_file("c.txt", "c\n", "add c");
    let mut git_repo = test.git_repo();
    // A drop pins the tips it moved between, and nothing else.
    git_repo
        .drop_commit(&Oid::from(c1), &Oid::from(head_oid(&test)))
        .unwrap();
    std::fs::write(journal_path(&test), "{ this is not valid json").unwrap();

    git_repo.clean_journal().unwrap();

    assert_eq!(
        git_tailor_ref_count(&test),
        0,
        "nothing of git-tailor's may be left"
    );
}

/// The same rule `lift_op::rescue` applies on the readable path: a tree HEAD
/// already reaches is not uncommitted work, so there is nothing to keep.
#[test]
fn clean_keeps_no_rescue_for_a_pinned_tree_head_already_reaches() {
    let test = common::TestRepo::new();
    test.commit_files(&[("a.txt", "a1\n"), ("b.txt", "b1\n")], "base");
    test.write_file("a.txt", "STAGED\n");
    test.stage_file("a.txt");

    let mut git_repo = test.git_repo();
    // Nothing unstaged, so the lift's working tree is the temporary commit's own.
    common::lift_at_head(&mut git_repo, git_tailor::repo::WorktreeSource::Staged)
        .unwrap()
        .expect("the staged row has changes");
    std::fs::write(journal_path(&test), "{ this is not valid json").unwrap();

    git_repo.clean_journal().unwrap();

    assert_eq!(git_tailor_ref_count(&test), 0);
}

/// What the user actually sees, which is the only thing telling them their work
/// was kept — and that the stash still holds the row the fold set aside, since
/// nothing read the record that would have dropped it.
#[test]
fn clean_journal_reports_what_it_kept_from_an_unreadable_journal() {
    let test = common::TestRepo::new();
    test.commit_files(&[("a.txt", "a1\n"), ("b.txt", "b1\n")], "base");
    test.write_file("a.txt", "STAGED\n");
    test.stage_file("a.txt");
    test.write_file("b.txt", "UNSTAGED\n");

    let mut git_repo = test.git_repo();
    common::lift_at_head(&mut git_repo, git_tailor::repo::WorktreeSource::Staged)
        .unwrap()
        .expect("the staged row has changes");
    std::fs::write(journal_path(&test), "{ this is not valid json").unwrap();
    drop(git_repo);

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_gt"))
        .arg("--clean-journal")
        .current_dir(test.repo.workdir().unwrap())
        .output()
        .expect("failed to run gt");

    assert!(out.status.success(), "exit: {:?}", out.status);
    let stdout = String::from_utf8_lossy(&out.stdout);
    for expected in ["could not be used", "--list-rescued", "git stash list"] {
        assert!(stdout.contains(expected), "missing {expected:?}: {stdout}");
    }
}

/// gt tells the user to run this after an interrupted upgrade, promising their
/// uncommitted work is kept. An older journal shape does not parse through the
/// current schema either, so the report used to contradict the instruction that
/// sent them here: "whatever it recorded is gone from git-tailor's view".
#[test]
fn clean_journal_does_not_claim_an_older_journal_was_unreadable() {
    let test = common::TestRepo::new();
    test.commit_file("a.txt", "a\n", "base");
    let v1 = r#"{
        "version": 1,
        "in_progress": { "operation_label": "Squash", "original_branch_oid": "aaaa" },
        "undo": [], "redo": [], "autostash": null
    }"#;
    std::fs::create_dir_all(journal_path(&test).parent().unwrap()).unwrap();
    std::fs::write(journal_path(&test), v1).unwrap();

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_gt"))
        .arg("--clean-journal")
        .current_dir(test.repo.workdir().unwrap())
        .output()
        .expect("failed to run gt");

    assert!(out.status.success(), "exit: {:?}", out.status);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("could not"),
        "nothing was kept and nothing was lost, so there is nothing to report: {stdout}"
    );
}

/// The readable path destroys what the unreadable one refuses to: a rescue that
/// fails is swallowed, and the sweep then drops the pins that were the only
/// refs reaching the fold's working tree.
#[test]
fn clean_refuses_rather_than_sweeping_a_fold_it_could_not_rescue() {
    let test = common::TestRepo::new();
    test.commit_files(&[("a.txt", "a1\n"), ("b.txt", "b1\n")], "base");
    test.write_file("a.txt", "STAGED\n");
    test.stage_file("a.txt");
    test.write_file("b.txt", "UNSTAGED\n");

    let mut git_repo = test.git_repo();
    let lifted = common::lift_at_head(&mut git_repo, git_tailor::repo::WorktreeSource::Staged)
        .unwrap()
        .expect("the staged row has changes");
    // A ref where the rescue namespace needs a directory, so no rescue ref can
    // be written under it.
    test.repo
        .reference(
            "refs/git-tailor/rescue",
            head_oid(&test),
            true,
            "directory/file conflict",
        )
        .unwrap();

    assert!(git_repo.clean_journal().is_err());

    let pins: Vec<String> = test
        .repo
        .references_glob("refs/git-tailor/wt/**")
        .unwrap()
        .names()
        .map(|name| name.unwrap().to_string())
        .collect();
    assert!(
        pins.iter()
            .any(|name| name.starts_with("refs/git-tailor/wt/")),
        "the pins must still be there: {pins:?}"
    );
    assert!(
        test.repo
            .find_object(git2::Oid::from(&lifted.worktree_tree), None)
            .is_ok(),
        "and the fold's working tree still reachable through them"
    );
}
