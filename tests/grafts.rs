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

//! Grafts, which give a commit parents other than the ones it records.
//!
//! `.git/info/grafts` is deprecated but still honored by git, and by libgit2:
//! a grafted commit reports the parents the file names. Rewriting it writes
//! those for real, baking the graft into the branch. Replace refs
//! (`git replace`) have the same shape, but libgit2 does not follow them, so
//! git-tailor rewrites the commits as they are and needs no guard for them.

#[allow(dead_code)]
mod common;

use common::prelude::*;

/// Three commits on top of a base, the middle one grafted to have no parents.
/// Returns `(test, grafted, head)`.
fn grafted_history() -> (common::TestRepo, git2::Oid, git2::Oid) {
    let test = common::TestRepo::new();
    test.commit_file("base.txt", "base\n", "base");
    test.commit_file("a.txt", "a\n", "first");
    let grafted = test.commit_file("b.txt", "b\n", "grafted");
    let head = test.commit_file("c.txt", "c\n", "third");
    let grafts = test.repo.path().join("info").join("grafts");
    std::fs::create_dir_all(grafts.parent().unwrap()).unwrap();
    std::fs::write(&grafts, format!("{grafted}\n")).unwrap();
    (test, grafted, head)
}

/// Rewording a grafted root used to write a genuinely parentless commit,
/// severing the branch for good: removing the graft no longer brings the
/// history back.
#[test]
fn rewording_a_grafted_commit_is_refused() {
    let (test, grafted, head) = grafted_history();
    let mut git_repo = test.git_repo();

    let result = git_repo.reword_commit(&Oid::from(grafted), "reworded".into(), &Oid::from(head));

    let error = format!("{:#}", result.expect_err("this must be refused"));
    assert!(error.contains("graft"), "the refusal must say why: {error}");
    assert_eq!(git_repo.head_oid().unwrap(), Oid::from(head));
}

/// A graft that substitutes a different parent rather than none is baked in
/// just the same, so it is refused too.
#[test]
fn rewriting_a_commit_grafted_onto_another_parent_is_refused() {
    let test = common::TestRepo::new();
    let base = test.commit_file("base.txt", "base\n", "base");
    test.commit_file("a.txt", "a\n", "first");
    let grafted = test.commit_file("b.txt", "b\n", "grafted");
    let head = test.commit_file("c.txt", "c\n", "third");
    let grafts = test.repo.path().join("info").join("grafts");
    std::fs::create_dir_all(grafts.parent().unwrap()).unwrap();
    std::fs::write(&grafts, format!("{grafted} {base}\n")).unwrap();
    let mut git_repo = test.git_repo();

    let result = git_repo.drop_commit(&Oid::from(grafted), &Oid::from(head));

    let error = format!("{:#}", result.expect_err("this must be refused"));
    assert!(error.contains("graft"), "{error}");
    assert_eq!(git_repo.head_oid().unwrap(), Oid::from(head));
}

/// Commits above a grafted one keep their real parents and are rewritten onto
/// it unchanged, so they stay rewritable.
#[test]
fn commits_above_a_graft_are_unaffected() {
    let (test, _grafted, head) = grafted_history();
    let mut git_repo = test.git_repo();

    git_repo
        .reword_commit(&Oid::from(head), "reworded".into(), &Oid::from(head))
        .unwrap();
}

/// A commit on a merged-in side branch has a base off HEAD's first-parent
/// line. The guard must check only what is rebuilt, not walk on to the root
/// and blame a graft far below for what is a merge in the range.
#[test]
fn a_rewrite_across_a_merge_is_refused_for_the_merge_not_a_graft_below() {
    let test = common::TestRepo::new();
    test.commit_file("base.txt", "base\n", "base");
    let grafted = test.commit_file("g.txt", "g\n", "grafted");
    let fork = test.commit_file("a.txt", "a\n", "fork point");
    test.create_branch("side", fork);
    test.checkout("refs/heads/side");
    test.commit_file("s1.txt", "s1\n", "side one");
    let side_tip = test.commit_file("s2.txt", "s2\n", "side two");
    test.checkout("refs/heads/main");
    let mainline = test.commit_file("c.txt", "c\n", "mainline");

    let ours = test.repo.find_commit(mainline).unwrap();
    let theirs = test.repo.find_commit(side_tip).unwrap();
    let tree_oid = test
        .repo
        .merge_commits(&ours, &theirs, None)
        .unwrap()
        .write_tree_to(&test.repo)
        .unwrap();
    let tree = test.repo.find_tree(tree_oid).unwrap();
    let sig = git2::Signature::now("Test User", "test@example.com").unwrap();
    let head = test
        .repo
        .commit(
            Some("HEAD"),
            &sig,
            &sig,
            "merge side",
            &tree,
            &[&ours, &theirs],
        )
        .unwrap();

    let grafts = test.repo.path().join("info").join("grafts");
    std::fs::create_dir_all(grafts.parent().unwrap()).unwrap();
    std::fs::write(&grafts, format!("{grafted}\n")).unwrap();
    let mut git_repo = test.git_repo();

    let result = git_repo.reword_commit(&Oid::from(side_tip), "reworded".into(), &Oid::from(head));

    let error = format!("{:#}", result.expect_err("a merge lies in the range"));
    assert!(error.contains("merge"), "{error}");
}

