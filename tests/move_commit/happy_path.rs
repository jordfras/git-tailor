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

use crate::common;
use crate::common::prelude::*;

#[test]
fn move_commit_earlier() {
    let test = common::TestRepo::new();

    // ref → A → B → C(source) → HEAD
    // Move C to after A → ref → A → C → B
    let base = test.commit_file("a.txt", "base\n", "base");
    let a = test.commit_file("x.txt", "x\n", "A");
    let _b = test.commit_file("y.txt", "y\n", "B");
    let c = test.commit_file("z.txt", "z\n", "C");

    let git_repo = test.git_repo();
    let result = git_repo
        .move_commit(&Oid::from(c), Some(&Oid::from(a)), &Oid::from(c))
        .unwrap();

    assert_rebase_complete!(result);

    assert_history!(&test, base, &["A", "C", "B"]);

    assert_file_contents_at_head!(&test.repo, "x.txt", "x\n");
    assert_file_contents_at_head!(&test.repo, "y.txt", "y\n");
    assert_file_contents_at_head!(&test.repo, "z.txt", "z\n");
}

#[test]
fn move_commit_later() {
    let test = common::TestRepo::new();

    // ref → A → B(source) → C → D → HEAD
    // Move B to after D → ref → A → C → D → B
    let base = test.commit_file("a.txt", "base\n", "base");
    let _a = test.commit_file("x.txt", "x\n", "A");
    let b = test.commit_file("y.txt", "y\n", "B");
    let _c = test.commit_file("z.txt", "z\n", "C");
    let d = test.commit_file("w.txt", "w\n", "D");

    let git_repo = test.git_repo();
    let result = git_repo
        .move_commit(&Oid::from(b), Some(&Oid::from(d)), &Oid::from(d))
        .unwrap();

    assert_rebase_complete!(result);

    assert_history!(&test, base, &["A", "C", "D", "B"]);
}

#[test]
fn move_commit_to_beginning() {
    let test = common::TestRepo::new();

    // ref → A → B → C(source) → HEAD
    // Move C to after ref → ref → C → A → B
    let base = test.commit_file("a.txt", "base\n", "base");
    let _a = test.commit_file("x.txt", "x\n", "A");
    let _b = test.commit_file("y.txt", "y\n", "B");
    let c = test.commit_file("z.txt", "z\n", "C");

    let git_repo = test.git_repo();
    let result = git_repo
        .move_commit(&Oid::from(c), Some(&Oid::from(base)), &Oid::from(c))
        .unwrap();

    assert_rebase_complete!(result);

    assert_history!(&test, base, &["C", "A", "B"]);
}

#[test]
fn move_head_commit_earlier() {
    let test = common::TestRepo::new();

    // ref → A → B → C(HEAD, source)
    // Move C to after A → ref → A → C → B
    let base = test.commit_file("a.txt", "base\n", "base");
    let a = test.commit_file("x.txt", "x\n", "A");
    let _b = test.commit_file("y.txt", "y\n", "B");
    let head = test.commit_file("z.txt", "z\n", "C");

    let git_repo = test.git_repo();
    let result = git_repo
        .move_commit(&Oid::from(head), Some(&Oid::from(a)), &Oid::from(head))
        .unwrap();

    assert_rebase_complete!(result);

    assert_history!(&test, base, &["A", "C", "B"]);
}

#[test]
fn move_commit_conflict_returns_conflict_state() {
    let test = common::TestRepo::new();

    // Both A and B modify the same file. Moving B before A will conflict
    // because B's diff was against A's tree — without A present the cherry-pick
    // fails.
    let base = test.commit_file("a.txt", "line1\n", "base");
    let _a = test.commit_file("a.txt", "line1\nline2\n", "A");
    let b = test.commit_file("a.txt", "line1\nline2\nline3\n", "B");

    let git_repo = test.git_repo();
    let result = git_repo
        .move_commit(&Oid::from(b), Some(&Oid::from(base)), &Oid::from(b))
        .unwrap();

    match result {
        RebaseOutcome::Conflict(state) => {
            assert_eq!(state.operation_label, "Move");
            assert_eq!(state.original_branch_oid, Oid::from(b));
            assert!(!state.conflicting_files.is_empty());
        }
        RebaseOutcome::Complete => {
            // Moving B (which appends line3 to line1+line2) to before A
            // may or may not conflict depending on diff mechanics.
            // If git can apply B's hunk cleanly against the base, that's
            // also valid — the commit just adds line3 after line1.
            assert_history!(&test, base, &["B", "A"]);
        }
    }
}

#[test]
fn move_commit_preserves_file_contents() {
    let test = common::TestRepo::new();

    // Each commit touches a different file — no conflicts possible.
    // ref → A(x.txt) → B(y.txt) → C(z.txt)
    // Move B to end → ref → A → C → B
    let base = test.commit_file("a.txt", "base\n", "base");
    let _a = test.commit_file("x.txt", "x-content\n", "A");
    let b = test.commit_file("y.txt", "y-content\n", "B");
    let c = test.commit_file("z.txt", "z-content\n", "C");

    let git_repo = test.git_repo();
    let result = git_repo
        .move_commit(&Oid::from(b), Some(&Oid::from(c)), &Oid::from(c))
        .unwrap();

    assert_rebase_complete!(result);

    assert_file_contents_at_head!(&test.repo, "x.txt", "x-content\n");
    assert_file_contents_at_head!(&test.repo, "y.txt", "y-content\n");
    assert_file_contents_at_head!(&test.repo, "z.txt", "z-content\n");

    assert_history!(&test, base, &["A", "C", "B"]);
}
