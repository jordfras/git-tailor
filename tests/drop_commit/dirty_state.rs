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

    let mut git_repo = test.git_repo();
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

    let mut git_repo = test.git_repo();
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

    let mut git_repo = test.git_repo();
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

    let mut git_repo = test.git_repo();
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

/// An untracked file must not be silently overwritten when the operation
/// reintroduces a path at the same name.
///
/// The dirty guard only counts staged and unstaged diffs, so a working tree
/// holding nothing but an untracked file reads as clean and the operation
/// proceeds. Dropping the commit that deleted `notes.txt` brings the tracked
/// version back, and the force checkout that ends the rebase writes it straight
/// over the user's own file.
#[test]
fn drop_commit_does_not_clobber_a_colliding_untracked_file() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "v1\n", "base");
    test.commit_file("notes.txt", "from history\n", "add notes");
    let deletion = test.delete_file("notes.txt", "delete notes");
    let head = test.commit_file("c.txt", "v1\n", "later work");

    // Untracked: HEAD no longer tracks this path, so nothing is staged or
    // unstaged and the working tree reads as clean.
    test.write_file("notes.txt", "my local scratch\n");

    let mut git_repo = test.git_repo();
    let result = git_repo.drop_commit(&Oid::from(deletion), &Oid::from(head));

    let workdir = test.repo.workdir().unwrap().to_path_buf();
    let on_disk = std::fs::read_to_string(workdir.join("notes.txt")).unwrap_or_default();
    assert_eq!(
        on_disk, "my local scratch\n",
        "the untracked file's contents must survive the drop (result: {result:?})"
    );
    let _ = base;
}

/// `--autostash` does not rescue the colliding untracked file either, because
/// the stash is only taken when the tree is *dirty* — and untracked files do
/// not count towards that. The flag is no answer to this.
#[test]
fn autostash_does_not_rescue_a_colliding_untracked_file() {
    let test = common::TestRepo::new();

    test.commit_file("a.txt", "v1\n", "base");
    test.commit_file("notes.txt", "from history\n", "add notes");
    let deletion = test.delete_file("notes.txt", "delete notes");
    let head = test.commit_file("c.txt", "v1\n", "later work");

    test.write_file("notes.txt", "my local scratch\n");

    let mut git_repo = test.git_repo();
    git_repo.set_autostash(true);
    git_repo.autostash_save().unwrap();
    let result = git_repo.drop_commit(&Oid::from(deletion), &Oid::from(head));
    let _ = git_repo.autostash_restore();

    let workdir = test.repo.workdir().unwrap().to_path_buf();
    let on_disk = std::fs::read_to_string(workdir.join("notes.txt")).unwrap_or_default();
    assert_eq!(
        on_disk, "my local scratch\n",
        "the untracked file's contents must survive with --autostash too (result: {result:?})"
    );
}

/// The collision must be caught when auto-stash fires too.
///
/// Sweeping the untracked file into the stash only defers the clash: the drop
/// reintroduces the path, the reapply merges the stashed copy onto it, and the
/// user is left with conflict markers. Worse, the markers arrive with no
/// unmerged index entry, so the stage-based conflict check does not see them
/// and the restore reports success — the user is told it worked.
///
/// Whichever way the stash behaves, the file must be left as the user wrote it
/// and never quietly rewritten into a merge.
#[test]
fn a_colliding_untracked_file_survives_when_autostash_fires() {
    let test = common::TestRepo::new();

    test.commit_file("a.txt", "v1\n", "base");
    test.commit_file("notes.txt", "from history\n", "add notes");
    let deletion = test.delete_file("notes.txt", "delete notes");
    let head = test.commit_file("c.txt", "v1\n", "later work");

    // Other dirt, so the stash is actually taken.
    test.write_file("a.txt", "v1\nedited\n");
    test.write_file("notes.txt", "my local scratch\n");

    let mut git_repo = test.git_repo();
    git_repo.set_autostash(true);
    git_repo.autostash_save().unwrap();
    let dropped = git_repo.drop_commit(&Oid::from(deletion), &Oid::from(head));
    let restored = git_repo.autostash_restore();

    let workdir = test.repo.workdir().unwrap().to_path_buf();
    assert!(
        dropped.is_err(),
        "the collision must be refused, not deferred into the reapply"
    );
    let on_disk = std::fs::read_to_string(workdir.join("notes.txt")).unwrap_or_default();
    assert!(
        !on_disk.contains("<<<<<<<"),
        "the file must not be silently rewritten into a merge \
         (drop: {dropped:?}, restore: {restored:?}): {on_disk:?}"
    );
    assert_eq!(
        on_disk, "my local scratch\n",
        "the user's content must survive (drop: {dropped:?}, restore: {restored:?})"
    );
}

