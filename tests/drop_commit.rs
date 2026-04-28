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

// ---------------------------------------------------------------------------
// Happy-path tests
// ---------------------------------------------------------------------------

#[test]
fn drop_head_commit_removes_it() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "v1\n", "base");
    let _middle = test.commit_file("a.txt", "v2\n", "middle");
    let to_drop = test.commit_file("a.txt", "v3\n", "to drop");

    let git_repo = test.git_repo();
    let result = git_repo
        .drop_commit(&Oid::from(to_drop), &Oid::from(to_drop))
        .unwrap();

    assert_rebase_complete!(result);

    let commits = common::commits_from_head(&test.repo, base);
    assert_eq!(commits.len(), 1, "should have 1 commit above base");

    let head_oid = test.repo.head().unwrap().target().unwrap();
    assert_eq!(
        common::file_content_at(&test.repo, head_oid, "a.txt"),
        "v2\n",
        "HEAD should have the middle commit's content"
    );
}

#[test]
fn drop_middle_commit_rebases_descendants() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "base\n", "base");
    let to_drop = test.commit_file("b.txt", "added\n", "add b.txt");
    let child = test.commit_file("a.txt", "changed\n", "modify a.txt");

    let git_repo = test.git_repo();
    let result = git_repo
        .drop_commit(&Oid::from(to_drop), &Oid::from(child))
        .unwrap();

    assert_rebase_complete!(result);

    let commits = common::commits_from_head(&test.repo, base);
    assert_eq!(
        commits.len(),
        1,
        "should have 1 commit above base (dropped middle)"
    );

    let head_oid = test.repo.head().unwrap().target().unwrap();
    assert_eq!(
        common::file_content_at(&test.repo, head_oid, "a.txt"),
        "changed\n",
        "descendant's change to a.txt should survive"
    );

    // b.txt should not exist in the final tree since the commit that added it
    // was dropped.
    let head_commit = test.repo.find_commit(head_oid).unwrap();
    let tree = head_commit.tree().unwrap();
    assert!(
        tree.get_path(std::path::Path::new("b.txt")).is_err(),
        "b.txt should not exist after dropping the commit that added it"
    );
}

#[test]
fn drop_with_multiple_descendants() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "base\n", "base");
    let to_drop = test.commit_file("b.txt", "added\n", "add b.txt");
    let _child1 = test.commit_file("c.txt", "c1\n", "add c.txt");
    let head = test.commit_file("d.txt", "d1\n", "add d.txt");

    let git_repo = test.git_repo();
    let result = git_repo
        .drop_commit(&Oid::from(to_drop), &Oid::from(head))
        .unwrap();

    assert_rebase_complete!(result);

    let commits = common::commits_from_head(&test.repo, base);
    assert_eq!(
        commits.len(),
        2,
        "should have 2 commits above base after dropping middle"
    );

    let head_oid = test.repo.head().unwrap().target().unwrap();
    let head_tree = test.repo.find_commit(head_oid).unwrap().tree().unwrap();
    assert!(head_tree.get_path(std::path::Path::new("c.txt")).is_ok());
    assert!(head_tree.get_path(std::path::Path::new("d.txt")).is_ok());
    assert!(head_tree.get_path(std::path::Path::new("b.txt")).is_err());
}

#[test]
fn drop_preserves_commit_messages() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "base\n", "base");
    let to_drop = test.commit_file("b.txt", "added\n", "add b.txt");
    let _child = test.commit_file("c.txt", "c1\n", "important change");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();
    git_repo
        .drop_commit(&Oid::from(to_drop), &head_oid)
        .unwrap();

    let commits = common::commits_from_head(&test.repo, base);
    assert_eq!(commits.len(), 1);

    let rebased = test.repo.find_commit(commits[0]).unwrap();
    assert_eq!(
        rebased.message().unwrap(),
        "important change",
        "rebased descendant should keep its original message"
    );
}

// ---------------------------------------------------------------------------
// Conflict tests
// ---------------------------------------------------------------------------

#[test]
fn drop_returns_conflict_when_descendant_depends_on_dropped_commit() {
    let test = common::TestRepo::new();

    let _base = test.commit_file("a.txt", "line1\n", "base");
    let to_drop = test.commit_file("a.txt", "line1\nline2\n", "add line2");
    // This commit modifies the same area that to_drop introduced, so dropping
    // to_drop will conflict when rebasing this descendant.
    let head = test.commit_file("a.txt", "line1\nline2\nline3\n", "add line3");

    let git_repo = test.git_repo();
    let result = git_repo
        .drop_commit(&Oid::from(to_drop), &Oid::from(head))
        .unwrap();

    match result {
        RebaseOutcome::Conflict(state) => {
            assert_eq!(state.conflicting_commit_oid, Oid::from(head));
            assert!(
                state.remaining_oids.is_empty(),
                "no commits after the conflicting one"
            );
            assert_eq!(state.original_branch_oid, Oid::from(head));
        }
        RebaseOutcome::Complete => panic!("expected Conflict, got Complete"),
    }
}

