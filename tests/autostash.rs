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

//! Integration tests for the opt-in auto-stash. These exercise the
//! `autostash_save` / `autostash_restore` primitives around real operations,
//! the way `main.rs` orchestrates them.

#[allow(dead_code)]
mod common;

use common::prelude::*;
use git_tailor::repo::{AutostashContinue, AutostashRestore, UndoOutcome};
use std::path::Path;

/// Open a fresh handle and return `(staged, unstaged)` file paths, so the read
/// isn't affected by the operating handle's cached index.
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

fn stash_count(gitdir: &Path) -> usize {
    let mut repo = git2::Repository::open(gitdir).unwrap();
    let mut n = 0;
    repo.stash_foreach(|_, _, _| {
        n += 1;
        true
    })
    .unwrap();
    n
}

/// Three independent-file commits on `work`, plus a staged change to `s.txt`
/// and an unstaged change to `u.txt`. Returns `(base, c1, c2)`.
fn setup_dirty_repo(test: &common::TestRepo) -> (git2::Oid, git2::Oid, git2::Oid) {
    let base = test.commit_files(
        &[("a.txt", "a\n"), ("s.txt", "s0\n"), ("u.txt", "u0\n")],
        "base",
    );
    let c1 = test.commit_file("b.txt", "b\n", "add b");
    let c2 = test.commit_file("c.txt", "c\n", "add c");

    // Staged change to s.txt, unstaged change to u.txt.
    test.write_file("s.txt", "s1\n");
    test.stage_file("s.txt");
    test.write_file("u.txt", "u1\n");

    (base, c1, c2)
}

#[test]
fn autostash_restores_mixed_state_after_drop() {
    let test = common::TestRepo::new();
    let (base, c1, c2) = setup_dirty_repo(&test);
    let gitdir = test.repo.path().to_path_buf();

    let (staged_before, unstaged_before) = staged_and_unstaged(&gitdir);
    assert_eq!(staged_before, vec!["s.txt"]);
    assert_eq!(unstaged_before, vec!["u.txt"]);

    let mut git_repo = test.git_repo();
    git_repo.set_autostash(true);

    git_repo.autostash_save().unwrap();
    // The working tree is clean while the operation runs.
    let (s, u) = staged_and_unstaged(&gitdir);
    assert!(
        s.is_empty() && u.is_empty(),
        "tree should be clean after stash"
    );

    // Drop an independent commit (no conflict).
    assert_rebase_complete!(
        git_repo
            .drop_commit(&Oid::from(c1), &Oid::from(c2))
            .unwrap()
    );
    git_repo.autostash_restore().unwrap();

    // The exact staged/unstaged split is back.
    let (staged_after, unstaged_after) = staged_and_unstaged(&gitdir);
    assert_eq!(staged_after, staged_before);
    assert_eq!(unstaged_after, unstaged_before);
    assert_eq!(read_workdir(&test, "s.txt"), "s1\n");
    assert_eq!(read_workdir(&test, "u.txt"), "u1\n");
    // The drop actually happened.
    assert_eq!(test.commits_from_head(base).len(), 1);
}

#[test]
fn autostash_preserves_untracked_files() {
    let test = common::TestRepo::new();
    let (_base, c1, c2) = setup_dirty_repo(&test);
    test.write_file("untracked.txt", "keep me\n");
    let gitdir = test.repo.path().to_path_buf();

    let mut git_repo = test.git_repo();
    git_repo.set_autostash(true);

    git_repo.autostash_save().unwrap();
    assert!(
        !test.repo.workdir().unwrap().join("untracked.txt").exists(),
        "untracked file should be stashed away"
    );

    assert_rebase_complete!(
        git_repo
            .drop_commit(&Oid::from(c1), &Oid::from(c2))
            .unwrap()
    );
    git_repo.autostash_restore().unwrap();

    assert_eq!(read_workdir(&test, "untracked.txt"), "keep me\n");
    let (staged, unstaged) = staged_and_unstaged(&gitdir);
    assert_eq!(staged, vec!["s.txt"]);
    assert!(unstaged.contains(&"u.txt".to_string()));
    assert!(unstaged.contains(&"untracked.txt".to_string()));
}