/// Staging the colliding file is already safe: it is then a real index entry,
/// so the dirty guard sees it and refuses before anything is rewritten. Pinned
/// so the untracked check above is never widened into taking this path over.
#[test]
fn a_staged_file_at_a_reintroduced_path_is_refused_by_the_dirty_guard() {
    let test = common::TestRepo::new();

    test.commit_file("a.txt", "v1\n", "base");
    test.commit_file("notes.txt", "from history\n", "add notes");
    let deletion = test.delete_file("notes.txt", "delete notes");
    let head = test.commit_file("c.txt", "v1\n", "later work");

    test.write_file("notes.txt", "my local scratch\n");
    test.stage_file("notes.txt");

    let mut git_repo = test.git_repo();
    let result = git_repo.drop_commit(&Oid::from(deletion), &Oid::from(head));

    assert!(result.is_err(), "a staged collision must be refused");
    let workdir = test.repo.workdir().unwrap().to_path_buf();
    assert_eq!(
        std::fs::read_to_string(workdir.join("notes.txt")).unwrap(),
        "my local scratch\n"
    );
}

/// Aborting must clean up after the conflict, not after the user.
///
/// The abort checkout passes `remove_untracked`, which libgit2 does not scope
/// to the files the conflict wrote — it takes every untracked file in the
/// checkout's path scope. A scratch file that has nothing to do with the
/// conflict is deleted outright, and nothing says so.
#[test]
fn rebase_abort_keeps_the_users_own_untracked_files() {
    let test = common::TestRepo::new();
    let _base = test.commit_file("a.txt", "base\n", "base");
    let to_drop = test.commit_file("a.txt", "base\ndropped\n", "add dropped line");
    let head = test.commit_file("a.txt", "base\ndropped\nhead\n", "add head line");

    let mut git_repo = test.git_repo();
    let state = expect_rebase_conflict!(
        git_repo
            .drop_commit(&Oid::from(to_drop), &Oid::from(head))
            .unwrap()
    );

    // Written while the operation is paused, unrelated to the conflict.
    test.write_file("my-notes.txt", "important\n");

    git_repo.rebase_abort(&state).unwrap();

    let workdir = test.repo.workdir().unwrap().to_path_buf();
    assert_eq!(
        std::fs::read_to_string(workdir.join("my-notes.txt")).unwrap_or_default(),
        "important\n",
        "an abort must not delete the user's own untracked files"
    );
}

/// Every colliding path is named, not just the first one the diff happens to
/// reach — the user needs the whole list to clear it in one go.
#[test]
fn a_collision_names_every_untracked_file_it_would_overwrite() {
    let test = common::TestRepo::new();

    test.commit_file("a.txt", "v1\n", "base");
    test.commit_files(
        &[("notes.txt", "from history\n"), ("todo.txt", "also\n")],
        "add notes and todo",
    );
    // One commit removing both, so dropping it brings both back together.
    let deletion = test.delete_files(&["notes.txt", "todo.txt"], "delete both");
    let head = test.commit_file("c.txt", "v1\n", "later work");

    test.write_file("notes.txt", "my notes\n");
    test.write_file("todo.txt", "my todo\n");

    let mut git_repo = test.git_repo();
    let err = git_repo
        .drop_commit(&Oid::from(deletion), &Oid::from(head))
        .unwrap_err()
        .to_string();

    assert!(err.contains("notes.txt"), "should name notes.txt: {err}");
    assert!(err.contains("todo.txt"), "should name todo.txt: {err}");
}

/// A symlink counts as a file: git tracked the path as one, and following the
/// link would judge it by whatever it points at instead.
#[test]
fn an_untracked_symlink_at_a_reintroduced_path_is_refused() {
    let test = common::TestRepo::new();

    test.commit_file("a.txt", "v1\n", "base");
    test.commit_file("notes.txt", "from history\n", "add notes");
    let deletion = test.delete_file("notes.txt", "delete notes");
    let head = test.commit_file("c.txt", "v1\n", "later work");

    let workdir = test.repo.workdir().unwrap().to_path_buf();
    test.write_file("target.txt", "pointed at\n");
    std::os::unix::fs::symlink("target.txt", workdir.join("notes.txt")).unwrap();

    let mut git_repo = test.git_repo();
    let result = git_repo.drop_commit(&Oid::from(deletion), &Oid::from(head));

    assert!(result.is_err(), "a symlink in the way must be refused");
    assert!(
        workdir
            .join("notes.txt")
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink(),
        "the symlink itself must be left alone"
    );
}

/// The content check hashes the file as it sits on disk, without checkout
/// filters. With `core.autocrlf` a file that *would* match after filtering
/// hashes differently and is reported as a collision.
///
/// That is the conservative direction and the one to keep: the user is asked
/// about a file rather than quietly relieved of it. Pinned here so the
/// behaviour is a decision rather than a surprise.
#[test]
fn a_crlf_file_matching_only_after_filtering_is_still_refused() {
    let test = common::TestRepo::new();
    test.set_config("core.autocrlf", "true");

    test.commit_file("a.txt", "v1\n", "base");
    test.commit_file("notes.txt", "line one\n", "add notes");
    let deletion = test.delete_file("notes.txt", "delete notes");
    let head = test.commit_file("c.txt", "v1\n", "later work");

    let workdir = test.repo.workdir().unwrap().to_path_buf();
    std::fs::write(workdir.join("notes.txt"), "line one\r\n").unwrap();

    let mut git_repo = test.git_repo();
    let result = git_repo.drop_commit(&Oid::from(deletion), &Oid::from(head));

    assert!(
        result.is_err(),
        "hashing without filters must err towards refusing"
    );
}
