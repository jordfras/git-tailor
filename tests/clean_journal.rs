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

//! Integration tests for `--clean-journal` (`GitRepo::clean_journal`): wiping
//! the journal file and every `refs/git-tailor/*` ref, including stray refs.

#[allow(dead_code)]
mod common;

use common::prelude::*;

fn head_oid(test: &common::TestRepo) -> git2::Oid {
    test.repo.head().unwrap().target().unwrap()
}

fn git_tailor_ref_count(test: &common::TestRepo) -> usize {
    test.repo
        .references_glob("refs/git-tailor/*")
        .unwrap()
        .count()
}

fn journal_path(test: &common::TestRepo) -> std::path::PathBuf {
    test.repo.path().join("git-tailor").join("journal.json")
}

#[test]
fn clean_removes_journal_file_and_all_refs_including_stray() {
    let test = common::TestRepo::new();
    let base = test.commit_file("a.txt", "a\n", "base");
    let c1 = test.commit_file("b.txt", "b\n", "add b");
    test.commit_file("c.txt", "c\n", "add c");
    let git_repo = test.git_repo();

    // A real op seeds journal.json + undo pins.
    git_repo
        .drop_commit(&Oid::from(c1), &Oid::from(head_oid(&test)))
        .unwrap();
    // A stray ref not referenced by the journal.
    test.repo
        .reference("refs/git-tailor/undo/999", base, true, "stray")
        .unwrap();

    assert!(journal_path(&test).exists(), "journal file should exist");
    let before = git_tailor_ref_count(&test);
    assert!(before >= 2, "expected undo pins plus the stray ref");

    let summary = git_repo.clean_journal().unwrap();

    assert!(summary.journal_removed);
    assert_eq!(summary.refs_removed, before);
    assert!(!journal_path(&test).exists(), "journal file should be gone");
    assert_eq!(
        git_tailor_ref_count(&test),
        0,
        "every refs/git-tailor/* ref should be removed"
    );
}

#[test]
fn clean_on_a_pristine_repo_is_a_noop() {
    let test = common::TestRepo::new();
    test.commit_file("a.txt", "a\n", "base");
    let git_repo = test.git_repo();

    let summary = git_repo.clean_journal().unwrap();

    assert!(!summary.journal_removed);
    assert_eq!(summary.refs_removed, 0);
}

#[test]
fn clean_removes_refs_even_with_no_journal_file() {
    // The core promise: refs are found by namespace, not from the journal, so
    // they are cleared even when no journal file exists (deleted/corrupt/never
    // written).
    let test = common::TestRepo::new();
    let base = test.commit_file("a.txt", "a\n", "base");
    let git_repo = test.git_repo();
    test.repo
        .reference("refs/git-tailor/undo/0", base, true, "stray")
        .unwrap();
    test.repo
        .reference("refs/git-tailor/orig", base, true, "stray")
        .unwrap();
    assert!(!journal_path(&test).exists());

    let summary = git_repo.clean_journal().unwrap();

    assert!(!summary.journal_removed, "no journal file was present");
    assert_eq!(summary.refs_removed, 2);
    assert_eq!(git_tailor_ref_count(&test), 0);
}