#[test]
fn drop_conflict_state_has_correct_remaining_oids() {
    let test = common::TestRepo::new();

    let _base = test.commit_file("a.txt", "v1\n", "base");
    let to_drop = test.commit_file("a.txt", "v2\n", "change a");
    // First descendant will conflict (depends on dropped change).
    let child1 = test.commit_file("a.txt", "v3\n", "change a again");
    let child2 = test.commit_file("b.txt", "b1\n", "add b");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();
    let result = git_repo
        .drop_commit(&Oid::from(to_drop), &head_oid)
        .unwrap();

    match result {
        RebaseOutcome::Conflict(state) => {
            assert_eq!(state.conflicting_commit_oid, Oid::from(child1));
            assert_eq!(state.remaining_oids, vec![Oid::from(child2)]);
        }
        RebaseOutcome::Complete => panic!("expected Conflict, got Complete"),
    }
}

// ---------------------------------------------------------------------------
// Continue / abort tests
// ---------------------------------------------------------------------------

#[test]
fn drop_continue_after_resolving_conflict() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "line1\n", "base");
    let to_drop = test.commit_file("a.txt", "line1\nline2\n", "add line2");
    let head = test.commit_file("a.txt", "line1\nline2\nline3\n", "add line3");

    let git_repo = test.git_repo();
    let result = git_repo
        .drop_commit(&Oid::from(to_drop), &Oid::from(head))
        .unwrap();

    let state = expect_rebase_conflict!(result);

    // Simulate user resolving the conflict: write the resolved content,
    // clear conflict entries, then stage the file.
    let workdir = test.repo.workdir().unwrap();
    std::fs::write(workdir.join("a.txt"), "line1\nline3\n").unwrap();
    let mut index = test.repo.index().unwrap();
    index
        .conflict_remove(std::path::Path::new("a.txt"))
        .unwrap();
    index.add_path(std::path::Path::new("a.txt")).unwrap();
    index.write().unwrap();

    let result = git_repo.rebase_continue(&state).unwrap();
    assert_rebase_complete!(result);

    let commits = common::commits_from_head(&test.repo, base);
    assert_eq!(commits.len(), 1);

    let head_oid = test.repo.head().unwrap().target().unwrap();
    assert_eq!(
        common::file_content_at(&test.repo, head_oid, "a.txt"),
        "line1\nline3\n"
    );
}

#[test]
fn drop_continue_with_unresolved_conflicts_stays_in_conflict_mode() {
    // Calling rebase_continue when the index still has conflict markers
    // must return Conflict (same OIDs, refreshed file list) instead of an
    // error — leaving the repo in a usable state so the user can keep
    // editing or abort.
    let test = common::TestRepo::new();

    let _base = test.commit_file("a.txt", "line1\n", "base");
    let to_drop = test.commit_file("a.txt", "line1\nline2\n", "add line2");
    let head = test.commit_file("a.txt", "line1\nline2\nline3\n", "add line3");

    let git_repo = test.git_repo();
    let result = git_repo
        .drop_commit(&Oid::from(to_drop), &Oid::from(head))
        .unwrap();

    let state = expect_rebase_conflict!(result);

    // Do NOT resolve the conflict — just call continue immediately.
    let result = git_repo.rebase_continue(&state).unwrap();

    match result {
        RebaseOutcome::Conflict(new_state) => {
            // Same commit still conflicting.
            assert_eq!(
                new_state.conflicting_commit_oid,
                state.conflicting_commit_oid
            );
            assert_eq!(new_state.original_branch_oid, state.original_branch_oid);
            assert_eq!(new_state.remaining_oids, state.remaining_oids);
            // File list must show the still-unresolved file.
            assert!(
                !new_state.conflicting_files.is_empty(),
                "conflicting_files should be populated"
            );
            assert!(
                new_state.conflicting_files.iter().any(|f| f == "a.txt"),
                "a.txt should be listed as conflicting, got {:?}",
                new_state.conflicting_files
            );
            // The flag must be set so the dialog can warn the user.
            assert!(
                new_state.still_unresolved,
                "still_unresolved should be true when continuing with unresolved conflicts"
            );
        }
        RebaseOutcome::Complete => panic!("expected Conflict since index was not resolved"),
    }

    // Repo must still be in a state where abort works cleanly.
    git_repo.rebase_abort(&state).unwrap();
    let restored = test.repo.head().unwrap().target().unwrap();
    assert_eq!(restored, head, "abort should restore original HEAD");
}

