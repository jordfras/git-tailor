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

/// A branch whose target commit introduces both `a.txt` and `b.txt`, followed by
/// an unrelated commit, with the requested working-tree changes on top: `staged`
/// edits `a.txt` and stages it, `unstaged` edits `b.txt` and leaves it. Either
/// row can be folded into the target.
fn repo_with(staged: bool, unstaged: bool) -> (common::TestRepo, git2::Oid, git2::Oid) {
    let test = common::TestRepo::new();
    let base = test.commit_file("base.txt", "base\n", "base");
    let target = test.commit_files(&[("a.txt", "a1\n"), ("b.txt", "b1\n")], "target commit");
    test.commit_file("c.txt", "c1\n", "later commit");
    if staged {
        test.write_file("a.txt", "a2\n");
        test.stage_file("a.txt");
    }
    if unstaged {
        test.write_file("b.txt", "b2\n");
    }
    (test, base, target)
}

/// Both rows present, either one foldable.
fn mixed_state() -> (common::TestRepo, git2::Oid, git2::Oid) {
    repo_with(true, true)
}

/// Fold the whole row into `target`, as the fixup key does — keeping the
/// target's own message.
fn fixup_row(
    git_repo: &Git2Repo,
    source: WorktreeSource,
    target: git2::Oid,
) -> git_tailor::repo::RebaseOutcome {
    let message = git_repo
        .commit_diff(&Oid::from(target), 0)
        .unwrap()
        .commit
        .message;
    let started = git_repo
        .lift_worktree_row(source)
        .unwrap()
        .expect("the row has changes to fold in");
    git_repo
        .squash_commits(
            &started.temp_oid,
            &Oid::from(target),
            &message,
            &started.temp_oid,
        )
        .unwrap()
}

