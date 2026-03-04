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

mod common;

use git_tailor::repo::{GitRepo, RebaseOutcome};

/// Read a file from a specific commit tree.
fn file_content_at(repo: &git2::Repository, commit_oid: git2::Oid, path: &str) -> String {
    let commit = repo.find_commit(commit_oid).unwrap();
    let tree = commit.tree().unwrap();
    let entry = tree.get_path(std::path::Path::new(path)).unwrap();
    let blob = repo
        .find_blob(entry.id())
        .expect("tree entry should be a blob");
    String::from_utf8_lossy(blob.content()).into_owned()
}

/// Walk commits from HEAD back to (but not including) the given stop OID.
fn commits_from_head(repo: &git2::Repository, stop_oid: git2::Oid) -> Vec<git2::Oid> {
    let head_oid = repo.head().unwrap().target().unwrap();
    let mut revwalk = repo.revwalk().unwrap();
    revwalk.push(head_oid).unwrap();
    let mut oids = Vec::new();
    for result in revwalk {
        let oid = result.unwrap();
        if oid == stop_oid {
            break;
        }
        oids.push(oid);
    }
    oids.reverse(); // oldest first
    oids
}

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
        .drop_commit(&to_drop.to_string(), &to_drop.to_string())
        .unwrap();

    assert!(
        matches!(result, RebaseOutcome::Complete),
        "expected Complete, got {result:?}"
    );

    let commits = commits_from_head(&test.repo, base);
    assert_eq!(commits.len(), 1, "should have 1 commit above base");

    let head_oid = test.repo.head().unwrap().target().unwrap();
    assert_eq!(
        file_content_at(&test.repo, head_oid, "a.txt"),
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
        .drop_commit(&to_drop.to_string(), &child.to_string())
        .unwrap();

    assert!(
        matches!(result, RebaseOutcome::Complete),
        "expected Complete, got {result:?}"
    );

    let commits = commits_from_head(&test.repo, base);
    assert_eq!(
        commits.len(),
        1,
        "should have 1 commit above base (dropped middle)"
    );

    let head_oid = test.repo.head().unwrap().target().unwrap();
    assert_eq!(
        file_content_at(&test.repo, head_oid, "a.txt"),
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
        .drop_commit(&to_drop.to_string(), &head.to_string())
        .unwrap();

    assert!(matches!(result, RebaseOutcome::Complete));

    let commits = commits_from_head(&test.repo, base);
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
        .drop_commit(&to_drop.to_string(), &head_oid)
        .unwrap();

    let commits = commits_from_head(&test.repo, base);
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
        .drop_commit(&to_drop.to_string(), &head.to_string())
        .unwrap();

    match result {
        RebaseOutcome::Conflict(state) => {
            assert_eq!(state.conflicting_commit_oid, head.to_string());
            assert!(
                state.remaining_oids.is_empty(),
                "no commits after the conflicting one"
            );
            assert_eq!(state.original_branch_oid, head.to_string());
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
        .drop_commit(&to_drop.to_string(), &head_oid)
        .unwrap();

    match result {
        RebaseOutcome::Conflict(state) => {
            assert_eq!(state.conflicting_commit_oid, child1.to_string());
            assert_eq!(state.remaining_oids, vec![child2.to_string()]);
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
        .drop_commit(&to_drop.to_string(), &head.to_string())
        .unwrap();

    let state = match result {
        RebaseOutcome::Conflict(s) => s,
        RebaseOutcome::Complete => panic!("expected Conflict"),
    };

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

    let result = git_repo.drop_commit_continue(&state).unwrap();
    assert!(
        matches!(result, RebaseOutcome::Complete),
        "expected Complete after resolution, got {result:?}"
    );

    let commits = commits_from_head(&test.repo, base);
    assert_eq!(commits.len(), 1);

    let head_oid = test.repo.head().unwrap().target().unwrap();
    assert_eq!(
        file_content_at(&test.repo, head_oid, "a.txt"),
        "line1\nline3\n"
    );
}

#[test]
fn drop_continue_with_unresolved_conflicts_stays_in_conflict_mode() {
    // Calling drop_commit_continue when the index still has conflict markers
    // must return Conflict (same OIDs, refreshed file list) instead of an
    // error — leaving the repo in a usable state so the user can keep
    // editing or abort.
    let test = common::TestRepo::new();

    let _base = test.commit_file("a.txt", "line1\n", "base");
    let to_drop = test.commit_file("a.txt", "line1\nline2\n", "add line2");
    let head = test.commit_file("a.txt", "line1\nline2\nline3\n", "add line3");

    let git_repo = test.git_repo();
    let result = git_repo
        .drop_commit(&to_drop.to_string(), &head.to_string())
        .unwrap();

    let state = match result {
        RebaseOutcome::Conflict(s) => s,
        RebaseOutcome::Complete => panic!("expected Conflict"),
    };

    // Do NOT resolve the conflict — just call continue immediately.
    let result = git_repo.drop_commit_continue(&state).unwrap();

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
    git_repo.drop_commit_abort(&state).unwrap();
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
        .drop_commit(&to_drop.to_string(), &head.to_string())
        .unwrap();

    let state = match result {
        RebaseOutcome::Conflict(s) => s,
        RebaseOutcome::Complete => panic!("expected Conflict"),
    };

    git_repo.drop_commit_abort(&state).unwrap();

    // Branch should be back to the original HEAD.
    let current_head = test.repo.head().unwrap().target().unwrap();
    assert_eq!(
        current_head, head,
        "HEAD should be restored to original after abort"
    );

    assert_eq!(
        file_content_at(&test.repo, current_head, "a.txt"),
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
    assert_eq!(head_oid_before, child2.to_string());

    // First drop → first conflict on child1.
    let result = git_repo
        .drop_commit(&to_drop.to_string(), &child2.to_string())
        .unwrap();
    let state1 = match result {
        RebaseOutcome::Conflict(s) => s,
        RebaseOutcome::Complete => panic!("expected first Conflict"),
    };
    assert_eq!(state1.conflicting_commit_oid, child1.to_string());
    assert_eq!(state1.original_branch_oid, child2.to_string());

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
    let result = git_repo.drop_commit_continue(&state1).unwrap();
    let state2 = match result {
        RebaseOutcome::Conflict(s) => s,
        RebaseOutcome::Complete => panic!("expected second Conflict"),
    };
    assert_eq!(state2.conflicting_commit_oid, child2.to_string());
    // original_branch_oid must still refer to the pre-drop HEAD.
    assert_eq!(state2.original_branch_oid, child2.to_string());

    // Abort from the second conflict — must fully restore the branch.
    git_repo.drop_commit_abort(&state2).unwrap();

    let head_after = test.repo.head().unwrap().target().unwrap();
    assert_eq!(
        head_after, child2,
        "HEAD must be fully restored after abort at second conflict"
    );
    assert_eq!(
        file_content_at(&test.repo, head_after, "a.txt"),
        "v4\n",
        "file content must match original HEAD after abort"
    );
}

#[test]
fn drop_root_commit_fails() {
    let test = common::TestRepo::new();

    let root = test.commit_file("a.txt", "v1\n", "root");

    let git_repo = test.git_repo();
    let result = git_repo.drop_commit(&root.to_string(), &root.to_string());

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
        .drop_commit(&to_drop.to_string(), &to_drop.to_string())
        .unwrap();

    assert!(matches!(result, RebaseOutcome::Complete));

    let head_oid = test.repo.head().unwrap().target().unwrap();
    assert_eq!(
        head_oid, base,
        "HEAD should point to base after dropping the only commit above it"
    );
}
