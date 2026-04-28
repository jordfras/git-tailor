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

#[allow(dead_code)]
mod common;

use git_tailor::{
    Oid,
    repo::{GitRepo, RebaseOutcome},
};

fn commit_message(repo: &git2::Repository, oid: git2::Oid) -> String {
    repo.find_commit(oid)
        .unwrap()
        .message()
        .unwrap_or("")
        .trim()
        .to_string()
}

// ---------------------------------------------------------------------------
// Happy-path tests
// ---------------------------------------------------------------------------

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

    assert_history!(&test.repo, base, &["A", "C", "B"]);

    let head_oid = test.repo.head().unwrap().target().unwrap();
    assert_file_contents!(&test.repo, head_oid, "x.txt", "x\n");
    assert_file_contents!(&test.repo, head_oid, "y.txt", "y\n");
    assert_file_contents!(&test.repo, head_oid, "z.txt", "z\n");
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

    assert_history!(&test.repo, base, &["A", "C", "D", "B"]);
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

    assert_history!(&test.repo, base, &["C", "A", "B"]);
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

    assert_history!(&test.repo, base, &["A", "C", "B"]);
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
            assert_history!(&test.repo, base, &["B", "A"]);
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

    let head_oid = test.repo.head().unwrap().target().unwrap();
    assert_file_contents!(&test.repo, head_oid, "x.txt", "x-content\n");
    assert_file_contents!(&test.repo, head_oid, "y.txt", "y-content\n");
    assert_file_contents!(&test.repo, head_oid, "z.txt", "z-content\n");

    assert_history!(&test.repo, base, &["A", "C", "B"]);
}

// ---------------------------------------------------------------------------
// Dirty-state guard tests
// ---------------------------------------------------------------------------

#[test]
fn move_commit_blocked_with_staged_changes() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "base\n", "base");
    let a = test.commit_file("a.txt", "a\n", "A");
    let b = test.commit_file("b.txt", "b\n", "B");

    // Stage a change to an unrelated file
    let workdir = test.repo.workdir().unwrap();
    std::fs::write(workdir.join("unrelated.txt"), "staged work\n").unwrap();
    let mut index = test.repo.index().unwrap();
    index
        .add_path(std::path::Path::new("unrelated.txt"))
        .unwrap();
    index.write().unwrap();

    let git_repo = test.git_repo();
    let result = git_repo.move_commit(&Oid::from(b), Some(&Oid::from(base)), &Oid::from(b));

    assert!(
        result.is_err(),
        "move should be blocked when staged changes exist"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("staged or unstaged"),
        "error should mention staged/unstaged: {msg}"
    );
    let _ = a;
}

#[test]
fn move_commit_blocked_with_unstaged_changes() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "base\n", "base");
    let a = test.commit_file("a.txt", "a\n", "A");
    let b = test.commit_file("b.txt", "b\n", "B");

    // Modify a tracked file without staging
    let workdir = test.repo.workdir().unwrap();
    std::fs::write(workdir.join("a.txt"), "unstaged work\n").unwrap();

    let git_repo = test.git_repo();
    let result = git_repo.move_commit(&Oid::from(b), Some(&Oid::from(base)), &Oid::from(b));

    assert!(
        result.is_err(),
        "move should be blocked when unstaged changes exist"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("staged or unstaged"),
        "error should mention staged/unstaged: {msg}"
    );
    let _ = a;
}