/// Rewording a merge at HEAD rebuilds only the merge; its parents stay as they
/// are. A graft on the side branch it merges is never rewritten, so it is no
/// reason to refuse.
#[test]
fn rewording_a_merge_ignores_a_graft_on_the_branch_it_merges() {
    let test = common::TestRepo::new();
    let base = test.commit_file("base.txt", "base\n", "base");
    test.create_branch("side", base);
    test.checkout("refs/heads/side");
    let grafted = test.commit_file("s1.txt", "s1\n", "side one");
    let side_tip = test.commit_file("s2.txt", "s2\n", "side two");
    test.checkout("refs/heads/main");
    let mainline = test.commit_file("a.txt", "a\n", "mainline");

    let ours = test.repo.find_commit(mainline).unwrap();
    let theirs = test.repo.find_commit(side_tip).unwrap();
    let tree_oid = test
        .repo
        .merge_commits(&ours, &theirs, None)
        .unwrap()
        .write_tree_to(&test.repo)
        .unwrap();
    let tree = test.repo.find_tree(tree_oid).unwrap();
    let sig = git2::Signature::now("Test User", "test@example.com").unwrap();
    let head = test
        .repo
        .commit(
            Some("HEAD"),
            &sig,
            &sig,
            "merge side",
            &tree,
            &[&ours, &theirs],
        )
        .unwrap();

    let grafts = test.repo.path().join("info").join("grafts");
    std::fs::create_dir_all(grafts.parent().unwrap()).unwrap();
    std::fs::write(&grafts, format!("{grafted}\n")).unwrap();
    let mut git_repo = test.git_repo();

    git_repo
        .reword_commit(&Oid::from(head), "reworded".into(), &Oid::from(head))
        .unwrap();
}

/// A merge above the reworded commit keeps the branch it merges; nothing on
/// that branch is rebuilt. A graft there must not be blamed for what the merge
/// refusal reports.
#[test]
fn a_graft_on_a_branch_merged_above_is_not_blamed() {
    let test = common::TestRepo::new();
    let base = test.commit_file("base.txt", "base\n", "base");
    test.create_branch("side", base);
    test.checkout("refs/heads/side");
    let grafted = test.commit_file("s1.txt", "s1\n", "side one");
    let side_tip = test.commit_file("s2.txt", "s2\n", "side two");
    test.checkout("refs/heads/main");
    let reworded = test.commit_file("a.txt", "a\n", "to reword");

    let ours = test.repo.find_commit(reworded).unwrap();
    let theirs = test.repo.find_commit(side_tip).unwrap();
    let tree_oid = test
        .repo
        .merge_commits(&ours, &theirs, None)
        .unwrap()
        .write_tree_to(&test.repo)
        .unwrap();
    let tree = test.repo.find_tree(tree_oid).unwrap();
    let sig = git2::Signature::now("Test User", "test@example.com").unwrap();
    let head = test
        .repo
        .commit(
            Some("HEAD"),
            &sig,
            &sig,
            "merge side",
            &tree,
            &[&ours, &theirs],
        )
        .unwrap();

    let grafts = test.repo.path().join("info").join("grafts");
    std::fs::create_dir_all(grafts.parent().unwrap()).unwrap();
    std::fs::write(&grafts, format!("{grafted}\n")).unwrap();
    let mut git_repo = test.git_repo();

    let result = git_repo.reword_commit(&Oid::from(reworded), "reworded".into(), &Oid::from(head));

    let error = format!("{:#}", result.expect_err("a merge lies in the range"));
    assert!(error.contains("merge"), "{error}");
}

/// Autofixup squashes its pairs one at a time. A pair whose target is grafted
/// must refuse the whole batch before any pair lands: a refusal half-way
/// leaves the earlier squashes on the branch with no undo entry for them.
#[test]
fn autofixup_with_a_grafted_target_refuses_before_squashing_anything() {
    let test = common::TestRepo::new();
    let base = test.commit_file("base.txt", "base\n", "base");
    let grafted = test.commit_file("a.txt", "a\n", "A");
    test.commit_file("b.txt", "b\n", "B");
    test.commit_file("b.txt", "b2\n", "fixup! B");
    let head = test.commit_file("a.txt", "a2\n", "fixup! A");
    let grafts = test.repo.path().join("info").join("grafts");
    std::fs::create_dir_all(grafts.parent().unwrap()).unwrap();
    std::fs::write(&grafts, format!("{grafted}\n")).unwrap();
    let mut git_repo = test.git_repo();

    let result = git_repo.autofixup(
        &Oid::from(head),
        &Oid::from(base),
        &std::collections::HashMap::new(),
    );

    let error = format!("{:#}", result.expect_err("the batch must be refused"));
    assert!(error.contains("graft"), "{error}");
    assert_eq!(
        git_repo.head_oid().unwrap(),
        Oid::from(head),
        "no pair may have been squashed"
    );
}