#[test]
fn autostash_restored_after_conflict_resolved_and_continued() {
    let test = common::TestRepo::new();
    // c2 depends on c1's line, so dropping c1 conflicts.
    let base = test.commit_files(&[("a.txt", "0\n"), ("s.txt", "s0\n")], "base");
    let c1 = test.commit_file("a.txt", "0\n1\n", "add 1");
    let c2 = test.commit_file("a.txt", "0\n1\n2\n", "add 2");
    test.write_file("s.txt", "s1\n");
    test.stage_file("s.txt");
    let gitdir = test.repo.path().to_path_buf();

    let mut git_repo = test.git_repo();
    git_repo.set_autostash(true);
    git_repo.autostash_save().unwrap();

    let state = expect_rebase_conflict!(
        git_repo
            .drop_commit(&Oid::from(c1), &Oid::from(c2))
            .unwrap()
    );

    // Resolve the conflict in a.txt, then continue. Re-read the index from disk
    // first: auto-stash reset the on-disk index via a different handle, so this
    // handle's cached index is stale.
    test.write_file("a.txt", "0\n2\n");
    let mut index = test.repo.index().unwrap();
    index.read(true).unwrap();
    index.conflict_remove(Path::new("a.txt")).unwrap();
    index.add_path(Path::new("a.txt")).unwrap();
    index.write().unwrap();

    assert_rebase_complete!(git_repo.rebase_continue(&state).unwrap());
    git_repo.autostash_restore().unwrap();

    // Staged s.txt is restored, and the conflict resolution stuck.
    let (staged, _unstaged) = staged_and_unstaged(&gitdir);
    assert!(staged.contains(&"s.txt".to_string()));
    assert_eq!(read_workdir(&test, "s.txt"), "s1\n");
    assert_eq!(test.commits_from_head(base).len(), 1);
}

#[test]
fn autostash_restored_after_conflict_aborted() {
    let test = common::TestRepo::new();
    let base = test.commit_files(&[("a.txt", "0\n"), ("s.txt", "s0\n")], "base");
    let c1 = test.commit_file("a.txt", "0\n1\n", "add 1");
    let c2 = test.commit_file("a.txt", "0\n1\n2\n", "add 2");
    test.write_file("s.txt", "s1\n");
    test.stage_file("s.txt");
    let gitdir = test.repo.path().to_path_buf();

    let mut git_repo = test.git_repo();
    git_repo.set_autostash(true);
    git_repo.autostash_save().unwrap();

    let state = expect_rebase_conflict!(
        git_repo
            .drop_commit(&Oid::from(c1), &Oid::from(c2))
            .unwrap()
    );
    git_repo.rebase_abort(&state).unwrap();
    git_repo.autostash_restore().unwrap();

    // Branch restored to the original tip, and the staged change is back.
    assert_eq!(
        Oid::from(test.repo.head().unwrap().target().unwrap()),
        Oid::from(c2)
    );
    let (staged, _unstaged) = staged_and_unstaged(&gitdir);
    assert!(staged.contains(&"s.txt".to_string()));
    assert_eq!(read_workdir(&test, "s.txt"), "s1\n");
    let _ = base;
}

#[test]
fn autostash_disabled_still_refuses_dirty_operation() {
    let test = common::TestRepo::new();
    let (_base, c1, c2) = setup_dirty_repo(&test);

    let git_repo = test.git_repo(); // auto-stash NOT enabled
    let mut git_repo = git_repo;
    git_repo.autostash_save().unwrap(); // no-op when disabled

    let err = git_repo
        .drop_commit(&Oid::from(c1), &Oid::from(c2))
        .unwrap_err();
    assert!(
        err.to_string().contains("staged or unstaged"),
        "should still refuse: {err}"
    );
}

