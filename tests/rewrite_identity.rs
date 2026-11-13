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

//! A rewritten commit keeps its author, as `git rebase` keeps it, and is
//! committed by the configured user, dated now in the local time zone. Each
//! test takes a turn at the environment, so an identity the runner exported
//! cannot stand in for the configured one.

#[allow(dead_code)]
mod common;

use common::env::EnvTurn;
use common::prelude::*;
use git2::{Signature, Time};

/// Offset -07:00 and a date in 2001, so a copied committer is told apart from
/// one made now, whatever the local time zone.
fn original_identity() -> Signature<'static> {
    Signature::new(
        "Original Author",
        "original@example.com",
        &Time::new(1_000_000_000, -7 * 60),
    )
    .unwrap()
}

fn commit_file_as_original(
    test: &common::TestRepo,
    path: &str,
    content: &str,
    message: &str,
) -> git2::Oid {
    test.write_file(path, content);
    test.stage_file(path);
    test.commit_signed(message, &original_identity())
}

fn now_seconds() -> i64 {
    Signature::now("x", "y").unwrap().when().seconds()
}

fn assert_rewritten_by_user(test: &common::TestRepo, oid: git2::Oid, since: i64) {
    let commit = test.repo.find_commit(oid).unwrap();
    let author = commit.author();
    assert_eq!(author.name().unwrap(), "Original Author");
    assert_eq!(author.email().unwrap(), "original@example.com");
    assert_eq!(author.when(), original_identity().when());

    let committer = commit.committer();
    assert_eq!(committer.name().unwrap(), "Test User");
    assert_eq!(committer.email().unwrap(), "test@example.com");
    let when = committer.when();
    assert!(
        (since..=now_seconds()).contains(&when.seconds()),
        "committer date {} is not now",
        when.seconds()
    );
    assert_eq!(
        when.offset_minutes(),
        Signature::now("x", "y").unwrap().when().offset_minutes(),
        "committer date is not in the local time zone"
    );
}

#[test]
fn a_reworded_commit_and_its_descendants_are_committed_by_the_user() {
    let _env = EnvTurn::take();
    let test = common::TestRepo::new();
    let base = test.commit_file("a.txt", "base\n", "base");
    let to_reword = commit_file_as_original(&test, "x.txt", "x\n", "old message");
    let tip = commit_file_as_original(&test, "y.txt", "y\n", "descendant");

    let since = now_seconds();
    test.git_repo()
        .reword_commit(&Oid::from(to_reword), "new message".into(), &Oid::from(tip))
        .unwrap();

    for oid in test.commits_from_head(base) {
        assert_rewritten_by_user(&test, oid, since);
    }
}

#[test]
fn moved_commits_are_committed_by_the_user() {
    let _env = EnvTurn::take();
    let test = common::TestRepo::new();
    let base = test.commit_file("a.txt", "base\n", "base");
    let a = commit_file_as_original(&test, "x.txt", "x\n", "A");
    let b = commit_file_as_original(&test, "y.txt", "y\n", "B");

    let since = now_seconds();
    let outcome = test
        .git_repo()
        .move_commit(&Oid::from(a), Some(&Oid::from(b)), &Oid::from(b))
        .unwrap();
    assert_rebase_complete!(outcome);

    for oid in test.commits_from_head(base) {
        assert_rewritten_by_user(&test, oid, since);
    }
}

#[test]
fn a_squashed_commit_is_committed_by_the_user() {
    let _env = EnvTurn::take();
    let test = common::TestRepo::new();
    let base = test.commit_file("a.txt", "base\n", "base");
    let target = commit_file_as_original(&test, "x.txt", "x\n", "target");
    let source = commit_file_as_original(&test, "y.txt", "y\n", "source");

    let since = now_seconds();
    let outcome = test
        .git_repo()
        .squash_commits(
            &Oid::from(source),
            &Oid::from(target),
            "squashed".into(),
            &Oid::from(source),
        )
        .unwrap();
    assert_rebase_complete!(outcome);

    for oid in test.commits_from_head(base) {
        assert_rewritten_by_user(&test, oid, since);
    }
}

