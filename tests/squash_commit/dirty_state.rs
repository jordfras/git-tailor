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
fn squash_commits_blocked_with_staged_changes() {
    let test = common::TestRepo::new();

    let _base = test.commit_file("a.txt", "base\n", "base");
    let target = test.commit_file("a.txt", "target\n", "target");
    let source = test.commit_file("b.txt", "source\n", "source");

    // Stage a change to an unrelated file
    test.write_file("unrelated.txt", "staged work\n");
    test.stage_file("unrelated.txt");

    let git_repo = test.git_repo();
    let result = git_repo.squash_commits(
        &Oid::from(source),
        &Oid::from(target),
        "combined",
        &Oid::from(source),
    );

    assert!(
        result.is_err(),
        "squash should be blocked when staged changes exist"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("staged or unstaged"),
        "error should mention staged/unstaged: {msg}"
    );
}

#[test]
fn squash_commits_blocked_with_unstaged_changes() {
    let test = common::TestRepo::new();

    let _base = test.commit_file("a.txt", "base\n", "base");
    let target = test.commit_file("a.txt", "target\n", "target");
    let source = test.commit_file("b.txt", "source\n", "source");

    // Modify a tracked file without staging
    test.write_file("a.txt", "unstaged work\n");

    let git_repo = test.git_repo();
    let result = git_repo.squash_commits(
        &Oid::from(source),
        &Oid::from(target),
        "combined",
        &Oid::from(source),
    );

    assert!(
        result.is_err(),
        "squash should be blocked when unstaged changes exist"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("staged or unstaged"),
        "error should mention staged/unstaged: {msg}"
    );
}

#[test]
fn squash_try_combine_blocked_with_staged_changes() {
    let test = common::TestRepo::new();

    let _base = test.commit_file("a.txt", "base\n", "base");
    let target = test.commit_file("a.txt", "target\n", "target");
    let source = test.commit_file("b.txt", "source\n", "source");

    // Stage a change to an unrelated file
    test.write_file("unrelated.txt", "staged work\n");
    test.stage_file("unrelated.txt");

    let git_repo = test.git_repo();
    let result = git_repo.squash_try_combine(
        &Oid::from(source),
        &Oid::from(target),
        "combined",
        false,
        &Oid::from(source),
    );

    assert!(
        result.is_err(),
        "squash_try_combine should be blocked when staged changes exist"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("staged or unstaged"),
        "error should mention staged/unstaged: {msg}"
    );
}

#[test]
fn squash_try_combine_blocked_with_unstaged_changes() {
    let test = common::TestRepo::new();

    let _base = test.commit_file("a.txt", "base\n", "base");
    let target = test.commit_file("a.txt", "target\n", "target");
    let source = test.commit_file("b.txt", "source\n", "source");

    // Modify a tracked file without staging
    test.write_file("a.txt", "unstaged work\n");

    let git_repo = test.git_repo();
    let result = git_repo.squash_try_combine(
        &Oid::from(source),
        &Oid::from(target),
        "combined",
        false,
        &Oid::from(source),
    );

    assert!(
        result.is_err(),
        "squash_try_combine should be blocked when unstaged changes exist"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("staged or unstaged"),
        "error should mention staged/unstaged: {msg}"
    );
}

#[test]
fn squash_commits_allowed_with_staged_submodule() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "base\n", "base");
    let target = test.commit_file("b.txt", "b\n", "target commit");
    let source = test.commit_file("c.txt", "c\n", "source commit");

    // Stage a submodule pointer update — the only dirty state is a gitlink.
    test.stage_gitlink("libs/sub", base);

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
fn squash_try_combine_allowed_with_staged_submodule() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "base\n", "base");
    let target = test.commit_file("b.txt", "b\n", "target commit");
    let source = test.commit_file("c.txt", "c\n", "source commit");

    // Stage a submodule pointer update — the only dirty state is a gitlink.
    test.stage_gitlink("libs/sub", base);

    let git_repo = test.git_repo();
    // Returns Ok(None) when there is no merge conflict — both files differ.
    let result = git_repo
        .squash_try_combine(
            &Oid::from(source),
            &Oid::from(target),
            "squashed",
            false,
            &Oid::from(source),
        )
        .unwrap();

    assert!(
        result.is_none(),
        "squash_try_combine should succeed (no conflict) when only a submodule pointer is staged"
    );
}

/// After aborting a conflicted squash, the working tree must be completely
/// clean — no staged changes, no unstaged changes, and no untracked files
/// (e.g. files written to the workdir during conflict checkout that are not
/// present in HEAD).
#[test]
fn squash_abort_leaves_clean_working_tree() {
    // Scenario: squash a source commit (which adds an extra file b.txt) onto
    // a target commit, producing a modify/modify conflict on a.txt.  The
    // conflict checkout writes b.txt to the workdir.  The original HEAD
    // (_base) has no b.txt, so after abort checkout_head must clean it up.
    //
    //   _base:  a.txt = "base"                      ← original HEAD (head_oid)
    //   target: a.txt = "target"                    ← squash target / onto_commit
    //   _mid:   a.txt = "mid"                       ← source's parent
    //   source: a.txt = "source", b.txt = "new"     ← squash source
    //
    // cherry-pick(source, target): ancestor=_mid, ours=target, theirs=source
    //   a.txt: both changed from "mid" → modify/modify conflict (stages 1,2,3)
    //   b.txt: only theirs added       → stage 0, written to workdir by checkout_index
    //
    // After abort restoring HEAD to _base (no b.txt), checkout_head must
    // remove b.txt from the workdir (requires remove_untracked in the
    // checkout options — the bug is that it was missing).
    let test = common::TestRepo::new();
    let _base = test.commit_file("a.txt", "base\n", "base — will be the original HEAD");
    let target = test.commit_file("a.txt", "target\n", "target modifies a");
    let _mid = test.commit_file("a.txt", "mid\n", "mid — parent of source");
    let source = test.commit_files(
        &[("a.txt", "source\n"), ("b.txt", "new file from source\n")],
        "source modifies a and adds b",
    );

    let git_repo = test.git_repo();

    // Pass _base as head_oid so rebase_abort restores the branch there.
    // _base has only a.txt; b.txt is absent from it.
    let state = git_repo
        .squash_try_combine(
            &Oid::from(source),
            &Oid::from(target),
            "combined",
            false,
            &Oid::from(_base),
        )
        .unwrap()
        .expect("a.txt conflict (modify/modify) should be detected");

    assert!(
        !state.conflicting_files.is_empty(),
        "should report conflicting files"
    );

    // Verify b.txt was actually written to workdir during conflict checkout
    // (this confirms the scenario exercises the code path where a file not
    // in _base ends up in the workdir, so cleanup on abort is necessary).
    let workdir = git_repo.workdir().unwrap();
    assert!(
        workdir.join("b.txt").exists(),
        "b.txt should be present in workdir after conflict checkout (pre-abort)"
    );

    // Abort without resolving anything.
    git_repo.rebase_abort(&state).unwrap();

    // Branch ref is restored to _base.
    let current_head = test.repo.head().unwrap().target().unwrap();
    assert_eq!(
        current_head, _base,
        "HEAD should be restored to _base after abort"
    );

    // No staged changes (index matches HEAD).
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

    // No untracked files (e.g. b.txt written to workdir by conflict checkout).
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