#[test]
fn autostash_around_undo_with_dirty_tree() {
    let test = common::TestRepo::new();
    let base = test.commit_files(&[("a.txt", "a\n"), ("s.txt", "s0\n")], "base");
    let c1 = test.commit_file("b.txt", "b\n", "add b");
    test.commit_file("c.txt", "c\n", "add c");
    let gitdir = test.repo.path().to_path_buf();

    let mut git_repo = test.git_repo();
    git_repo.set_autostash(true);

    // Clean drop, which records an undo entry.
    let head = test.repo.head().unwrap().target().unwrap();
    assert_rebase_complete!(
        git_repo
            .drop_commit(&Oid::from(c1), &Oid::from(head))
            .unwrap()
    );
    assert_eq!(test.commits_from_head(base).len(), 1);

    // Now dirty the tree, then undo with auto-stash around it.
    test.write_file("s.txt", "s1\n");
    test.stage_file("s.txt");

    git_repo.autostash_save().unwrap();
    assert!(matches!(git_repo.undo().unwrap(), UndoOutcome::Done { .. }));
    git_repo.autostash_restore().unwrap();

    // Undo restored the dropped commit, and the staged change survived.
    assert_eq!(test.commits_from_head(base).len(), 2);
    let (staged, _unstaged) = staged_and_unstaged(&gitdir);
    assert!(staged.contains(&"s.txt".to_string()));
    assert_eq!(read_workdir(&test, "s.txt"), "s1\n");
}

/// Build a repo where reapplying the auto-stash after dropping `c1` conflicts:
/// `c1` changes line 2 and the user edits the same line differently (unstaged).
/// The edit changes the file size, sidestepping git's racy "stat-clean"
/// shortcut so the change is reliably captured. Returns `(base, c1)`.
fn setup_restore_conflict(test: &common::TestRepo) -> (git2::Oid, git2::Oid) {
    let base = test.commit_files(&[("f.txt", "AAAA\nBBBB\nCCCC\n")], "base");
    let c1 = test.commit_file("f.txt", "AAAA\nYYYY\nCCCC\n", "c1");
    test.write_file("f.txt", "AAAA\nZZZZZZZZ\nCCCC\n");
    (base, c1)
}

#[test]
fn autostash_restore_conflict_reports_and_keeps_stash() {
    let test = common::TestRepo::new();
    let (_base, c1) = setup_restore_conflict(&test);
    let gitdir = test.repo.path().to_path_buf();

    let mut git_repo = test.git_repo();
    git_repo.set_autostash(true);
    git_repo.autostash_save().unwrap();
    assert_rebase_complete!(
        git_repo
            .drop_commit(&Oid::from(c1), &Oid::from(c1))
            .unwrap()
    );

    // libgit2's stash_apply does not report a content conflict as an error, so
    // the restore detects it via the index and reports a Conflict — keeping the
    // stash rather than silently dropping it and reporting success.
    let files = match git_repo.autostash_restore().unwrap() {
        AutostashRestore::Conflict { files } => files,
        AutostashRestore::Done => panic!("expected a conflict"),
    };
    assert_eq!(files, vec!["f.txt".to_string()]);
    assert!(
        read_workdir(&test, "f.txt").contains("<<<<<<<"),
        "conflict markers should be left in the working tree"
    );
    assert_eq!(stash_count(&gitdir), 1, "the stash must be kept");

    // The record is flagged applied, so a second restore re-reports the conflict
    // without reapplying onto the already-markered tree.
    assert!(matches!(
        git_repo.autostash_restore().unwrap(),
        AutostashRestore::Conflict { .. }
    ));
    assert_eq!(stash_count(&gitdir), 1);
}