#[test]
fn drop_abort_restores_original_branch() {
    let test = common::TestRepo::new();

    let _base = test.commit_file("a.txt", "line1\n", "base");
    let to_drop = test.commit_file("a.txt", "line1\nline2\n", "add line2");
    let head = test.commit_file("a.txt", "line1\nline2\nline3\n", "add line3");

    let git_repo = test.git_repo();
    let result = git_repo
        .drop_commit(&Oid::from(to_drop), &Oid::from(head))
        .unwrap();

    let state = expect_rebase_conflict!(result);

    git_repo.rebase_abort(&state).unwrap();

    // Branch should be back to the original HEAD.
    let current_head = test.repo.head().unwrap().target().unwrap();
    assert_eq!(
        current_head, head,
        "HEAD should be restored to original after abort"
    );

    assert_eq!(
        common::file_content_at(&test.repo, current_head, "a.txt"),
        "line1\nline2\nline3\n",
        "working tree content should match original HEAD"
    );
}

// ---------------------------------------------------------------------------
// Error cases
// ---------------------------------------------------------------------------

#[test]
fn drop_abort_after_second_conflict_restores_branch() {
    // Regression test: abort must work even when it is triggered after a
    // second (or later) conflict, not just the first one.
    //
    // Layout (oldest→newest): base → to_drop → child1 → child2
    //
    // to_drop edits a.txt.  child1 and child2 both edit a.txt too, so
    // cherry-picking child1 onto base conflicts, and after a fake-resolve
    // cherry-picking child2 onto that conflicts again.  Aborting at that
    // second conflict must restore HEAD to the original tip (child2).
    let test = common::TestRepo::new();

    let _base = test.commit_file("a.txt", "v1\n", "base");
    let to_drop = test.commit_file("a.txt", "v2\n", "change a (will be dropped)");
    let child1 = test.commit_file("a.txt", "v3\n", "change a again");
    let child2 = test.commit_file("a.txt", "v4\n", "change a a third time");

    let git_repo = test.git_repo();
    let head_oid_before = git_repo.head_oid().unwrap();
    assert_eq!(head_oid_before, Oid::from(child2));

    // First drop → first conflict on child1.
    let result = git_repo
        .drop_commit(&Oid::from(to_drop), &Oid::from(child2))
        .unwrap();
    let state1 = expect_rebase_conflict!(result);
    assert_eq!(state1.conflicting_commit_oid, Oid::from(child1));
    assert_eq!(state1.original_branch_oid, Oid::from(child2));

    // Fake-resolve conflict 1: write content and re-stage.
    let workdir = test.repo.workdir().unwrap();
    std::fs::write(workdir.join("a.txt"), "v1\nv3\n").unwrap();
    let mut index = test.repo.index().unwrap();
    index
        .conflict_remove(std::path::Path::new("a.txt"))
        .unwrap();
    index.add_path(std::path::Path::new("a.txt")).unwrap();
    index.write().unwrap();

    // Continue → second conflict on child2.
    let result = git_repo.rebase_continue(&state1).unwrap();
    let state2 = expect_rebase_conflict!(result);
    assert_eq!(state2.conflicting_commit_oid, Oid::from(child2));
    // original_branch_oid must still refer to the pre-drop HEAD.
    assert_eq!(state2.original_branch_oid, Oid::from(child2));

    // Abort from the second conflict — must fully restore the branch.
    git_repo.rebase_abort(&state2).unwrap();

    let head_after = test.repo.head().unwrap().target().unwrap();
    assert_eq!(
        head_after, child2,
        "HEAD must be fully restored after abort at second conflict"
    );
    assert_eq!(
        common::file_content_at(&test.repo, head_after, "a.txt"),
        "v4\n",
        "file content must match original HEAD after abort"
    );
}

#[test]
fn drop_root_commit_fails() {
    let test = common::TestRepo::new();

    let root = test.commit_file("a.txt", "v1\n", "root");

    let git_repo = test.git_repo();
    let result = git_repo.drop_commit(&Oid::from(root), &Oid::from(root));

    assert!(result.is_err(), "dropping a root commit should fail");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("merge or root"),
        "error should mention merge or root: {msg}"
    );
}