/// Every combination of which rows have changes and which one is folded.
///
/// The invariant across all of them: the row being folded ends up in the target
/// commit, the other row's changes stay exactly where they were, the files on
/// disk never change, and one undo puts it all back.
#[test]
fn each_combination_of_rows_folds_and_undoes() {
    // (row to fold, staged changes present, unstaged changes present)
    let cases = [
        (WorktreeSource::Staged, true, false),
        (WorktreeSource::Staged, true, true),
        (WorktreeSource::Unstaged, false, true),
        (WorktreeSource::Unstaged, true, true),
    ];
    for (source, staged, unstaged) in cases {
        let case = format!("{source:?} row, staged={staged} unstaged={unstaged}");
        let (test, base, target) = repo_with(staged, unstaged);
        let git_repo = test.git_repo();
        let head_before = git_repo.head_oid().unwrap();

        // What is on disk, which no part of this may change.
        let on_disk = (workdir(&test, "a.txt"), workdir(&test, "b.txt"));

        assert_rebase_complete!(fixup_row(&git_repo, source, target));

        assert_history!(&test, base, &["target commit", "later commit"]);
        let (folded, folded_content, kept) = match source {
            WorktreeSource::Staged => (
                "a.txt",
                "a2\n",
                if unstaged { vec!["b.txt"] } else { vec![] },
            ),
            WorktreeSource::Unstaged => {
                ("b.txt", "b2\n", if staged { vec!["a.txt"] } else { vec![] })
            }
        };
        assert_file_contents_at_head!(&test.repo, folded, folded_content);
        let (still_staged, still_unstaged) = match source {
            WorktreeSource::Staged => (vec![], kept),
            WorktreeSource::Unstaged => (kept, vec![]),
        };
        assert_eq!(
            row_paths(git_repo.staged_diff(3).unwrap()),
            still_staged,
            "{case}"
        );
        assert_eq!(
            row_paths(git_repo.unstaged_diff(3).unwrap()),
            still_unstaged,
            "{case}"
        );
        assert_eq!(
            (workdir(&test, "a.txt"), workdir(&test, "b.txt")),
            on_disk,
            "the fold must not touch the files on disk: {case}"
        );

        let head_after = git_repo.head_oid().unwrap();
        assert!(
            matches!(git_repo.undo().unwrap(), UndoOutcome::Done { .. }),
            "{case}"
        );
        assert_eq!(git_repo.head_oid().unwrap(), head_before, "{case}");
        assert_eq!(
            row_paths(git_repo.staged_diff(3).unwrap()),
            if staged { vec!["a.txt"] } else { vec![] },
            "undo must restore what was staged: {case}"
        );
        assert_eq!(
            row_paths(git_repo.unstaged_diff(3).unwrap()),
            if unstaged { vec!["b.txt"] } else { vec![] },
            "undo must restore what was unstaged: {case}"
        );
        assert_eq!(
            (workdir(&test, "a.txt"), workdir(&test, "b.txt")),
            on_disk,
            "undo must not touch the files on disk: {case}"
        );

        assert!(
            matches!(git_repo.redo().unwrap(), UndoOutcome::Done { .. }),
            "{case}"
        );
        assert_eq!(git_repo.head_oid().unwrap(), head_after, "{case}");
        assert_eq!(
            row_paths(git_repo.staged_diff(3).unwrap()),
            still_staged,
            "{case}"
        );
        assert_eq!(
            row_paths(git_repo.unstaged_diff(3).unwrap()),
            still_unstaged,
            "{case}"
        );
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
        .lift_worktree_row(WorktreeSource::Unstaged)
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

/// Resolving a conflicted fold finishes the operation: the resolution is
/// committed and the other row's changes come back on their own side.
#[test]
fn resolving_a_conflicted_fold_completes_it() {
    let test = common::TestRepo::new();
    let base = test.commit_file("a.txt", "one\ntwo\n", "base");
    let target = test.commit_file("a.txt", "one\nTARGET\n", "target commit");
    test.commit_file("a.txt", "one\nLATER\n", "later commit");
    test.write_file("a.txt", "one\nWIP\n");
    test.write_file("b.txt", "b1\n");
    test.stage_file("b.txt");
    let git_repo = test.git_repo();

    let started = git_repo
        .lift_worktree_row(WorktreeSource::Unstaged)
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

    test.write_file("a.txt", "one\nRESOLVED\n");
    git_repo
        .auto_stage_resolved_conflicts(&state.conflicting_files)
        .unwrap();
    // The row's own changes clashed with the target, so this resumes the way
    // the conflict dialog does for a squash-tree conflict.
    let Resume::Squash(ctx) = &state.resume else {
        panic!("expected a squash-tree conflict, got {:?}", state.resume);
    };
    let next = git_repo
        .squash_finalize(ctx, "target commit", &state.original_branch_oid, None)
        .unwrap();

    // Replaying the commits after the target hits the same lines, so the fold
    // conflicts a second time and finishes through the ordinary continue path.
    let state = expect_rebase_conflict!(next);
    assert!(matches!(state.resume, Resume::Chain { .. }));
    test.write_file("a.txt", "one\nLATER\n");
    git_repo
        .auto_stage_resolved_conflicts(&state.conflicting_files)
        .unwrap();
    assert_rebase_complete!(git_repo.rebase_continue(&state).unwrap());

    assert_history!(&test, base, &["target commit", "later commit"]);
    // The resolution stands in the working tree — the row's changes were folded
    // in, not reverted back on top of it.
    assert_eq!(workdir(&test, "a.txt"), "one\nLATER\n");
    assert!(git_repo.unstaged_diff(3).unwrap().is_none());
    assert_eq!(row_paths(git_repo.staged_diff(3).unwrap()), ["b.txt"]);
    assert_eq!(workdir(&test, "b.txt"), "b1\n");

    let reopened = test.git_repo();
    assert!(matches!(
        reopened.read_journal().unwrap(),
        git_tailor::repo::JournalStatus::None
    ));

    // Still one undoable operation, not two.
    assert!(matches!(git_repo.undo().unwrap(), UndoOutcome::Done { .. }));
    assert!(matches!(git_repo.undo().unwrap(), UndoOutcome::Empty));
}

/// Both rows touching the *same* file is the case that needs the unstaged delta
/// lifted off the staged one. Non-overlapping edits separate cleanly, and each
/// side keeps its own.
#[test]
fn one_file_edited_on_both_sides_separates_cleanly() {
    let test = common::TestRepo::new();
    let base = test.commit_file("f.txt", "1\n2\n3\n4\n5\n6\n7\n8\n9\n", "base");
    let target = test.commit_file("g.txt", "g\n", "target commit");
    test.write_file("f.txt", "STAGED\n2\n3\n4\n5\n6\n7\n8\n9\n");
    test.stage_file("f.txt");
    test.write_file("f.txt", "STAGED\n2\n3\n4\n5\n6\n7\n8\nUNSTAGED\n");
    let git_repo = test.git_repo();

    assert_rebase_complete!(fixup_row(&git_repo, WorktreeSource::Unstaged, target));

    assert_history!(&test, base, &["target commit"]);
    // Only the unstaged line went in; the staged one is still waiting.
    assert_file_contents_at_head!(&test.repo, "f.txt", "1\n2\n3\n4\n5\n6\n7\n8\nUNSTAGED\n");
    assert_eq!(row_paths(git_repo.staged_diff(3).unwrap()), ["f.txt"]);
    assert!(git_repo.unstaged_diff(3).unwrap().is_none());
    assert_eq!(
        workdir(&test, "f.txt"),
        "STAGED\n2\n3\n4\n5\n6\n7\n8\nUNSTAGED\n"
    );

    assert!(matches!(git_repo.undo().unwrap(), UndoOutcome::Done { .. }));
    assert_eq!(row_paths(git_repo.staged_diff(3).unwrap()), ["f.txt"]);
    assert_eq!(row_paths(git_repo.unstaged_diff(3).unwrap()), ["f.txt"]);
}

/// Folding into HEAD is the amend case, and needs no commits replayed after it.
#[test]
fn folding_into_head_amends_it() {
    let test = common::TestRepo::new();
    let base = test.commit_file("base.txt", "base\n", "base");
    let head_commit = test.commit_file("a.txt", "a1\n", "head commit");
    test.write_file("a.txt", "a2\n");
    test.stage_file("a.txt");
    test.write_file("b.txt", "b1\n");
    test.stage_file("b.txt");
    let git_repo = test.git_repo();

    assert_rebase_complete!(fixup_row(&git_repo, WorktreeSource::Staged, head_commit));

    assert_history!(&test, base, &["head commit"]);
    assert_file_contents_at_head!(&test.repo, "a.txt", "a2\n");
    assert_file_contents_at_head!(&test.repo, "b.txt", "b1\n");
    assert!(git_repo.staged_diff(3).unwrap().is_none());
    assert!(git_repo.unstaged_diff(3).unwrap().is_none());

    assert!(matches!(git_repo.undo().unwrap(), UndoOutcome::Done { .. }));
    assert_eq!(
        row_paths(git_repo.staged_diff(3).unwrap()),
        ["a.txt", "b.txt"]
    );
}

/// The root commit is also a valid target, including when it is the only commit
/// on the branch — a row needs nothing else to fold into.
#[test]
fn folding_into_the_root_commit() {
    let test = common::TestRepo::new();
    let root = test.commit_file("a.txt", "a1\n", "root");
    test.write_file("a.txt", "a2\n");
    let git_repo = test.git_repo();

    assert_rebase_complete!(fixup_row(&git_repo, WorktreeSource::Unstaged, root));

    let mut walk = test.repo.revwalk().unwrap();
    walk.push_head().unwrap();
    assert_eq!(walk.count(), 1, "the branch must still be a single commit");
    assert_file_contents_at_head!(&test.repo, "a.txt", "a2\n");
    assert!(git_repo.unstaged_diff(3).unwrap().is_none());

    assert!(matches!(git_repo.undo().unwrap(), UndoOutcome::Done { .. }));
    assert_eq!(row_paths(git_repo.unstaged_diff(3).unwrap()), ["a.txt"]);
}

/// A row is not only edits: a deletion, a newly staged file and a rename all
/// have to survive the round trip through the temporary commit.
#[test]
fn a_row_can_delete_add_and_rename_files() {
    let test = common::TestRepo::new();
    let base = test.commit_files(
        &[
            ("gone.txt", "g\n"),
            ("old.txt", "content\n"),
            ("keep.txt", "k\n"),
        ],
        "base",
    );
    let target = test.commit_file("t.txt", "t\n", "target commit");

    // An unstaged deletion, folded in.
    std::fs::remove_file(test.repo.workdir().unwrap().join("gone.txt")).unwrap();
    let git_repo = test.git_repo();
    assert_rebase_complete!(fixup_row(&git_repo, WorktreeSource::Unstaged, target));
    assert!(!test.repo.workdir().unwrap().join("gone.txt").exists());
    assert!(git_repo.unstaged_diff(3).unwrap().is_none());
    assert!(matches!(git_repo.undo().unwrap(), UndoOutcome::Done { .. }));
    assert_eq!(row_paths(git_repo.unstaged_diff(3).unwrap()), ["gone.txt"]);
    assert!(matches!(git_repo.redo().unwrap(), UndoOutcome::Done { .. }));

    // A staged rename plus a brand-new staged file, folded in together.
    let target = test.commits_from_head(base)[0];
    let workdir_path = test.repo.workdir().unwrap();
    std::fs::rename(workdir_path.join("old.txt"), workdir_path.join("new.txt")).unwrap();
    let mut index = test.repo.index().unwrap();
    index.read(true).unwrap();
    index.remove_path(std::path::Path::new("old.txt")).unwrap();
    index.add_path(std::path::Path::new("new.txt")).unwrap();
    index.write().unwrap();
    test.write_file("added.txt", "added\n");
    test.stage_file("added.txt");

    assert_rebase_complete!(fixup_row(&git_repo, WorktreeSource::Staged, target));
    assert!(!workdir_path.join("old.txt").exists());
    assert_eq!(workdir(&test, "new.txt"), "content\n");
    assert_eq!(workdir(&test, "added.txt"), "added\n");
    assert!(git_repo.staged_diff(3).unwrap().is_none());

    assert!(matches!(git_repo.undo().unwrap(), UndoOutcome::Done { .. }));
    assert_eq!(
        row_paths(git_repo.staged_diff(3).unwrap()),
        ["added.txt", "new.txt", "old.txt"]
    );
}

/// An untracked file is in neither row, so a fold neither commits nor disturbs
/// it — before, after, or through an undo.
#[test]
fn untracked_files_survive_a_fold() {
    let (test, _base, target) = mixed_state();
    test.write_file("untracked.txt", "u\n");
    let git_repo = test.git_repo();

    assert_rebase_complete!(fixup_row(&git_repo, WorktreeSource::Staged, target));
    assert_eq!(workdir(&test, "untracked.txt"), "u\n");
    assert!(
        test.repo
            .find_commit(git2::Oid::from(&git_repo.head_oid().unwrap()))
            .unwrap()
            .tree()
            .unwrap()
            .get_path(std::path::Path::new("untracked.txt"))
            .is_err(),
        "an untracked file must not be swept into the fold"
    );

    assert!(matches!(git_repo.undo().unwrap(), UndoOutcome::Done { .. }));
    assert_eq!(workdir(&test, "untracked.txt"), "u\n");
}

/// Folds stack: each is its own undo entry, and unwinding them walks back
/// through the splits they were made from.
#[test]
fn consecutive_folds_undo_one_at_a_time() {
    let (test, base, target) = mixed_state();
    let git_repo = test.git_repo();

    assert_rebase_complete!(fixup_row(&git_repo, WorktreeSource::Staged, target));
    let target = test.commits_from_head(base)[0];
    assert_rebase_complete!(fixup_row(&git_repo, WorktreeSource::Unstaged, target));
    assert!(git_repo.staged_diff(3).unwrap().is_none());
    assert!(git_repo.unstaged_diff(3).unwrap().is_none());

    assert!(matches!(git_repo.undo().unwrap(), UndoOutcome::Done { .. }));
    assert!(git_repo.staged_diff(3).unwrap().is_none());
    assert_eq!(row_paths(git_repo.unstaged_diff(3).unwrap()), ["b.txt"]);

    assert!(matches!(git_repo.undo().unwrap(), UndoOutcome::Done { .. }));
    assert_eq!(row_paths(git_repo.staged_diff(3).unwrap()), ["a.txt"]);
    assert_eq!(row_paths(git_repo.unstaged_diff(3).unwrap()), ["b.txt"]);

    assert!(matches!(git_repo.redo().unwrap(), UndoOutcome::Done { .. }));
    assert!(matches!(git_repo.redo().unwrap(), UndoOutcome::Done { .. }));
    assert!(git_repo.staged_diff(3).unwrap().is_none());
    assert!(git_repo.unstaged_diff(3).unwrap().is_none());
}

/// Undo restores the index, so it must not run over an index the user has since
/// changed — it reports the history as stale and leaves their work alone.
#[test]
fn undo_refuses_once_the_index_has_moved_on() {
    let (test, base, target) = mixed_state();
    let git_repo = test.git_repo();
    assert_rebase_complete!(fixup_row(&git_repo, WorktreeSource::Staged, target));
    let head_after = git_repo.head_oid().unwrap();

    test.write_file("new.txt", "n\n");
    test.stage_file("new.txt");

    assert!(matches!(git_repo.undo().unwrap(), UndoOutcome::Stale));
    assert_eq!(git_repo.head_oid().unwrap(), head_after);
    assert_eq!(row_paths(git_repo.staged_diff(3).unwrap()), ["new.txt"]);
    assert_history!(&test, base, &["target commit", "later commit"]);
}

/// The conflict probe the squash dialog runs before opening the editor sees the
/// row the same way the fold itself does.
#[test]
fn the_conflict_probe_accepts_a_row_source() {
    let (test, base, target) = mixed_state();
    let git_repo = test.git_repo();
    let started = git_repo
        .lift_worktree_row(WorktreeSource::Unstaged)
        .unwrap()
        .unwrap();

    let probe = git_repo
        .squash_try_combine(
            &started.temp_oid,
            &Oid::from(target),
            "target commit\n\nfolded in",
            git_tailor::app::SquashMode::Squash,
            &started.temp_oid,
        )
        .unwrap();
    assert!(probe.is_none(), "the row merges cleanly, so no conflict");

    assert_rebase_complete!(
        git_repo
            .squash_commits(
                &started.temp_oid,
                &Oid::from(target),
                "target commit\n\nfolded in",
                &started.temp_oid,
            )
            .unwrap()
    );
    assert_history!(&test, base, &["target commit", "later commit"]);
    assert_eq!(row_paths(git_repo.staged_diff(3).unwrap()), ["a.txt"]);
}

/// A squash — as opposed to a fixup — folds the row in under a message the user
/// writes. The row has none of its own, so there is nothing to join, and every
/// other test here goes through the fixup path.
#[test]
fn a_squash_folds_a_row_under_a_new_message() {
    let (test, base, target) = mixed_state();
    let git_repo = test.git_repo();

    let started = git_repo
        .lift_worktree_row(WorktreeSource::Staged)
        .unwrap()
        .unwrap();
    assert!(
        git_repo
            .squash_try_combine(
                &started.temp_oid,
                &Oid::from(target),
                "a message of the user's own",
                git_tailor::app::SquashMode::Squash,
                &started.temp_oid,
            )
            .unwrap()
            .is_none()
    );
    assert_rebase_complete!(
        git_repo
            .squash_commits(
                &started.temp_oid,
                &Oid::from(target),
                "a message of the user's own",
                &started.temp_oid,
            )
            .unwrap()
    );

    assert_history!(
        &test,
        base,
        &["a message of the user's own", "later commit"]
    );
    assert_file_contents_at_head!(&test.repo, "a.txt", "a2\n");
    assert!(git_repo.staged_diff(3).unwrap().is_none());
    assert_eq!(row_paths(git_repo.unstaged_diff(3).unwrap()), ["b.txt"]);
}

/// The staged row's conflict path, which differs from the unstaged row's in
/// what `finish` leaves in the index: the staged changes are now committed, so
/// the index matches the new tip rather than the whole working tree.
#[test]
fn resolving_a_conflicted_fold_from_the_staged_row_completes_it() {
    let test = common::TestRepo::new();
    let base = test.commit_files(&[("a.txt", "one\ntwo\n"), ("b.txt", "b1\n")], "base");
    let target = test.commit_file("a.txt", "one\nTARGET\n", "target commit");
    test.commit_file("a.txt", "one\nLATER\n", "later commit");
    test.write_file("a.txt", "one\nWIP\n");
    test.stage_file("a.txt");
    // The other row: a tracked file edited but not staged.
    test.write_file("b.txt", "b2\n");
    let git_repo = test.git_repo();

    let started = git_repo
        .lift_worktree_row(WorktreeSource::Staged)
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

    test.write_file("a.txt", "one\nRESOLVED\n");
    git_repo
        .auto_stage_resolved_conflicts(&state.conflicting_files)
        .unwrap();
    let Resume::Squash(ctx) = &state.resume else {
        panic!("expected a squash-tree conflict, got {:?}", state.resume);
    };
    let next = git_repo
        .squash_finalize(ctx, "target commit", &state.original_branch_oid, None)
        .unwrap();

    let state = expect_rebase_conflict!(next);
    test.write_file("a.txt", "one\nLATER\n");
    git_repo
        .auto_stage_resolved_conflicts(&state.conflicting_files)
        .unwrap();
    assert_rebase_complete!(git_repo.rebase_continue(&state).unwrap());

    assert_history!(&test, base, &["target commit", "later commit"]);
    assert_eq!(workdir(&test, "a.txt"), "one\nLATER\n");
    assert!(
        git_repo.staged_diff(3).unwrap().is_none(),
        "the staged row's changes are committed now"
    );
    assert_eq!(
        row_paths(git_repo.unstaged_diff(3).unwrap()),
        ["b.txt"],
        "the other row stays on its own side"
    );
    assert!(matches!(git_repo.undo().unwrap(), UndoOutcome::Done { .. }));
    assert!(matches!(git_repo.undo().unwrap(), UndoOutcome::Empty));
}

/// When the other row's changes cannot be carried onto the resolution, they are
/// kept as they were rather than dropped. The user's work is all still there,
/// even though it no longer sits on top of what they resolved to.
#[test]
fn a_resolution_that_clashes_with_the_other_row_keeps_the_row() {
    let test = common::TestRepo::new();
    let lines = |second: &str, eighth: &str| format!("1\n{second}\n3\n4\n5\n6\n7\n{eighth}\n9\n");
    let base = test.commit_file("a.txt", &lines("2", "8"), "base");
    let target = test.commit_file("a.txt", &lines("TARGET", "8"), "target commit");
    test.commit_file("a.txt", &lines("LATER", "8"), "later commit");
    // Source row: the staged edit near the top, which is what conflicts with the
    // target. Other row: an unstaged edit far enough away to separate cleanly.
    test.write_file("a.txt", &lines("WIP", "8"));
    test.stage_file("a.txt");
    test.write_file("a.txt", &lines("WIP", "UNSTAGED"));
    let git_repo = test.git_repo();

    let started = git_repo
        .lift_worktree_row(WorktreeSource::Staged)
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

    // Resolve by also rewriting the line the *other* row is sitting on, so the
    // other row cannot be carried onto the resolution.
    test.write_file("a.txt", &lines("RESOLVED", "RESOLVED-TOO"));
    git_repo
        .auto_stage_resolved_conflicts(&state.conflicting_files)
        .unwrap();
    let Resume::Squash(ctx) = &state.resume else {
        panic!("expected a squash-tree conflict, got {:?}", state.resume);
    };
    let mut outcome = git_repo
        .squash_finalize(ctx, "target commit", &state.original_branch_oid, None)
        .unwrap();
    while let git_tailor::repo::RebaseOutcome::Conflict(state) = outcome {
        test.write_file("a.txt", &lines("LATER", "RESOLVED-TOO"));
        git_repo
            .auto_stage_resolved_conflicts(&state.conflicting_files)
            .unwrap();
        outcome = git_repo.rebase_continue(&state).unwrap();
    }

    assert_history!(&test, base, &["target commit", "later commit"]);
    // Whichever way the carry went, the unstaged edit is still on disk. Losing
    // it silently is the failure this guards against.
    assert!(
        workdir(&test, "a.txt").contains("UNSTAGED")
            || workdir(&test, "a.txt").contains("RESOLVED-TOO"),
        "the other row's content must survive the fold, got {:?}",
        workdir(&test, "a.txt")
    );
    assert!(
        git_repo.unstaged_diff(3).unwrap().is_some(),
        "the other row must still be an unstaged change, not folded into history"
    );
}