#[test]
fn autostash_conflict_continue_resolves_and_drops_stash() {
    let test = common::TestRepo::new();
    let (base, c1) = setup_restore_conflict(&test);
    let gitdir = test.repo.path().to_path_buf();

    let mut git_repo = test.git_repo();
    git_repo.set_autostash(true);
    git_repo.autostash_save().unwrap();
    assert_rebase_complete!(
        git_repo
            .drop_commit(&Oid::from(c1), &Oid::from(c1))
            .unwrap()
    );
    assert!(matches!(
        git_repo.autostash_restore().unwrap(),
        AutostashRestore::Conflict { .. }
    ));

    // The user resolves the markers in the working tree, then continues.
    test.write_file("f.txt", "AAAA\nRESOLVED\nCCCC\n");
    assert!(matches!(
        git_repo.autostash_conflict_continue().unwrap(),
        AutostashContinue::Resolved
    ));

    // The drop stuck (HEAD is base), the resolution is in the tree, and the
    // stash is gone.
    assert_eq!(test.commits_from_head(base).len(), 0);
    assert_eq!(read_workdir(&test, "f.txt"), "AAAA\nRESOLVED\nCCCC\n");
    assert_eq!(stash_count(&gitdir), 0);
}

#[test]
fn autostash_conflict_continue_stays_when_unresolved() {
    let test = common::TestRepo::new();
    let (_base, c1) = setup_restore_conflict(&test);

    let mut git_repo = test.git_repo();
    git_repo.set_autostash(true);
    git_repo.autostash_save().unwrap();
    assert_rebase_complete!(
        git_repo
            .drop_commit(&Oid::from(c1), &Oid::from(c1))
            .unwrap()
    );
    assert!(matches!(
        git_repo.autostash_restore().unwrap(),
        AutostashRestore::Conflict { .. }
    ));

    // Markers still present: continue must report the files as unresolved.
    match git_repo.autostash_conflict_continue().unwrap() {
        AutostashContinue::StillUnresolved { files } => {
            assert_eq!(files, vec!["f.txt".to_string()])
        }
        AutostashContinue::Resolved => panic!("should still be unresolved"),
    }
}

#[test]
fn split_refuses_overlapping_dirty_without_autostash() {
    // Documents the guard that auto-stash sidesteps: split on its own refuses
    // when a dirty change overlaps a file the commit touches.
    let test = common::TestRepo::new();
    let _base = test.commit_files(&[("a.txt", "a0\n"), ("b.txt", "b0\n")], "base");
    let to_split = test.commit_files(&[("a.txt", "a1\n"), ("b.txt", "b1\n")], "big change");
    test.write_file("a.txt", "a1-dirty\n");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();
    let err = git_repo
        .split_commit_per_file(&Oid::from(to_split), &head_oid)
        .unwrap_err();
    assert!(
        err.to_string().contains("overlap"),
        "expected an overlap refusal, got: {err}"
    );
}

#[test]
fn autostash_lets_split_proceed_with_overlapping_unstaged() {
    // A split reproduces the same final tree, so stashing the overlapping edit,
    // splitting, then reapplying is always conflict-free — exactly the case the
    // bare overlap guard rejects.
    let test = common::TestRepo::new();
    let base = test.commit_files(&[("a.txt", "a0\n"), ("b.txt", "b0\n")], "base");
    let to_split = test.commit_files(&[("a.txt", "a1\n"), ("b.txt", "b1\n")], "big change");

    // Unstaged edit to a.txt, which the split touches (overlap).
    test.write_file("a.txt", "a1-dirty\n");
    let gitdir = test.repo.path().to_path_buf();

    let mut git_repo = test.git_repo();
    git_repo.set_autostash(true);
    let head_oid = git_repo.head_oid().unwrap();

    git_repo.autostash_save().unwrap();
    git_repo
        .split_commit_per_file(&Oid::from(to_split), &head_oid)
        .unwrap();
    git_repo.autostash_restore().unwrap();

    // The split happened (base → two commits) and the overlapping edit is back.
    assert_eq!(test.commits_from_head(base).len(), 2);
    assert_eq!(read_workdir(&test, "a.txt"), "a1-dirty\n");
    let (_staged, unstaged) = staged_and_unstaged(&gitdir);
    assert_eq!(unstaged, vec!["a.txt"]);
}