#[test]
fn split_pieces_are_committed_by_the_user() {
    let _env = EnvTurn::take();
    let test = common::TestRepo::new();
    let base = test.commit_file("a.txt", "base\n", "base");
    test.write_file("x.txt", "x\n");
    test.write_file("y.txt", "y\n");
    test.stage_file("x.txt");
    test.stage_file("y.txt");
    let to_split = test.commit_signed("two files", &original_identity());

    let since = now_seconds();
    test.git_repo()
        .split_commit_per_file(&Oid::from(to_split), &Oid::from(to_split))
        .unwrap();

    let pieces = test.commits_from_head(base);
    assert_eq!(pieces.len(), 2);
    for oid in pieces {
        assert_rewritten_by_user(&test, oid, since);
    }
}

#[test]
fn a_rewrite_without_a_committer_identity_fails_and_leaves_the_branch() {
    let _env = EnvTurn::take();
    let test = common::TestRepo::new();
    let _base = test.commit_file("a.txt", "base\n", "base");
    let tip = commit_file_as_original(&test, "x.txt", "x\n", "old message");
    // Empty rather than unset: a global user.name would fill an unset one.
    test.set_config("user.name", "");

    let err = test
        .git_repo()
        .reword_commit(&Oid::from(tip), "new message".into(), &Oid::from(tip))
        .unwrap_err();

    assert!(format!("{err:#}").contains("user.name"), "{err:#}");
    assert_eq!(test.repo.head().unwrap().target().unwrap(), tip);
}

/// Refused before it starts: a rewrite whose first pick conflicts pauses
/// without writing a commit, and only a resolved conflict would then find the
/// identity missing.
#[test]
fn a_conflicting_rewrite_without_a_committer_identity_is_refused_before_it_pauses() {
    let _env = EnvTurn::take();
    let test = common::TestRepo::new();
    let base = test.commit_file("a.txt", "base\n", "base");
    let _first = commit_file_as_original(&test, "a.txt", "first\n", "first");
    let second = commit_file_as_original(&test, "a.txt", "second\n", "second");
    test.set_config("user.name", "");

    let mut git_repo = test.git_repo();
    let err = git_repo
        .move_commit(
            &Oid::from(second),
            Some(&Oid::from(base)),
            &Oid::from(second),
        )
        .unwrap_err();

    assert!(format!("{err:#}").contains("user.name"), "{err:#}");
    assert_eq!(test.repo.head().unwrap().target().unwrap(), second);
    assert!(matches!(
        git_repo.read_journal().unwrap(),
        git_tailor::repo::JournalStatus::None
    ));
}

/// The identity can go missing while the edit shell runs. Like any failure to
/// replay the descendants, that rewinds the branch and ends the edit, naming
/// the commits made in the shell so they can be recovered.
#[test]
fn finishing_an_edit_without_a_committer_identity_rewinds_it() {
    let _env = EnvTurn::take();
    let test = common::TestRepo::new();
    let base = test.commit_file("README", "readme\n", "base");
    let parent = test.commit_file("a.txt", "a\n", "add a");
    let edited = test.commit_file("b.txt", "b\n", "add b");
    let tip = test.commit_file("c.txt", "c\n", "add c");

    let mut git_repo = test.git_repo();
    git_repo
        .begin_edit(&Oid::from(edited), &Oid::from(tip))
        .unwrap();
    let parent_commit = test.repo.find_commit(parent).unwrap();
    test.repo
        .reset(parent_commit.as_object(), git2::ResetType::Mixed, None)
        .unwrap();
    test.write_file("b.txt", "b EDITED\n");
    test.stage_file("b.txt");
    let abandoned = test.commit("add b (edited)");
    test.set_config("user.name", "");

    let err = git_repo.finish_edit(&Oid::from(edited)).unwrap_err();

    let msg = format!("{err:#}");
    assert!(msg.contains("aborted"), "{msg}");
    assert!(
        msg.contains(&Oid::from(abandoned).short().to_string()),
        "the commits the edit leaves behind must be findable: {msg}"
    );
    assert_eq!(test.commits_from_head(base), vec![parent, edited, tip]);
    assert!(matches!(
        git_repo.read_journal().unwrap(),
        git_tailor::repo::JournalStatus::None
    ));
}

/// What every handler that may commit asks before its editor or dialog opens.
#[test]
fn a_repository_without_a_committer_identity_is_uncommittable() {
    let _env = EnvTurn::take();
    let test = common::TestRepo::new();
    let git_repo = test.git_repo();
    assert!(git_repo.refuse_if_uncommittable().is_ok());

    test.set_config("user.name", "");

    let err = test.git_repo().refuse_if_uncommittable().unwrap_err();
    assert!(format!("{err:#}").contains("user.name"), "{err:#}");
}
