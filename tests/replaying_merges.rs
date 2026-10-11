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

//! A rewrite that would replay a merge.
//!
//! Replays walk the range with a plain revwalk and cherry-pick each commit, so
//! a merge in the range is either flattened or fails in libgit2's words
//! ("mainline branch is not specified"). Every rewrite refuses it before doing
//! any work: drop, move, squash and its probe, edit and split here; reword and
//! autofixup in their own tests.

#[allow(dead_code)]
mod common;

use common::prelude::*;
use git_tailor::app::SquashMode;

/// Commit a merge onto HEAD, of a one-commit branch forked from HEAD's parent.
fn commit_merge(test: &common::TestRepo) -> git2::Oid {
    let head = test.repo.head().unwrap().peel_to_commit().unwrap();
    let fork = head.parent(0).unwrap();
    let sig = git2::Signature::now("Test User", "test@example.com").unwrap();
    let side = test
        .repo
        .commit(None, &sig, &sig, "Side", &fork.tree().unwrap(), &[&fork])
        .unwrap();
    let side = test.repo.find_commit(side).unwrap();
    test.repo
        .commit(
            Some("HEAD"),
            &sig,
            &sig,
            "Merge side",
            &head.tree().unwrap(),
            &[&head, &side],
        )
        .unwrap()
}

/// Our own refusal, with the branch left where it was.
fn assert_refused(
    result: anyhow::Result<impl std::fmt::Debug>,
    git_repo: &impl GitRepo,
    head: &Oid,
) {
    let msg = format!(
        "{:#}",
        result.expect_err("replaying a merge must be refused")
    );
    assert!(!msg.contains("mainline"), "libgit2's wording leaked: {msg}");
    assert!(msg.contains("merge"), "{msg}");
    assert_eq!(
        &git_repo.head_oid().unwrap(),
        head,
        "the branch must be untouched"
    );
}

#[test]
fn dropping_below_a_merge_is_refused() {
    let test = common::TestRepo::new();
    test.commit_file("a.txt", "base\n", "base");
    let to_drop = test.commit_file("b.txt", "b\n", "To drop");
    test.commit_file("c.txt", "c\n", "Above");
    commit_merge(&test);
    test.commit_file("d.txt", "d\n", "Top");

    let mut git_repo = test.git_repo();
    let head = git_repo.head_oid().unwrap();
    let result = git_repo.drop_commit(&Oid::from(to_drop), &head);

    assert_refused(result, &git_repo, &head);
}

/// Dropping the root replays everything behind HEAD.
#[test]
fn dropping_the_root_below_a_merge_is_refused() {
    let test = common::TestRepo::new();
    let root = test.commit_file("a.txt", "root\n", "Root");
    test.commit_file("b.txt", "b\n", "Next");
    test.commit_file("c.txt", "c\n", "Above");
    commit_merge(&test);

    let mut git_repo = test.git_repo();
    let head = git_repo.head_oid().unwrap();
    let result = git_repo.drop_commit(&Oid::from(root), &head);

    assert_refused(result, &git_repo, &head);
}

#[test]
fn moving_across_a_merge_is_refused() {
    let test = common::TestRepo::new();
    let base = test.commit_file("a.txt", "base\n", "base");
    test.commit_file("b.txt", "b\n", "First");
    test.commit_file("c.txt", "c\n", "Second");
    commit_merge(&test);
    let moved = test.commit_file("d.txt", "d\n", "Moved");

    let mut git_repo = test.git_repo();
    let head = git_repo.head_oid().unwrap();
    let result = git_repo.move_commit(&Oid::from(moved), Some(&Oid::from(base)), &head);

    assert_refused(result, &git_repo, &head);
}

#[test]
fn squashing_across_a_merge_is_refused() {
    let test = common::TestRepo::new();
    test.commit_file("a.txt", "base\n", "base");
    let target = test.commit_file("b.txt", "b\n", "Target");
    test.commit_file("c.txt", "c\n", "Above");
    commit_merge(&test);
    let source = test.commit_file("b.txt", "b\nfix\n", "Source");

    let mut git_repo = test.git_repo();
    let head = git_repo.head_oid().unwrap();
    let result = git_repo.squash_commits(
        &Oid::from(source),
        &Oid::from(target),
        "Target".into(),
        &head,
    );

    assert_refused(result, &git_repo, &head);
}

/// The probe runs before the editor opens, so refusing there spares writing a
/// message for a squash that cannot happen.
#[test]
fn the_squash_probe_refuses_across_a_merge_too() {
    let test = common::TestRepo::new();
    test.commit_file("a.txt", "base\n", "base");
    let target = test.commit_file("b.txt", "b\n", "Target");
    test.commit_file("c.txt", "c\n", "Above");
    commit_merge(&test);
    let source = test.commit_file("b.txt", "b\nfix\n", "Source");

    let mut git_repo = test.git_repo();
    let head = git_repo.head_oid().unwrap();
    let result = git_repo.squash_try_combine(
        &Oid::from(source),
        &Oid::from(target),
        "Target".into(),
        SquashMode::Squash,
        &head,
    );

    assert_refused(result, &git_repo, &head);
}

/// Refused when the edit begins: the replay comes after the user has done the
/// edit in a shell, which is too late to find out.
#[test]
fn editing_below_a_merge_is_refused_before_the_edit_begins() {
    let test = common::TestRepo::new();
    test.commit_file("a.txt", "base\n", "base");
    let to_edit = test.commit_file("b.txt", "b\n", "To edit");
    test.commit_file("c.txt", "c\n", "Above");
    commit_merge(&test);

    let mut git_repo = test.git_repo();
    let head = git_repo.head_oid().unwrap();
    let result = git_repo.begin_edit(&Oid::from(to_edit), &head);

    assert_refused(result, &git_repo, &head);
}

/// Refused before the split's work, not after: splitting per hunk group
/// computes the whole hunk group matrix first, with a progress screen.
#[test]
fn splitting_below_a_merge_is_refused_before_any_work() {
    let test = common::TestRepo::new();
    let base = test.commit_file("a.txt", "base\n", "base");
    let to_split = test.commit_files(&[("b.txt", "b\n"), ("c.txt", "c\n")], "To split");
    test.commit_file("d.txt", "d\n", "Above");
    commit_merge(&test);

    let mut git_repo = test.git_repo();
    let head = git_repo.head_oid().unwrap();
    let mut progress_reports = 0;
    let result = git_repo.split_commit_per_hunk_group(
        &Oid::from(to_split),
        &head,
        &Oid::from(base),
        &mut |_| {
            progress_reports += 1;
            true
        },
    );

    assert_refused(result, &git_repo, &head);
    assert_eq!(
        progress_reports, 0,
        "nothing may be computed before the refusal"
    );
}