#[test]
fn drop_commit_with_no_descendants() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "v1\n", "base");
    let to_drop = test.commit_file("b.txt", "added\n", "add b");

    let git_repo = test.git_repo();
    let result = git_repo
        .drop_commit(&Oid::from(to_drop), &Oid::from(to_drop))
        .unwrap();

    assert_rebase_complete!(result);

    let head_oid = test.repo.head().unwrap().target().unwrap();
    assert_eq!(
        head_oid, base,
        "HEAD should point to base after dropping the only commit above it"
    );
}

// ---------------------------------------------------------------------------
// Dirty-state guard tests
// ---------------------------------------------------------------------------

#[test]
fn drop_commit_blocked_with_staged_changes() {
    let test = common::TestRepo::new();

    let _base = test.commit_file("a.txt", "v1\n", "base");
    let to_drop = test.commit_file("b.txt", "content\n", "to drop");

    // Stage a change to an unrelated file
    let workdir = test.repo.workdir().unwrap();
    std::fs::write(workdir.join("unrelated.txt"), "staged work\n").unwrap();
    let mut index = test.repo.index().unwrap();
    index
        .add_path(std::path::Path::new("unrelated.txt"))
        .unwrap();
    index.write().unwrap();

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
    let workdir = test.repo.workdir().unwrap();
    std::fs::write(workdir.join("a.txt"), "unstaged work\n").unwrap();

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
    common::stage_gitlink(&test.repo, "libs/sub", base);

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

// ---------------------------------------------------------------------------
// Auto-stage resolved conflicts (T132)
// ---------------------------------------------------------------------------

#[test]
fn auto_stage_resolved_conflicts_stages_externally_edited_file() {
    // Simulates the user resolving a conflict in an external editor (e.g.
    // VS Code) and saving the file without explicitly staging it. The
    // auto_stage_resolved_conflicts method should detect the resolution
    // and stage the file so that rebase_continue succeeds.
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "line1\n", "base");
    let to_drop = test.commit_file("a.txt", "line1\nline2\n", "add line2");
    let head = test.commit_file("a.txt", "line1\nline2\nline3\n", "add line3");

    let git_repo = test.git_repo();
    let result = git_repo
        .drop_commit(&Oid::from(to_drop), &Oid::from(head))
        .unwrap();

    let state = expect_rebase_conflict!(result);

    // Simulate external editor: write resolved content but do NOT stage.
    let workdir = test.repo.workdir().unwrap();
    std::fs::write(workdir.join("a.txt"), "line1\nline3\n").unwrap();

    // Index still has conflict entries at this point.
    assert!(
        !git_repo.read_conflicting_files().is_empty(),
        "conflict entries should still be in the index before auto-staging"
    );

    // Auto-stage should detect the file no longer has conflict markers.
    git_repo
        .auto_stage_resolved_conflicts(&state.conflicting_files)
        .unwrap();

    // Conflict entries should be cleared now.
    assert!(
        git_repo.read_conflicting_files().is_empty(),
        "conflict entries should be cleared after auto-staging"
    );

    // rebase_continue should succeed.
    let result = git_repo.rebase_continue(&state).unwrap();
    assert_rebase_complete!(result);

    let commits = common::commits_from_head(&test.repo, base);
    assert_eq!(commits.len(), 1);
    let head_oid = test.repo.head().unwrap().target().unwrap();
    assert_eq!(
        common::file_content_at(&test.repo, head_oid, "a.txt"),
        "line1\nline3\n"
    );
}

#[test]
fn auto_stage_does_not_stage_file_with_conflict_markers() {
    // If the working-tree file still contains conflict markers,
    // auto_stage_resolved_conflicts must leave it alone.
    let test = common::TestRepo::new();

    let _base = test.commit_file("a.txt", "line1\n", "base");
    let to_drop = test.commit_file("a.txt", "line1\nline2\n", "add line2");
    let head = test.commit_file("a.txt", "line1\nline2\nline3\n", "add line3");

    let git_repo = test.git_repo();
    let result = git_repo
        .drop_commit(&Oid::from(to_drop), &Oid::from(head))
        .unwrap();

    let state = expect_rebase_conflict!(result);

    // Working tree already has conflict markers from write_conflicts_to_workdir.
    // auto_stage should NOT clear them.
    git_repo
        .auto_stage_resolved_conflicts(&state.conflicting_files)
        .unwrap();

    assert!(
        !git_repo.read_conflicting_files().is_empty(),
        "conflict entries should still be present when markers remain"
    );
}
