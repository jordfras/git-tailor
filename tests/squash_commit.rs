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
    oids.reverse();
    oids
}

// ---------------------------------------------------------------------------
// Happy-path tests
// ---------------------------------------------------------------------------

#[test]
fn squash_adjacent_commits_source_is_head() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "base\n", "base");
    let target = test.commit_file("a.txt", "target\n", "target commit");
    let source = test.commit_file("b.txt", "source\n", "source commit");

    let git_repo = test.git_repo();
    let result = git_repo
        .squash_commits(
            &source.to_string(),
            &target.to_string(),
            "squashed message",
            &source.to_string(),
        )
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
        "target\n",
        "target's file change should be in the squash"
    );
    assert_eq!(
        file_content_at(&test.repo, head_oid, "b.txt"),
        "source\n",
        "source's file change should be in the squash"
    );

    let squash_commit = test.repo.find_commit(head_oid).unwrap();
    assert_eq!(squash_commit.message().unwrap(), "squashed message");
}

#[test]
fn squash_non_adjacent_commits_rebases_intermediates() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "base\n", "base");
    let target = test.commit_file("a.txt", "target\n", "target commit");
    let middle = test.commit_file("c.txt", "middle\n", "middle commit");
    let source = test.commit_file("b.txt", "source\n", "source commit");

    let git_repo = test.git_repo();
    let result = git_repo
        .squash_commits(
            &source.to_string(),
            &target.to_string(),
            "squashed",
            &source.to_string(),
        )
        .unwrap();

    assert!(matches!(result, RebaseOutcome::Complete));

    let commits = commits_from_head(&test.repo, base);
    assert_eq!(
        commits.len(),
        2,
        "should have 2 commits: squash + rebased middle"
    );

    let head_oid = test.repo.head().unwrap().target().unwrap();

    // The squash is the older commit (index 0), middle is rebased on top
    let squash_commit = test.repo.find_commit(commits[0]).unwrap();
    assert_eq!(squash_commit.message().unwrap(), "squashed");

    let middle_commit = test.repo.find_commit(commits[1]).unwrap();
    assert_eq!(middle_commit.message().unwrap(), "middle commit");

    // Final tree should have all three files
    assert_eq!(file_content_at(&test.repo, head_oid, "a.txt"), "target\n");
    assert_eq!(file_content_at(&test.repo, head_oid, "b.txt"), "source\n");
    assert_eq!(file_content_at(&test.repo, head_oid, "c.txt"), "middle\n");

    // Verify middle was properly squash-excluded
    assert_ne!(
        commits[1], middle,
        "middle should be a new OID (rebased), not the original"
    );
}

#[test]
fn squash_source_not_head_rebases_later_commits() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "base\n", "base");
    let target = test.commit_file("a.txt", "target\n", "target commit");
    let source = test.commit_file("b.txt", "source\n", "source commit");
    let after = test.commit_file("c.txt", "after\n", "after commit");

    let git_repo = test.git_repo();
    let result = git_repo
        .squash_commits(
            &source.to_string(),
            &target.to_string(),
            "squashed",
            &after.to_string(),
        )
        .unwrap();

    assert!(matches!(result, RebaseOutcome::Complete));

    let commits = commits_from_head(&test.repo, base);
    assert_eq!(
        commits.len(),
        2,
        "should have 2 commits: squash + rebased after"
    );

    let head_oid = test.repo.head().unwrap().target().unwrap();
    assert_eq!(file_content_at(&test.repo, head_oid, "a.txt"), "target\n");
    assert_eq!(file_content_at(&test.repo, head_oid, "b.txt"), "source\n");
    assert_eq!(file_content_at(&test.repo, head_oid, "c.txt"), "after\n");

    let after_commit = test.repo.find_commit(commits[1]).unwrap();
    assert_eq!(after_commit.message().unwrap(), "after commit");
}

#[test]
fn squash_uses_provided_message() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "base\n", "base");
    let target = test.commit_file("b.txt", "target\n", "target msg");
    let source = test.commit_file("c.txt", "source\n", "source msg");

    let git_repo = test.git_repo();
    let custom_message = "target msg\n\nsource msg\n";
    git_repo
        .squash_commits(
            &source.to_string(),
            &target.to_string(),
            custom_message,
            &source.to_string(),
        )
        .unwrap();

    let commits = commits_from_head(&test.repo, base);
    assert_eq!(commits.len(), 1);

    let squash = test.repo.find_commit(commits[0]).unwrap();
    assert_eq!(squash.message().unwrap(), custom_message);
}

