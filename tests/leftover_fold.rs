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

//! Integration tests for rewriting over the temporary commit a working-tree
//! squash leaves behind when its journal record is gone before it finishes.

#[allow(dead_code)]
mod common;

use common::prelude::*;
use git_tailor::repo::{LiftedRow, WorktreeSource};

struct Interrupted {
    test: common::TestRepo,
    git_repo: Git2Repo,
    base: git2::Oid,
    first: git2::Oid,
    lifted: LiftedRow,
}

/// `base`, `first` and `second`, then a fold of the staged row lifted onto the
/// branch and left there, with the unstaged row parked.
fn interrupted_fold() -> Interrupted {
    let test = common::TestRepo::new();
    let base = test.commit_file("a.txt", "a1\n", "base");
    let first = test.commit_file("b.txt", "b1\n", "first");
    test.commit_file("c.txt", "c1\n", "second");
    test.write_file("a.txt", "a2\n");
    test.stage_file("a.txt");
    test.write_file("b.txt", "b2\n");
    let mut git_repo = test.git_repo();
    let lifted = common::lift_at_head(&mut git_repo, WorktreeSource::Staged)
        .unwrap()
        .expect("the staged row has changes to fold in");
    Interrupted {
        test,
        git_repo,
        base,
        first,
        lifted,
    }
}

fn assert_refused_for_the_leftover(
    result: anyhow::Result<impl std::fmt::Debug>,
    lifted: &LiftedRow,
) {
    let error = format!(
        "{:#}",
        result.expect_err("the leftover commit is in the range")
    );
    assert!(error.contains(lifted.temp_oid.short()), "{error}");
    assert!(error.contains("interrupted working-tree squash"), "{error}");
    assert!(error.contains("gt --list-rescued"), "{error}");
}

/// `--clean-journal` drops the fold's record and leaves the branch on its
/// temporary commit.
#[test]
fn a_drop_below_a_leftover_fold_commit_is_refused() {
    let Interrupted {
        test: _test,
        mut git_repo,
        first,
        lifted,
        ..
    } = interrupted_fold();
    git_repo.clean_journal().unwrap();
    let head = git_repo.head_oid().unwrap();

    let result = git_repo.drop_commit(&Oid::from(first), &head);

    assert_refused_for_the_leftover(result, &lifted);
    assert_eq!(
        git_repo.head_oid().unwrap(),
        head,
        "nothing may be rewritten"
    );
}

/// Startup discards a stale record when the branch has moved on, here by a
/// commit on top of the temporary one.
#[test]
fn a_move_across_a_leftover_fold_commit_is_refused() {
    let Interrupted {
        test,
        mut git_repo,
        first,
        lifted,
        ..
    } = interrupted_fold();
    test.commit_file("d.txt", "d1\n", "on top");
    git_repo.clear_journal().unwrap();
    let head = git_repo.head_oid().unwrap();
    let on_top = test.repo.head().unwrap().target().unwrap();

    let result = git_repo.move_commit(&Oid::from(first), Some(&Oid::from(on_top)), &head);

    assert_refused_for_the_leftover(result, &lifted);
}

/// Autofixup works out its own range from the pairs it folds, so it is checked
/// on its own.
#[test]
fn an_autofixup_across_a_leftover_fold_commit_is_refused() {
    let Interrupted {
        test,
        mut git_repo,
        base,
        lifted,
        ..
    } = interrupted_fold();
    test.commit_file("b.txt", "b3\n", "fixup! first");
    git_repo.clear_journal().unwrap();
    let head = git_repo.head_oid().unwrap();

    let result = common::autofixup_as_shown(&mut git_repo, &head, base, &Default::default());

    assert_refused_for_the_leftover(result, &lifted);
}

/// Only what a rewrite replays is checked: rewording a commit made on top of
/// the leftover leaves the leftover where it is.
#[test]
fn a_reword_above_a_leftover_fold_commit_runs() {
    let Interrupted {
        test,
        mut git_repo,
        lifted,
        ..
    } = interrupted_fold();
    let on_top = test.commit_file("d.txt", "d1\n", "on top");
    git_repo.clear_journal().unwrap();
    let head = git_repo.head_oid().unwrap();

    git_repo
        .reword_commit(&Oid::from(on_top), "reworded".into(), &head)
        .unwrap();

    let tip = test.repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(tip.message_bytes(), b"reworded");
    assert_eq!(Oid::from(tip.parent_id(0).unwrap()), lifted.temp_oid);
}

/// A record startup kept because rescuing its fold failed still names the
/// temporary commit, but no fold is running from it once the branch has moved.
#[test]
fn a_record_the_branch_has_moved_past_does_not_excuse_its_commit() {
    let Interrupted {
        test,
        mut git_repo,
        first,
        lifted,
        ..
    } = interrupted_fold();
    test.commit_file("d.txt", "d1\n", "on top");
    let head = git_repo.head_oid().unwrap();

    let result = git_repo.drop_commit(&Oid::from(first), &head);

    assert_refused_for_the_leftover(result, &lifted);
}