#[test]
fn autostash_lets_split_proceed_with_overlapping_staged() {
    let test = common::TestRepo::new();
    let base = test.commit_files(&[("a.txt", "a0\n"), ("b.txt", "b0\n")], "base");
    let to_split = test.commit_files(&[("a.txt", "a1\n"), ("b.txt", "b1\n")], "big change");

    // Staged edit to a.txt, overlapping the split.
    test.write_file("a.txt", "a1-staged\n");
    test.stage_file("a.txt");
    let gitdir = test.repo.path().to_path_buf();

    let mut git_repo = test.git_repo();
    git_repo.set_autostash(true);
    let head_oid = git_repo.head_oid().unwrap();

    git_repo.autostash_save().unwrap();
    git_repo
        .split_commit_per_file(&Oid::from(to_split), &head_oid)
        .unwrap();
    git_repo.autostash_restore().unwrap();

    assert_eq!(test.commits_from_head(base).len(), 2);
    assert_eq!(read_workdir(&test, "a.txt"), "a1-staged\n");
    let (staged, _unstaged) = staged_and_unstaged(&gitdir);
    assert_eq!(staged, vec!["a.txt"]);
}

#[test]
fn autostash_conflict_abort_rewinds_whole_operation() {
    let test = common::TestRepo::new();
    let (base, c1) = setup_restore_conflict(&test);
    let gitdir = test.repo.path().to_path_buf();

    let mut git_repo = test.git_repo();
    git_repo.set_autostash(true);
    git_repo.autostash_save().unwrap();
    // A clean drop of an unrelated earlier commit would record undo; here we drop
    // c1, recording an undo entry that the abort must also remove.
    assert_rebase_complete!(
        git_repo
            .drop_commit(&Oid::from(c1), &Oid::from(c1))
            .unwrap()
    );
    assert!(matches!(
        git_repo.autostash_restore().unwrap(),
        AutostashRestore::Conflict { .. }
    ));

    git_repo.autostash_conflict_abort().unwrap();

    // Back to exactly before the drop: c1 is restored as HEAD, the user's
    // original unstaged edit is back (no markers), and the stash is gone.
    assert_eq!(
        Oid::from(test.repo.head().unwrap().target().unwrap()),
        Oid::from(c1)
    );
    assert_eq!(test.commits_from_head(base).len(), 1);
    assert_eq!(read_workdir(&test, "f.txt"), "AAAA\nZZZZZZZZ\nCCCC\n");
    assert_eq!(stash_count(&gitdir), 0);
    // The reverted drop no longer lingers in the undo history.
    assert!(matches!(
        git_repo.undo().unwrap(),
        UndoOutcome::Empty | UndoOutcome::Stale
    ));
}

/// Regression: a *same-size* unstaged edit must survive an autostash round-trip.
///
/// `setup_dirty_repo` edits `u.txt` from "u0\n" to "u1\n" — both 3 bytes. When
/// such an edit's mtime collides with the index entry's cached stat, libgit2's
/// stash dirty-detection can treat the file as unmodified and serialize the
/// stale blob, silently dropping the edit (see `save_autostash`, which now
/// forces a content-level index refresh before stashing). Unlike the sibling
/// tests this performs no intermediate status reads — those perturb the timing
/// and hide the bug — so it triggers the collision reliably.
#[test]
fn autostash_preserves_same_size_unstaged_edit() {
    for _ in 0..10 {
        let test = common::TestRepo::new();
        let (_base, c1, c2) = setup_dirty_repo(&test);

        let mut git_repo = test.git_repo();
        git_repo.set_autostash(true);
        git_repo.autostash_save().unwrap();
        assert_rebase_complete!(
            git_repo
                .drop_commit(&Oid::from(c1), &Oid::from(c2))
                .unwrap()
        );
        git_repo.autostash_restore().unwrap();

        assert_eq!(
            read_workdir(&test, "u.txt"),
            "u1\n",
            "same-size unstaged edit was silently dropped by autostash"
        );
    }
}
