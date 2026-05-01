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

fn commit_message(repo: &git2::Repository, oid: git2::Oid) -> String {
    repo.find_commit(oid)
        .unwrap()
        .message()
        .unwrap_or("")
        .trim()
        .to_string()
}

#[test]
fn move_commit_blocked_with_staged_changes() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "base\n", "base");
    let a = test.commit_file("a.txt", "a\n", "A");
    let b = test.commit_file("b.txt", "b\n", "B");

    // Stage a change to an unrelated file
    test.write_file("unrelated.txt", "staged work\n");
    test.stage_file("unrelated.txt");

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
    test.write_file("a.txt", "unstaged work\n");

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
    assert_file_contents_at_head!(&test.repo, "a.txt", "root_content\n");
    assert_file_contents_at_head!(&test.repo, "x.txt", "x\n");
    assert_file_contents_at_head!(&test.repo, "y.txt", "y\n");
    assert_file_contents_at_head!(&test.repo, "z.txt", "z\n");

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

    assert_file_contents_at_head!(&test.repo, "root.txt", "root\n");
    assert_file_contents_at_head!(&test.repo, "a.txt", "a\n");
    assert_file_contents_at_head!(&test.repo, "b.txt", "b\n");

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
    test.stage_gitlink("libs/sub", base);

    let git_repo = test.git_repo();
    let result = git_repo
        .move_commit(&Oid::from(c), Some(&Oid::from(a)), &Oid::from(c))
        .unwrap();

    assert_rebase_complete!(result);
}
