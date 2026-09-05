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

//! Regression tests for squash/fixup across a rename commit.
//!
//! History shape:
//!   base     — creates foo.txt (20 lines)
//!   target   — edits line 1 of foo.txt
//!   rename   — renames foo.txt → bar.txt
//!   source   — edits line 20 of bar.txt
//!
//! Squashing source into target should complete without conflict because the
//! two edits touch different line ranges and the rename is handled transparently.

use crate::common;
use crate::common::prelude::*;
use git_tailor::repo::Git2Repo;

fn make_content() -> String {
    (1..=20).map(|i| format!("line {i}\n")).collect()
}

/// Build the standard 4-commit rename history and return (target_oid, source_oid).
/// History:
///   base     — creates foo.txt (20 lines)
///   target   — edits line 1 of foo.txt
///   rename   — renames foo.txt → bar.txt
///   middle   — edits line 10 of bar.txt (non-conflicting)
///   source   — edits line 20 of bar.txt
fn setup_rename_history(test: &common::TestRepo) -> (git2::Oid, git2::Oid) {
    let content = make_content();
    test.commit_file("foo.txt", &content, "base");

    let target_content = content.replacen("line 1\n", "line 1 edited by target\n", 1);
    let target = test.commit_file("foo.txt", &target_content, "target: edit line 1");

    test.rename_file("foo.txt", "bar.txt", None, "rename foo to bar");

    // Additional commit on bar.txt between the rename and the source.
    let current = std::fs::read_to_string(test.repo.workdir().unwrap().join("bar.txt")).unwrap();
    let middle_content = current.replacen("line 10\n", "line 10 edited by middle\n", 1);
    test.commit_file("bar.txt", &middle_content, "middle: edit line 10");

    let current = std::fs::read_to_string(test.repo.workdir().unwrap().join("bar.txt")).unwrap();
    let source_content = current.replacen("line 20\n", "line 20 edited by source\n", 1);
    let source = test.commit_file("bar.txt", &source_content, "source: edit line 20");

    (target, source)
}

#[test]
fn squash_across_rename_completes_without_conflict() {
    let test = common::TestRepo::new();
    let (target, source) = setup_rename_history(&test);

    let git_repo = test.git_repo();
    let result = git_repo
        .squash_commits(
            &Oid::from(source),
            &Oid::from(target),
            "squashed",
            &Oid::from(source),
        )
        .unwrap();

    assert_rebase_complete!(result);
}

#[test]
fn squash_try_combine_across_rename_returns_none() {
    let test = common::TestRepo::new();
    let (target, source) = setup_rename_history(&test);

    let git_repo = test.git_repo();
    let result = git_repo
        .squash_try_combine(
            &Oid::from(source),
            &Oid::from(target),
            "squashed",
            SquashMode::Squash,
            &Oid::from(source),
        )
        .unwrap();

    assert!(result.is_none(), "expected no conflict, got {result:?}");
}

#[test]
fn squash_across_rename_with_descendant() {
    // An additional commit after source must be replayed cleanly.
    let test = common::TestRepo::new();
    let (target, source) = setup_rename_history(&test);
    let descendant = test.commit_file("other.txt", "extra\n", "descendant");

    let git_repo = test.git_repo();
    let result = git_repo
        .squash_commits(
            &Oid::from(source),
            &Oid::from(target),
            "squashed",
            &Oid::from(descendant),
        )
        .unwrap();

    assert_rebase_complete!(result);
}

/// Regression test using the exact commits from test_repo_3 where the bug was
/// first observed. Skipped when the repo is not present on the machine.
#[test]
fn squash_across_rename_real_repo() {
    let repo_path = std::path::PathBuf::from("/home/thomas/Projects/test_repo_3");
    if !repo_path.exists() {
        eprintln!("test_repo_3 not found — skipping");
        return;
    }

    let git_repo = Git2Repo::open(repo_path).unwrap();

    // History: a61ba7c (target) … 9765260 (rename) … 38df584 (middle) … 835b1cc (source/HEAD)
    let source = Oid::from("835b1cc70ae351b2007356da6dc6ff8e0d717ae8");
    let target = Oid::from("a61ba7cc41b3589feb09a40fcecacf31f02f46bf");
    let head = source.clone();

    let result = git_repo
        .squash_commits(&source, &target, "squashed", &head)
        .unwrap();

    assert_rebase_complete!(result);
}
