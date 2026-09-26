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

//! Shallow clones, where the oldest commit is not the root it appears to be.
//!
//! `git clone --depth` grafts the history: the oldest fetched commit reports no
//! parents locally while upstream it has plenty. Every "is this the root?" test
//! in the rewrite engine asks `parent_count() == 0`, and in a shallow clone that
//! question has the wrong answer.
//!
//! Taking it at face value builds a genuinely parentless commit, which severs
//! the branch from all the history behind the graft. Locally that is undoable;
//! pushed, it truncates the history everyone else shares.
//!
//! These tests shell out to `git` — which git-tailor itself never does, see
//! CLAUDE.md — because libgit2 cannot *create* a shallow clone, and a fixture
//! faked with `.git/shallow` by hand would be testing our idea of the format
//! rather than the thing git actually produces.

#[allow(dead_code)]
mod common;

use common::prelude::*;

/// A depth-limited clone of a four-commit history, and the oid of the commit
/// that is the graft boundary — a root locally, not upstream.
struct Shallow {
    _origin: common::TestRepo,
    dir: std::path::PathBuf,
    repo: git2::Repository,
    boundary: git2::Oid,
    head: git2::Oid,
}

impl Drop for Shallow {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn shallow_clone(depth: &str, name: &str) -> Option<Shallow> {
    let origin = common::TestRepo::new();
    origin.commit_file("a.txt", "v1\n", "first");
    origin.commit_file("b.txt", "b\n", "second");
    origin.commit_file("c.txt", "c\n", "third");
    origin.commit_file("d.txt", "d\n", "fourth");

    let dir = std::env::temp_dir().join(format!("gt-shallow-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let out = std::process::Command::new("git")
        .args(["clone", "--depth", depth, "--no-local"])
        .arg(origin.repo.workdir().unwrap())
        .arg(&dir)
        .output()
        .expect("git must be on PATH to run these tests");
    assert!(
        out.status.success(),
        "shallow clone failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let repo = git2::Repository::open(&dir).unwrap();
    assert!(repo.is_shallow(), "the fixture must actually be shallow");
    let head = repo.head().unwrap().target().unwrap();

    // Walk down to the commit that claims to have no parents.
    let mut boundary = head;
    loop {
        let commit = repo.find_commit(boundary).unwrap();
        if commit.parent_count() == 0 {
            break;
        }
        boundary = commit.parent_id(0).unwrap();
    }

    Some(Shallow {
        _origin: origin,
        dir,
        repo,
        boundary,
        head,
    })
}

/// Dropping the graft boundary used to succeed and leave a parentless tip —
/// a branch severed from everything behind the graft.
#[test]
fn dropping_the_graft_boundary_is_refused() {
    let Some(s) = shallow_clone("3", "drop") else {
        return;
    };
    let mut git_repo = Git2Repo::open(s.dir.clone()).unwrap();

    let result = git_repo.drop_commit(&Oid::from(s.boundary), &Oid::from(s.head));

    let error = format!("{:#}", result.expect_err("this must be refused"));
    assert!(
        error.contains("shallow"),
        "the refusal must say why: {error}"
    );
    assert_eq!(
        git_repo.head_oid().unwrap(),
        Oid::from(s.head),
        "and nothing may have moved"
    );
}

/// The same boundary reached through a move, which rebuilds the root too.
#[test]
fn moving_the_graft_boundary_is_refused() {
    let Some(s) = shallow_clone("3", "move") else {
        return;
    };
    let second = s.repo.find_commit(s.boundary).unwrap();
    let _ = second;
    let mut git_repo = Git2Repo::open(s.dir.clone()).unwrap();

    let result = git_repo.move_commit(
        &Oid::from(s.boundary),
        Some(&Oid::from(s.head)),
        &Oid::from(s.head),
    );

    let error = format!("{:#}", result.expect_err("this must be refused"));
    assert!(error.contains("shallow"), "{error}");
}

/// Commits above the boundary are ordinary and must still be rewritable —
/// refusing the whole repository would make git-tailor useless on a shallow
/// clone, which is a perfectly normal way to work.
#[test]
fn commits_above_the_boundary_are_unaffected() {
    let Some(s) = shallow_clone("3", "above") else {
        return;
    };
    let head_commit = s.repo.find_commit(s.head).unwrap();
    let just_below = head_commit.parent_id(0).unwrap();
    assert_ne!(just_below, s.boundary, "the fixture needs room above");

    let mut git_repo = Git2Repo::open(s.dir.clone()).unwrap();
    assert_rebase_complete!(
        git_repo
            .drop_commit(&Oid::from(just_below), &Oid::from(s.head))
            .unwrap()
    );
}

/// Splitting the boundary makes its first piece an orphan root, so it is
/// refused on the same grounds.
#[test]
fn splitting_the_graft_boundary_is_refused() {
    let Some(s) = shallow_clone("3", "split") else {
        return;
    };
    let mut git_repo = Git2Repo::open(s.dir.clone()).unwrap();

    let result = git_repo.split_commit_per_file(&Oid::from(s.boundary), &Oid::from(s.head));

    let error = format!("{:#}", result.expect_err("this must be refused"));
    assert!(error.contains("shallow"), "{error}");
}

/// Squashing into the graft boundary makes the squash commit an orphan root,
/// so it is refused on the same grounds as drop, move, and split.
#[test]
fn squashing_into_the_graft_boundary_is_refused() {
    let Some(s) = shallow_clone("3", "squash") else {
        return;
    };
    let head_commit = s.repo.find_commit(s.head).unwrap();
    let source = head_commit.parent_id(0).unwrap();
    let mut git_repo = Git2Repo::open(s.dir.clone()).unwrap();

    let result = git_repo.squash_commits(
        &Oid::from(source),
        &Oid::from(s.boundary),
        "squashed".into(),
        &Oid::from(s.head),
    );

    let error = format!("{:#}", result.expect_err("this must be refused"));
    assert!(error.contains("shallow"), "{error}");
    assert_eq!(
        git_repo.head_oid().unwrap(),
        Oid::from(s.head),
        "and nothing may have moved"
    );
}

/// Moving an ordinary commit to the very beginning of the branch (`--all`
/// mode's "insert before the first visible entry") must not be refused just
/// because the repository happens to be shallow somewhere. The commit being
/// moved is not the graft boundary, so nothing behind it is at risk.
#[test]
fn moving_an_ordinary_commit_to_root_is_unaffected() {
    let Some(s) = shallow_clone("3", "move-root") else {
        return;
    };
    let head_commit = s.repo.find_commit(s.head).unwrap();
    let just_below = head_commit.parent_id(0).unwrap();
    assert_ne!(just_below, s.boundary, "the fixture needs room above");

    let mut git_repo = Git2Repo::open(s.dir.clone()).unwrap();
    let result = git_repo.move_commit(&Oid::from(just_below), None, &Oid::from(s.head));

    assert!(
        result.is_ok(),
        "moving an ordinary commit to root must not be refused: {result:?}"
    );
}

/// A repository that is not shallow keeps its real root rewritable — the guard
/// must key on the graft, not on "has no parents".
#[test]
fn a_real_root_commit_is_still_rewritable() {
    let test = common::TestRepo::new();
    let root = test.commit_file("a.txt", "v1\n", "root");
    test.commit_file("b.txt", "b\n", "second");
    let head = test.commit_file("c.txt", "c\n", "third");

    let mut git_repo = test.git_repo();
    assert_rebase_complete!(
        git_repo
            .drop_commit(&Oid::from(root), &Oid::from(head))
            .unwrap()
    );
}