/// A fold that would squash across a leftover is refused before it lifts
/// anything, so there is nothing to unwind.
#[test]
fn a_fold_across_a_leftover_fold_commit_is_refused_before_the_lift() {
    let Interrupted {
        test,
        mut git_repo,
        first,
        lifted,
        ..
    } = interrupted_fold();
    git_repo.clean_journal().unwrap();
    test.write_file("c.txt", "c2\n");
    test.stage_file("c.txt");
    let head = git_repo.head_oid().unwrap();
    let stashes = stash_count(&test);

    let result = git_repo.lift_worktree_row(WorktreeSource::Staged, &Oid::from(first), &head);

    assert_refused_for_the_leftover(result, &lifted);
    assert_eq!(git_repo.head_oid().unwrap(), head, "nothing may be lifted");
    assert_eq!(stash_count(&test), stashes, "nothing may be parked");
}

/// The leftover check reads the journal only when a temporary commit turns up,
/// so one that cannot be read does not stand in the way of the rewrite itself.
/// Recording it is what fails — and `gt` refuses to start on such a journal, so
/// getting here means it became unreadable mid-session.
#[test]
fn an_unreadable_journal_does_not_block_the_rewrite_itself() {
    let test = common::TestRepo::new();
    let base = test.commit_file("a.txt", "a1\n", "base");
    let middle = test.commit_file("b.txt", "b1\n", "middle");
    test.commit_file("c.txt", "c1\n", "top");
    let journal_dir = test.repo.path().join("git-tailor");
    std::fs::create_dir_all(&journal_dir).unwrap();
    std::fs::write(journal_dir.join("journal.json"), "not json").unwrap();
    let mut git_repo = test.git_repo();
    let head = git_repo.head_oid().unwrap();

    let result = git_repo.drop_commit(&Oid::from(middle), &head);

    assert!(result.is_err(), "the drop cannot be recorded");
    let tip = test.repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(tip.message_bytes(), b"top");
    assert_eq!(
        tip.parent_id(0).unwrap(),
        base,
        "the drop itself landed, with the middle commit gone"
    );
}

fn stash_count(test: &common::TestRepo) -> usize {
    let mut repo = git2::Repository::open(test.repo.path()).unwrap();
    let mut n = 0;
    repo.stash_foreach(|_, _, _| {
        n += 1;
        true
    })
    .unwrap();
    n
}

/// The refusal says to drop the leftover, so dropping it is what gt allows: the
/// commits above it are replayed without it.
#[test]
fn a_leftover_fold_commit_can_itself_be_dropped() {
    let Interrupted {
        test,
        mut git_repo,
        lifted,
        ..
    } = interrupted_fold();
    test.commit_file("d.txt", "d1\n", "on top");
    git_repo.clear_journal().unwrap();
    let head = git_repo.head_oid().unwrap();

    git_repo.drop_commit(&lifted.temp_oid, &head).unwrap();

    let tip = test.repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(tip.message_bytes(), b"on top");
    assert_eq!(
        Oid::from(tip.parent_id(0).unwrap()),
        lifted.tip_before,
        "the leftover is gone from under the commit above it"
    );
}

/// The journal only decides whether a fold owns the leftover, so one that
/// cannot be read still ends in the refusal that names the commit.
#[test]
fn an_unreadable_journal_still_refuses_with_the_leftover() {
    let Interrupted {
        test,
        mut git_repo,
        first,
        lifted,
        ..
    } = interrupted_fold();
    git_repo.clean_journal().unwrap();
    let journal_dir = test.repo.path().join("git-tailor");
    std::fs::create_dir_all(&journal_dir).unwrap();
    std::fs::write(journal_dir.join("journal.json"), "not json").unwrap();
    let head = git_repo.head_oid().unwrap();

    let result = git_repo.drop_commit(&Oid::from(first), &head);

    assert_refused_for_the_leftover(result, &lifted);
}

/// A record belongs to its branch: another branch made on the temporary commit
/// sits on the same tip, but no fold is running on it.
#[test]
fn a_record_from_another_branch_does_not_excuse_its_commit() {
    let Interrupted {
        test,
        mut git_repo,
        first,
        lifted,
        ..
    } = interrupted_fold();
    let temp = test
        .repo
        .find_commit(git2::Oid::from(&lifted.temp_oid))
        .unwrap();
    test.repo.branch("other", &temp, false).unwrap();
    test.repo.set_head("refs/heads/other").unwrap();
    let head = git_repo.head_oid().unwrap();

    let result = git_repo.drop_commit(&Oid::from(first), &head);

    assert_refused_for_the_leftover(result, &lifted);
}

/// An empty row is the answer to the question the user asked, so it comes
/// before anything about the range being in the way.
#[test]
fn an_empty_row_is_reported_before_a_leftover_in_the_range() {
    let Interrupted {
        test: _test,
        mut git_repo,
        first,
        ..
    } = interrupted_fold();
    git_repo.clean_journal().unwrap();
    let head = git_repo.head_oid().unwrap();

    let lifted = git_repo
        .lift_worktree_row(WorktreeSource::Staged, &Oid::from(first), &head)
        .unwrap();

    assert!(lifted.is_none(), "nothing is staged");
}
