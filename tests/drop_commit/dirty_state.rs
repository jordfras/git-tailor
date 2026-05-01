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
fn drop_commit_blocked_with_staged_changes() {
    let test = common::TestRepo::new();

    let _base = test.commit_file("a.txt", "v1\n", "base");
    let to_drop = test.commit_file("b.txt", "content\n", "to drop");

    // Stage a change to an unrelated file
    test.write_file("unrelated.txt", "staged work\n");
    test.stage_file("unrelated.txt");

    let git_repo = test.git_repo();
    let result = git_repo.drop_commit(&Oid::from(to_drop), &Oid::from(to_drop));

    assert!(
        result.is_err(),
        "drop should be blocked when staged changes exist"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("staged or unstaged"),
        "error should mention staged/unstaged: {msg}"
    );
}

#[test]
fn drop_commit_blocked_with_unstaged_changes() {
    let test = common::TestRepo::new();

    let _base = test.commit_file("a.txt", "v1\n", "base");
    let to_drop = test.commit_file("b.txt", "content\n", "to drop");

    // Modify a tracked file without staging
    test.write_file("a.txt", "unstaged work\n");

    let git_repo = test.git_repo();
    let result = git_repo.drop_commit(&Oid::from(to_drop), &Oid::from(to_drop));

    assert!(
        result.is_err(),
        "drop should be blocked when unstaged changes exist"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("staged or unstaged"),
        "error should mention staged/unstaged: {msg}"
    );
}

#[test]
fn drop_commit_allowed_with_staged_submodule() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "v1\n", "base");
    let to_drop = test.commit_file("a.txt", "v2\n", "to drop");

    // Stage a submodule pointer update — the only dirty state is a gitlink.
    test.stage_gitlink("libs/sub", base);

    let git_repo = test.git_repo();
    let result = git_repo
        .drop_commit(&Oid::from(to_drop), &Oid::from(to_drop))
        .unwrap();

    assert_rebase_complete!(result);
}

/// After aborting a conflicted operation, the working tree must be completely
/// clean — no staged changes, no unstaged changes, and no untracked files left
/// behind by the conflict checkout.
#[test]
fn rebase_abort_leaves_clean_working_tree() {
    // Scenario: drop a commit whose descendant modifies a file that the
    // dropped commit also touched. This creates a conflict where the
    // working tree is dirtied (conflict markers written).
    let test = common::TestRepo::new();

    let _base = test.commit_file("a.txt", "base\n", "base");
    let to_drop = test.commit_file("a.txt", "base\ndropped\n", "add dropped line");
    let head = test.commit_file("a.txt", "base\ndropped\nhead\n", "add head line");

    let git_repo = test.git_repo();
    let result = git_repo
        .drop_commit(&Oid::from(to_drop), &Oid::from(head))
        .unwrap();

    let state = expect_rebase_conflict!(result);

    // Abort — must restore branch and leave a clean working tree.
    git_repo.rebase_abort(&state).unwrap();

    // Branch ref is restored.
    let current_head = test.repo.head().unwrap().target().unwrap();
    assert_eq!(current_head, head, "HEAD should be restored after abort");

    // No staged changes.
    let head_tree = test.repo.head().unwrap().peel_to_tree().unwrap();
    let staged_diff = test
        .repo
        .diff_tree_to_index(Some(&head_tree), None, None)
        .unwrap();
    assert_eq!(
        staged_diff.deltas().len(),
        0,
        "no staged changes should remain after abort: {:?}",
        staged_diff
            .deltas()
            .map(|d| d.new_file().path().unwrap().display().to_string())
            .collect::<Vec<_>>()
    );

    // No unstaged changes.
    let unstaged_diff = test.repo.diff_index_to_workdir(None, None).unwrap();
    assert_eq!(
        unstaged_diff.deltas().len(),
        0,
        "no unstaged changes should remain after abort: {:?}",
        unstaged_diff
            .deltas()
            .map(|d| d.new_file().path().unwrap().display().to_string())
            .collect::<Vec<_>>()
    );

    // No untracked files left behind by the conflict checkout.
    let mut status_opts = git2::StatusOptions::new();
    status_opts.include_untracked(true);
    status_opts.recurse_untracked_dirs(true);
    let statuses = test.repo.statuses(Some(&mut status_opts)).unwrap();
    let untracked: Vec<_> = statuses
        .iter()
        .filter(|e| e.status().contains(git2::Status::WT_NEW))
        .map(|e| e.path().unwrap_or("?").to_string())
        .collect();
    assert!(
        untracked.is_empty(),
        "no untracked files should remain after abort: {untracked:?}"
    );
}