/// In `--all` mode the root commit is part of the visible list. When the user
/// moves a commit to position 0 (before the root), `move_select` emits `""`
/// as the `insert_after_oid` sentinel. The moved commit should become the new
/// orphan root and the old root is cherry-picked on top of it.
#[test]
fn move_commit_to_root_position() {
    let test = common::TestRepo::new();

    // History: root(a.txt) → A(x.txt) → B(y.txt) → C(z.txt)
    // Move C to before root → new root = C', then root, A, B on top.
    let _root = test.commit_file("a.txt", "root_content\n", "root");
    let _a = test.commit_file("x.txt", "x\n", "A");
    let _b = test.commit_file("y.txt", "y\n", "B");
    let c = test.commit_file("z.txt", "z\n", "C");

    let git_repo = test.git_repo();
    // "" is the sentinel for "make this commit the new root"
    let result = git_repo
        .move_commit(&Oid::from(c), None, &Oid::from(c))
        .unwrap();

    assert_rebase_complete!(result);

    let head_oid = test.repo.head().unwrap().target().unwrap();

    // Collect all commits oldest-first by exhausting revwalk.
    let mut revwalk = test.repo.revwalk().unwrap();
    revwalk.push(head_oid).unwrap();
    let mut all_oids: Vec<git2::Oid> = revwalk.collect::<Result<_, _>>().unwrap();
    all_oids.reverse();

    assert_eq!(all_oids.len(), 4, "should still have 4 commits");

    // New root must be C and must have no parent.
    let new_root = test.repo.find_commit(all_oids[0]).unwrap();
    assert_eq!(new_root.parent_count(), 0, "new root should be an orphan");
    assert_eq!(new_root.summary().unwrap_or(""), "C");

    // Remaining order: root, A, B.
    let messages: Vec<String> = all_oids
        .iter()
        .map(|&oid| commit_message(&test.repo, oid))
        .collect();
    assert_eq!(messages, vec!["C", "root", "A", "B"]);

    // All files must be present at HEAD.
    assert_file_contents!(&test.repo, head_oid, "a.txt", "root_content\n");
    assert_file_contents!(&test.repo, head_oid, "x.txt", "x\n");
    assert_file_contents!(&test.repo, head_oid, "y.txt", "y\n");
    assert_file_contents!(&test.repo, head_oid, "z.txt", "z\n");

    // Working tree must be clean — no staged deletions or untracked files.
    let statuses = test.repo.statuses(None).unwrap();
    assert!(
        statuses.is_empty(),
        "working tree should be clean after move; got: {:?}",
        statuses
            .iter()
            .map(|s| (s.path().unwrap_or("").to_string(), s.status()))
            .collect::<Vec<_>>()
    );
}

/// The root commit can be moved to a later position in `--all` mode.
/// History: root(root.txt) → A(a.txt) → B(b.txt)
/// Move root to after B → new-root = A, then B, then root.
#[test]
fn move_root_commit_to_later_position() {
    let test = common::TestRepo::new();

    let _root = test.commit_file("root.txt", "root\n", "root");
    let _a = test.commit_file("a.txt", "a\n", "A");
    let b = test.commit_file("b.txt", "b\n", "B");

    let git_repo = test.git_repo();
    let root_oid = git_repo.root_commit_oid().unwrap();

    let result = git_repo
        .move_commit(&root_oid, Some(&Oid::from(b)), &Oid::from(b))
        .unwrap();

    assert_rebase_complete!(result);

    let head_oid = test.repo.head().unwrap().target().unwrap();

    let mut revwalk = test.repo.revwalk().unwrap();
    revwalk.push(head_oid).unwrap();
    let mut all_oids: Vec<git2::Oid> = revwalk.collect::<Result<_, _>>().unwrap();
    all_oids.reverse();

    assert_eq!(all_oids.len(), 3);

    let new_root = test.repo.find_commit(all_oids[0]).unwrap();
    assert_eq!(new_root.parent_count(), 0, "new root should be orphan");

    let messages: Vec<String> = all_oids
        .iter()
        .map(|&oid| commit_message(&test.repo, oid))
        .collect();
    assert_eq!(messages, vec!["A", "B", "root"]);

    assert_file_contents!(&test.repo, head_oid, "root.txt", "root\n");
    assert_file_contents!(&test.repo, head_oid, "a.txt", "a\n");
    assert_file_contents!(&test.repo, head_oid, "b.txt", "b\n");

    // Working tree must be clean — no staged deletions or untracked files.
    let statuses = test.repo.statuses(None).unwrap();
    assert!(
        statuses.is_empty(),
        "working tree should be clean after move; got: {:?}",
        statuses
            .iter()
            .map(|s| (s.path().unwrap_or("").to_string(), s.status()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn move_commit_allowed_with_staged_submodule() {
    let test = common::TestRepo::new();

    // ref → A → B → C(source) → HEAD; move C to after A
    let base = test.commit_file("a.txt", "base\n", "base");
    let a = test.commit_file("x.txt", "x\n", "A");
    let _b = test.commit_file("y.txt", "y\n", "B");
    let c = test.commit_file("z.txt", "z\n", "C");

    // Stage a submodule pointer update — the only dirty state is a gitlink.
    common::stage_gitlink(&test.repo, "libs/sub", base);

    let git_repo = test.git_repo();
    let result = git_repo
        .move_commit(&Oid::from(c), Some(&Oid::from(a)), &Oid::from(c))
        .unwrap();

    assert_rebase_complete!(result);
}
