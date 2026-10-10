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

//! An operation that fails after it has already moved the branch.
//!
//! A read-only directory makes the checkout fail once the ref has moved: the
//! collision checks look for untracked files, not permissions.

#![cfg(unix)]

#[allow(dead_code)]
mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use common::prelude::*;
use git_tailor::repo::{InProgress, JournalStatus, UndoOutcome};

/// Makes a directory read-only until dropped, so the temporary repository can
/// still be removed when an assertion fails.
struct ReadOnly(PathBuf);

impl ReadOnly {
    fn new(dir: &Path) -> Self {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o555)).unwrap();
        Self(dir.to_path_buf())
    }
}

impl Drop for ReadOnly {
    fn drop(&mut self) {
        let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
    }
}

fn workdir(test: &common::TestRepo) -> PathBuf {
    test.repo.workdir().unwrap().to_path_buf()
}

fn in_progress(git_repo: &mut impl GitRepo) -> bool {
    matches!(
        git_repo.read_journal().unwrap(),
        JournalStatus::Recovered(_)
    )
}

#[test]
fn a_drop_that_fails_after_moving_the_branch_can_be_undone() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "base\n", "base");
    let to_drop = test.commit_file("d/x.txt", "x\n", "Add x");
    test.commit_file("y.txt", "y\n", "Add y");

    let mut git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();
    let result = {
        let _guard = ReadOnly::new(&workdir(&test).join("d"));
        git_repo.drop_commit(&Oid::from(to_drop), &head_oid)
    };

    let err = result.expect_err("removing d/x.txt from a read-only directory fails");
    assert!(
        format!("{err:#}").contains("stopped part-way"),
        "the error says the branch moved: {err:#}"
    );
    assert_history!(&test, base, &["Add y"]);

    match git_repo.undo().unwrap() {
        UndoOutcome::Done { label } => assert_eq!(label, "Drop"),
        other => panic!("expected Done, got {other:?}"),
    }
    assert_history!(&test, base, &["Add x", "Add y"]);
}

#[test]
fn a_resume_that_fails_after_moving_the_branch_can_be_undone() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "base\n", "base");
    let to_drop = test.commit_file("a.txt", "base\ndropped\n", "Add dropped line");
    test.commit_file("a.txt", "base\ndropped\nhead\n", "Add head line");
    test.commit_file("d/z.txt", "z\n", "Add z");

    let mut git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();
    let state = expect_rebase_conflict!(
        git_repo
            .drop_commit(&Oid::from(to_drop), &head_oid)
            .unwrap()
    );

    test.write_file("a.txt", "base\nhead\n");
    git_repo.stage_file(Path::new("a.txt")).unwrap();
    let result = {
        let _guard = ReadOnly::new(&workdir(&test).join("d"));
        git_repo.rebase_continue(&state)
    };

    let err = result.expect_err("writing d/z.txt into a read-only directory fails");
    assert!(
        format!("{err:#}").contains("stopped part-way"),
        "the error says the branch moved: {err:#}"
    );
    assert!(
        !in_progress(&mut git_repo),
        "the paused conflict is over, so nothing may offer to resume it"
    );

    match git_repo.undo().unwrap() {
        UndoOutcome::Done { label } => assert_eq!(label, "Drop"),
        other => panic!("expected Done, got {other:?}"),
    }
    assert_history!(&test, base, &["Add dropped line", "Add head line", "Add z"]);
}

/// A later pair conflicts, and writing that conflict out fails after the
/// branch moved to it. The record is what lets recovery resume or abort it.
#[test]
fn a_conflict_that_fails_to_write_keeps_its_record() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "base\n", "base");
    test.commit_file("a.txt", "base\nT1\n", "Add T1");
    test.commit_file("a.txt", "base\nT1\nF1\n", "fixup! Add T1");
    test.commit_file("d/k.txt", "target version\n", "Add T2");
    test.commit_file("d/k.txt", "mid version\n", "Unrelated edit to k");
    test.commit_file("d/k.txt", "source version\n", "fixup! Add T2");

    let mut git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();
    let result = {
        let _guard = ReadOnly::new(&workdir(&test).join("d"));
        git_repo.autofixup(&head_oid, &Oid::from(base), &Default::default())
    };

    assert!(
        result.is_err(),
        "expected the conflict write to fail, got {result:?}"
    );
    match git_repo.read_journal().unwrap() {
        JournalStatus::Recovered(recorded) => {
            assert!(matches!(*recorded, InProgress::Conflict(_)))
        }
        other => panic!("the conflict on disk lost its record: {other:?}"),
    }
}