#[test]
fn squash_preserves_target_authorship() {
    let test = common::TestRepo::new();

    let _base = test.commit_file("a.txt", "base\n", "base");
    let target = test.commit_file("b.txt", "target\n", "target");
    let source = test.commit_file("c.txt", "source\n", "source");

    let target_commit = test.repo.find_commit(target).unwrap();
    let target_author = target_commit.author().name().unwrap().to_string();

    let git_repo = test.git_repo();
    git_repo
        .squash_commits(
            &source.to_string(),
            &target.to_string(),
            "squashed",
            &source.to_string(),
        )
        .unwrap();

    let head_oid = test.repo.head().unwrap().target().unwrap();
    let squash_commit = test.repo.find_commit(head_oid).unwrap();
    assert_eq!(
        squash_commit.author().name().unwrap(),
        target_author,
        "squash commit should keep target's author"
    );
}

// ---------------------------------------------------------------------------
// Squash-time conflict tests (source changes overlap with target — T080)
// ---------------------------------------------------------------------------

#[test]
fn squash_returns_conflict_when_source_and_target_conflict() {
    let test = common::TestRepo::new();

    let _base = test.commit_file("a.txt", "original\n", "base");
    let target = test.commit_file("a.txt", "target version\n", "target changes a");
    let _mid = test.commit_file("a.txt", "mid version\n", "mid changes a");
    let source = test.commit_file("a.txt", "source version\n", "source changes a");

    let git_repo = test.git_repo();
    let result = git_repo
        .squash_commits(
            &source.to_string(),
            &target.to_string(),
            "squashed",
            &source.to_string(),
        )
        .unwrap();

    match result {
        RebaseOutcome::Conflict(state) => {
            assert_eq!(state.operation_label, "Squash");
            assert!(
                state.squash_context.is_some(),
                "squash-time conflict should carry squash_context"
            );
            assert!(
                !state.conflicting_files.is_empty(),
                "should list conflicting files"
            );
        }
        RebaseOutcome::Complete => panic!("expected Conflict, got Complete"),
    }
}

#[test]
fn squash_returns_conflict_when_all_three_modify_same_file() {
    let test = common::TestRepo::new();

    let _base = test.commit_file("a.txt", "base\n", "base");
    let target = test.commit_file("a.txt", "target\n", "target modifies a");
    let _mid = test.commit_file("a.txt", "intermediate\n", "mid modifies a");
    let source = test.commit_file("a.txt", "source\n", "source modifies a");
    let _after = test.commit_file("b.txt", "after\n", "after source");

    let git_repo = test.git_repo();
    let head = git_repo.head_oid().unwrap();
    let result = git_repo
        .squash_commits(&source.to_string(), &target.to_string(), "squashed", &head)
        .unwrap();

    match result {
        RebaseOutcome::Conflict(state) => {
            assert_eq!(state.operation_label, "Squash");
            assert!(state.squash_context.is_some());
            let ctx = state.squash_context.unwrap();
            // The descendant that is NOT the source should be in the list
            assert!(
                !ctx.descendant_oids.is_empty(),
                "should have descendants to rebase after resolution"
            );
        }
        RebaseOutcome::Complete => panic!("expected Conflict, got Complete"),
    }
}

#[test]
fn squash_source_onto_target_overlapping_edits_errors() {
    let test = common::TestRepo::new();

    let _base = test.commit_file("a.txt", "line1\n", "base");
    let target = test.commit_file("a.txt", "line1\nline2\n", "target adds line2");
    let source = test.commit_file("a.txt", "line1\nline2\nline3\n", "source adds line3");

    let git_repo = test.git_repo();
    // Source modifies the same file as target in a way that may conflict
    // when cherry-picked. For this specific case git can auto-merge, so
    // it should succeed.
    let result = git_repo
        .squash_commits(
            &source.to_string(),
            &target.to_string(),
            "squashed",
            &source.to_string(),
        )
        .unwrap();

    assert!(
        matches!(result, RebaseOutcome::Complete),
        "sequential line additions should auto-merge: got {result:?}"
    );

    let head_oid = test.repo.head().unwrap().target().unwrap();
    assert_eq!(
        file_content_at(&test.repo, head_oid, "a.txt"),
        "line1\nline2\nline3\n"
    );
}

