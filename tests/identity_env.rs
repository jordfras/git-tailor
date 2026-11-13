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

//! git's identity environment variables, honored as git honors them. Each
//! test changes the environment of the whole process, which is why they live
//! in a test binary of their own and take turns.

#[allow(dead_code)]
mod common;

use common::env::EnvTurn;
use common::prelude::*;
use git_tailor::repo::{Authorship, CommitOutcome};

fn head_commit(test: &common::TestRepo) -> git2::Commit<'_> {
    test.repo.head().unwrap().peel_to_commit().unwrap()
}

#[test]
fn a_rewrite_takes_its_committer_from_the_environment() {
    let mut env = EnvTurn::take();
    let test = common::TestRepo::new();
    let _base = test.commit_file("a.txt", "base\n", "base");
    let tip = test.commit_file("x.txt", "x\n", "old message");

    env.set(&[
        ("GIT_COMMITTER_NAME", "Env Committer"),
        ("GIT_COMMITTER_EMAIL", "committer@env.example"),
        ("GIT_COMMITTER_DATE", "2005-04-07T22:13:13 +0530"),
    ]);
    test.git_repo()
        .reword_commit(&Oid::from(tip), "new message".into(), &Oid::from(tip))
        .unwrap();

    let committer = head_commit(&test).committer().to_owned();
    assert_eq!(committer.name().unwrap(), "Env Committer");
    assert_eq!(committer.email().unwrap(), "committer@env.example");
    assert_eq!(
        committer.when(),
        git2::Time::new(1_112_892_193, 5 * 60 + 30)
    );
}

/// `TZ` is how a test can pick the local time zone; on Windows the C runtime
/// does not read it from the environment Rust writes.
#[cfg(unix)]
#[test]
fn a_rewrite_is_dated_in_the_local_time_zone() {
    let mut env = EnvTurn::take();
    let test = common::TestRepo::new();
    let _base = test.commit_file("a.txt", "base\n", "base");
    let tip = test.commit_file("x.txt", "x\n", "old message");

    // POSIX counts west of UTC as positive, so this is UTC+05:30.
    env.set(&[("TZ", "XYZ-05:30")]);
    test.git_repo()
        .reword_commit(&Oid::from(tip), "new message".into(), &Oid::from(tip))
        .unwrap();

    let when = head_commit(&test).committer().when();
    assert_eq!(when.offset_minutes(), 5 * 60 + 30);
    assert_eq!(when.sign(), '+');
}

#[test]
fn committing_staged_changes_takes_the_author_from_the_environment() {
    let mut env = EnvTurn::take();
    let test = common::TestRepo::new();
    let _base = test.commit_file("a.txt", "base\n", "base");
    test.write_file("a.txt", "staged\n");
    test.stage_file("a.txt");

    env.set(&[
        ("GIT_AUTHOR_NAME", "Env Author"),
        ("GIT_AUTHOR_EMAIL", "author@env.example"),
        ("GIT_AUTHOR_DATE", "2006-01-02T03:04:05 -0300"),
    ]);
    assert_eq!(
        test.git_repo().commit_staged("staged\n".into()).unwrap(),
        CommitOutcome::Committed
    );

    let commit = head_commit(&test);
    let author = commit.author();
    assert_eq!(author.name().unwrap(), "Env Author");
    assert_eq!(author.email().unwrap(), "author@env.example");
    assert_eq!(author.when(), git2::Time::new(1_136_181_845, -3 * 60));
    assert_eq!(commit.committer().name().unwrap(), "Test User");
}

/// A rewrite keeps every author it replays, so like `git rebase` it needs a
/// committer identity and nothing else: an author one that cannot be
/// resolved is none of its business.
#[test]
fn a_rewrite_needs_no_author_identity() {
    let mut env = EnvTurn::take();
    let test = common::TestRepo::new();
    let _base = test.commit_file("a.txt", "base\n", "base");
    let tip = test.commit_file("x.txt", "x\n", "old message");
    test.set_config("user.name", "");

    env.set(&[
        ("GIT_COMMITTER_NAME", "Env Committer"),
        ("GIT_COMMITTER_EMAIL", "committer@env.example"),
    ]);
    test.git_repo()
        .reword_commit(&Oid::from(tip), "new message".into(), &Oid::from(tip))
        .unwrap();

    let commit = head_commit(&test);
    assert_eq!(commit.committer().name().unwrap(), "Env Committer");
    assert_eq!(commit.author().name().unwrap(), "Test User");
}

/// Checked before the commit-staged editor opens, so it must cover the author
/// too: a message typed and then refused is lost.
#[test]
fn a_new_commit_needs_an_author_identity_where_a_rewrite_does_not() {
    let mut env = EnvTurn::take();
    let test = common::TestRepo::new();
    test.set_config("user.name", "");

    env.set(&[
        ("GIT_COMMITTER_NAME", "Env Committer"),
        ("GIT_COMMITTER_EMAIL", "committer@env.example"),
    ]);
    let git_repo = test.git_repo();

    assert!(git_repo.refuse_if_uncommittable(Authorship::Kept).is_ok());
    assert!(git_repo.refuse_if_uncommittable(Authorship::New).is_err());
}
