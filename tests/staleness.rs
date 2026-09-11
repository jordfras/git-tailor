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

//! The repository moving while git-tailor is looking away.
//!
//! git-tailor reads the branch tip when the commit list loads and works from
//! that. The session lock keeps a second git-tailor out, but nothing stops
//! `git commit` in another terminal, an IDE's git integration, or a script —
//! and a rewrite then force-writes the branch to a tip computed from a view
//! that is no longer true.
//!
//! Two ways it goes wrong, and both end with a ref pointing somewhere nobody
//! asked for: the branch has *moved*, or HEAD is on a *different branch*
//! altogether.

#[allow(dead_code)]
mod common;

use common::prelude::*;

/// A commit made elsewhere on the same branch, between loading the list and
/// choosing an operation.
#[test]
fn a_rewrite_refuses_when_the_branch_moved_underneath_it() {
    let test = common::TestRepo::new();
    test.commit_file("a.txt", "v1\n", "base");
    let to_drop = test.commit_file("b.txt", "b\n", "to drop");
    let head = test.commit_file("c.txt", "c\n", "head");

    // git-tailor loaded the commit list here.
    let mut git_repo = test.git_repo();
    let stale_head = Oid::from(head);

    // Another terminal commits on the same branch.
    let theirs = test.commit_file("theirs.txt", "their work\n", "their commit");

    let result = git_repo.drop_commit(&Oid::from(to_drop), &stale_head);

    let error = format!("{:#}", result.expect_err("a stale view must be refused"));
    assert!(
        error.contains("moved") || error.contains("changed"),
        "the refusal must say the branch moved: {error}"
    );
    assert_eq!(
        git_repo.head_oid().unwrap(),
        Oid::from(theirs),
        "and their commit must still be the tip"
    );
}

/// The same, through a squash — every entry point reads a tip it was handed.
#[test]
fn a_squash_refuses_when_the_branch_moved_underneath_it() {
    let test = common::TestRepo::new();
    test.commit_file("a.txt", "v1\n", "base");
    let target = test.commit_file("b.txt", "b\n", "target");
    let source = test.commit_file("c.txt", "c\n", "source");

    let mut git_repo = test.git_repo();
    let stale_head = Oid::from(source);
    let theirs = test.commit_file("theirs.txt", "their work\n", "their commit");

    let result = git_repo.squash_commits(
        &Oid::from(source),
        &Oid::from(target),
        b"squashed",
        &stale_head,
    );

    assert!(result.is_err(), "a stale view must be refused: {result:?}");
    assert_eq!(git_repo.head_oid().unwrap(), Oid::from(theirs));
}

/// HEAD moved to another branch while a conflict was paused. Resuming used to
/// write the rewind onto whatever HEAD pointed at *now*, rewriting a branch
/// that had nothing to do with the operation.
#[test]
fn aborting_refuses_after_head_moved_to_another_branch() {
    let test = common::TestRepo::new();
    test.commit_file("a.txt", "base\n", "base");
    let to_drop = test.commit_file("a.txt", "base\ndropped\n", "add dropped");
    let head = test.commit_file("a.txt", "base\ndropped\nhead\n", "add head");

    let mut git_repo = test.git_repo();
    let state = expect_rebase_conflict!(
        git_repo
            .drop_commit(&Oid::from(to_drop), &Oid::from(head))
            .unwrap()
    );

    // A different branch, pointing somewhere else entirely.
    let elsewhere = test
        .repo
        .find_commit(head)
        .unwrap()
        .parent(0)
        .unwrap()
        .parent(0)
        .unwrap();
    test.repo.branch("side", &elsewhere, false).unwrap();
    let side_before = test
        .repo
        .find_branch("side", git2::BranchType::Local)
        .unwrap()
        .get()
        .target();
    test.repo.set_head("refs/heads/side").unwrap();

    let result = git_repo.rebase_abort(&state);

    assert!(
        result.is_err(),
        "aborting onto a different branch must be refused: {result:?}"
    );
    let side_after = test
        .repo
        .find_branch("side", git2::BranchType::Local)
        .unwrap()
        .get()
        .target();
    assert_eq!(
        side_before, side_after,
        "the unrelated branch must not be rewritten"
    );
}

/// Resuming has the same exposure as aborting.
#[test]
fn continuing_refuses_after_head_moved_to_another_branch() {
    let test = common::TestRepo::new();
    test.commit_file("a.txt", "base\n", "base");
    let to_drop = test.commit_file("a.txt", "base\ndropped\n", "add dropped");
    let head = test.commit_file("a.txt", "base\ndropped\nhead\n", "add head");

    let mut git_repo = test.git_repo();
    let state = expect_rebase_conflict!(
        git_repo
            .drop_commit(&Oid::from(to_drop), &Oid::from(head))
            .unwrap()
    );

    let elsewhere = test
        .repo
        .find_commit(head)
        .unwrap()
        .parent(0)
        .unwrap()
        .parent(0)
        .unwrap();
    test.repo.branch("side", &elsewhere, false).unwrap();
    let side_before = test
        .repo
        .find_branch("side", git2::BranchType::Local)
        .unwrap()
        .get()
        .target();
    test.repo.set_head("refs/heads/side").unwrap();

    let result = git_repo.rebase_continue(&state);

    assert!(result.is_err(), "must be refused: {result:?}");
    let side_after = test
        .repo
        .find_branch("side", git2::BranchType::Local)
        .unwrap()
        .get()
        .target();
    assert_eq!(side_before, side_after);
}