#[test]
fn squash_with_multiple_intermediates_and_descendants() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "base\n", "base");
    let target = test.commit_file("b.txt", "target\n", "target");
    let mid1 = test.commit_file("c.txt", "mid1\n", "mid1");
    let _mid2 = test.commit_file("d.txt", "mid2\n", "mid2");
    let source = test.commit_file("e.txt", "source\n", "source");
    let _after1 = test.commit_file("f.txt", "after1\n", "after1");
    let after2 = test.commit_file("g.txt", "after2\n", "after2");

    let git_repo = test.git_repo();
    let result = git_repo
        .squash_commits(
            &source.to_string(),
            &target.to_string(),
            "squashed",
            &after2.to_string(),
        )
        .unwrap();

    assert!(matches!(result, RebaseOutcome::Complete));

    // 1 squash + 2 intermediates + 2 after = 5
    let commits = commits_from_head(&test.repo, base);
    assert_eq!(commits.len(), 5);

    let head_oid = test.repo.head().unwrap().target().unwrap();
    // All files present
    for (path, expected) in [
        ("b.txt", "target\n"),
        ("c.txt", "mid1\n"),
        ("d.txt", "mid2\n"),
        ("e.txt", "source\n"),
        ("f.txt", "after1\n"),
        ("g.txt", "after2\n"),
    ] {
        assert_eq!(
            file_content_at(&test.repo, head_oid, path),
            expected,
            "file {path} should have correct content"
        );
    }

    // Squash commit (first after base) should combine target + source
    let squash = test.repo.find_commit(commits[0]).unwrap();
    assert_eq!(squash.message().unwrap(), "squashed");

    // Mid1 should keep its message
    let mid1_rebased = test.repo.find_commit(commits[1]).unwrap();
    assert_eq!(mid1_rebased.message().unwrap(), "mid1");
    assert_ne!(commits[1], mid1, "mid1 should have a new OID after rebase");
}

#[test]
fn squash_try_combine_returns_none_when_clean() {
    let test = common::TestRepo::new();

    let _base = test.commit_file("a.txt", "base\n", "base");
    let target = test.commit_file("b.txt", "target\n", "target");
    let source = test.commit_file("c.txt", "source\n", "source");

    let git_repo = test.git_repo();
    let head = git_repo.head_oid().unwrap();

    let result = git_repo
        .squash_try_combine(
            &source.to_string(),
            &target.to_string(),
            "combined",
            false,
            &head,
        )
        .unwrap();

    assert!(result.is_none(), "clean merge should return None");
}

#[test]
fn squash_try_combine_returns_conflict_state() {
    let test = common::TestRepo::new();

    let _base = test.commit_file("a.txt", "original\n", "base");
    let target = test.commit_file("a.txt", "target\n", "target");
    let _mid = test.commit_file("a.txt", "mid\n", "mid");
    let source = test.commit_file("a.txt", "source\n", "source");

    let git_repo = test.git_repo();
    let head = git_repo.head_oid().unwrap();

    let state = git_repo
        .squash_try_combine(
            &source.to_string(),
            &target.to_string(),
            "combined msg",
            false,
            &head,
        )
        .unwrap()
        .expect("should return conflict state");

    assert_eq!(state.operation_label, "Squash");
    assert!(!state.conflicting_files.is_empty());
    let ctx = state.squash_context.as_ref().unwrap();
    assert_eq!(ctx.source_oid, source.to_string());
    assert_eq!(ctx.target_oid, target.to_string());
    assert_eq!(ctx.combined_message, "combined msg");
}

#[test]
fn squash_finalize_after_conflict_resolution() {
    use git_tailor::repo::SquashContext;

    let test = common::TestRepo::new();

    let _base = test.commit_file("a.txt", "original\n", "base");
    let target = test.commit_file("a.txt", "target\n", "target changes a");
    let _mid = test.commit_file("a.txt", "mid\n", "mid changes a");
    let source = test.commit_file("a.txt", "source\n", "source changes a");

    let git_repo = test.git_repo();
    let head = git_repo.head_oid().unwrap();

    // Step 1: try combine -> conflict
    let state = git_repo
        .squash_try_combine(
            &source.to_string(),
            &target.to_string(),
            "combined",
            false,
            &head,
        )
        .unwrap()
        .expect("should conflict");

    // Step 2: simulate user resolving the conflict
    let workdir = git_repo.workdir().unwrap();
    std::fs::write(workdir.join("a.txt"), "resolved\n").unwrap();
    git_repo.stage_file("a.txt").unwrap();

    // Step 3: finalize with NO descendants so that we only test the squash
    //         commit creation. (Intermediate commits that cause the initial
    //         conflict would cascade-conflict during cherry-pick; that outcome
    //         is exercised by the conflict-returning tests above.)
    let ctx = SquashContext {
        base_oid: state.squash_context.as_ref().unwrap().base_oid.clone(),
        source_oid: source.to_string(),
        target_oid: target.to_string(),
        combined_message: "combined".to_string(),
        descendant_oids: vec![],
        is_fixup: false,
    };

    let result = git_repo
        .squash_finalize(&ctx, "resolved squash", &state.original_branch_oid)
        .unwrap();

    assert!(
        matches!(result, RebaseOutcome::Complete),
        "should complete after resolution: {result:?}"
    );

    // Verify the squash commit
    let head_oid = test.repo.head().unwrap().target().unwrap();
    let head_commit = test.repo.find_commit(head_oid).unwrap();
    assert_eq!(head_commit.message().unwrap(), "resolved squash");
    assert_eq!(
        file_content_at(&test.repo, head_oid, "a.txt"),
        "resolved\n",
        "resolved content should be in HEAD"
    );

    // Squash commit's parent should be target's parent (the base commit)
    assert_eq!(head_commit.parent_count(), 1);
}

// ---------------------------------------------------------------------------
// Dirty-state guard tests
// ---------------------------------------------------------------------------

#[test]
fn squash_commits_blocked_with_staged_changes() {
    let test = common::TestRepo::new();

    let _base = test.commit_file("a.txt", "base\n", "base");
    let target = test.commit_file("a.txt", "target\n", "target");
    let source = test.commit_file("b.txt", "source\n", "source");

    // Stage a change to an unrelated file
    let workdir = test.repo.workdir().unwrap();
    std::fs::write(workdir.join("unrelated.txt"), "staged work\n").unwrap();
    let mut index = test.repo.index().unwrap();
    index
        .add_path(std::path::Path::new("unrelated.txt"))
        .unwrap();
    index.write().unwrap();

    let git_repo = test.git_repo();
    let result = git_repo.squash_commits(
        &source.to_string(),
        &target.to_string(),
        "combined",
        &source.to_string(),
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
    let workdir = test.repo.workdir().unwrap();
    std::fs::write(workdir.join("a.txt"), "unstaged work\n").unwrap();

    let git_repo = test.git_repo();
    let result = git_repo.squash_commits(
        &source.to_string(),
        &target.to_string(),
        "combined",
        &source.to_string(),
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
    let workdir = test.repo.workdir().unwrap();
    std::fs::write(workdir.join("unrelated.txt"), "staged work\n").unwrap();
    let mut index = test.repo.index().unwrap();
    index
        .add_path(std::path::Path::new("unrelated.txt"))
        .unwrap();
    index.write().unwrap();

    let git_repo = test.git_repo();
    let result = git_repo.squash_try_combine(
        &source.to_string(),
        &target.to_string(),
        "combined",
        false,
        &source.to_string(),
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
    let workdir = test.repo.workdir().unwrap();
    std::fs::write(workdir.join("a.txt"), "unstaged work\n").unwrap();

    let git_repo = test.git_repo();
    let result = git_repo.squash_try_combine(
        &source.to_string(),
        &target.to_string(),
        "combined",
        false,
        &source.to_string(),
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
    common::stage_gitlink(&test.repo, "libs/sub", base);

    let git_repo = test.git_repo();
    let result = git_repo
        .squash_commits(
            &source.to_string(),
            &target.to_string(),
            "squashed",
            &source.to_string(),
        )
        .unwrap();

    assert!(
        matches!(result, RebaseOutcome::Complete),
        "squash should succeed when only a submodule pointer is staged; got {result:?}"
    );
}

#[test]
fn squash_try_combine_allowed_with_staged_submodule() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "base\n", "base");
    let target = test.commit_file("b.txt", "b\n", "target commit");
    let source = test.commit_file("c.txt", "c\n", "source commit");

    // Stage a submodule pointer update — the only dirty state is a gitlink.
    common::stage_gitlink(&test.repo, "libs/sub", base);

    let git_repo = test.git_repo();
    // Returns Ok(None) when there is no merge conflict — both files differ.
    let result = git_repo
        .squash_try_combine(
            &source.to_string(),
            &target.to_string(),
            "squashed",
            false,
            &source.to_string(),
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
            &source.to_string(),
            &target.to_string(),
            "combined",
            false,
            &_base.to_string(),
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
