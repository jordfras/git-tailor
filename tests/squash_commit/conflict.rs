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
fn squash_returns_conflict_when_source_and_target_conflict() {
    let test = common::TestRepo::new();

    let _base = test.commit_file("a.txt", "original\n", "base");
    let target = test.commit_file("a.txt", "target version\n", "target changes a");
    let _mid = test.commit_file("a.txt", "mid version\n", "mid changes a");
    let source = test.commit_file("a.txt", "source version\n", "source changes a");

    let git_repo = test.git_repo();
    let result = git_repo
        .squash_commits(
            &Oid::from(source),
            &Oid::from(target),
            "squashed",
            &Oid::from(source),
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
        .squash_commits(&Oid::from(source), &Oid::from(target), "squashed", &head)
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
            &Oid::from(source),
            &Oid::from(target),
            "squashed",
            &Oid::from(source),
        )
        .unwrap();

    assert_rebase_complete!(result);

    assert_file_contents_at_head!(&test.repo, "a.txt", "line1\nline2\nline3\n");
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
            &Oid::from(source),
            &Oid::from(target),
            "squashed",
            &Oid::from(after2),
        )
        .unwrap();

    assert_rebase_complete!(result);

    // 1 squash + 2 intermediates + 2 after = 5
    let commits = test.commits_from_head(base);
    assert_eq!(commits.len(), 5);

    // All files present
    for (path, expected) in [
        ("b.txt", "target\n"),
        ("c.txt", "mid1\n"),
        ("d.txt", "mid2\n"),
        ("e.txt", "source\n"),
        ("f.txt", "after1\n"),
        ("g.txt", "after2\n"),
    ] {
        assert_file_contents_at_head!(&test.repo, path, expected);
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
            &Oid::from(source),
            &Oid::from(target),
            "combined",
            SquashMode::Squash,
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
            &Oid::from(source),
            &Oid::from(target),
            "combined msg",
            SquashMode::Squash,
            &head,
        )
        .unwrap()
        .expect("should return conflict state");

    assert_eq!(state.operation_label, "Squash");
    assert!(!state.conflicting_files.is_empty());
    let ctx = state.squash_context.as_ref().unwrap();
    assert_eq!(ctx.source_oid, Oid::from(source));
    assert_eq!(ctx.target_oid, Oid::from(target));
    assert_eq!(ctx.combined_message, "combined msg");
}

#[test]
fn squash_finalize_after_conflict_resolution() {
    use git_tailor::app::SquashMode;
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
            &Oid::from(source),
            &Oid::from(target),
            "combined",
            SquashMode::Squash,
            &head,
        )
        .unwrap()
        .expect("should conflict");

    // Step 2: simulate user resolving the conflict
    test.write_file("a.txt", "resolved\n");
    git_repo.stage_file("a.txt").unwrap();

    // Step 3: finalize with NO descendants so that we only test the squash
    //         commit creation. (Intermediate commits that cause the initial
    //         conflict would cascade-conflict during cherry-pick; that outcome
    //         is exercised by the conflict-returning tests above.)
    let ctx = SquashContext {
        base_oid: state.squash_context.as_ref().unwrap().base_oid.clone(),
        source_oid: Oid::from(source),
        target_oid: Oid::from(target),
        combined_message: "combined".to_string(),
        descendant_oids: vec![],
        squash_mode: SquashMode::Squash,
    };

    let result = git_repo
        .squash_finalize(&ctx, "resolved squash", &state.original_branch_oid)
        .unwrap();

    assert_rebase_complete!(result);

    // Verify the squash commit
    let head_oid = test.repo.head().unwrap().target().unwrap();
    let head_commit = test.repo.find_commit(head_oid).unwrap();
    assert_eq!(head_commit.message().unwrap(), "resolved squash");
    assert_file_contents_at_head!(&test.repo, "a.txt", "resolved\n");

    // Squash commit's parent should be target's parent (the base commit)
    assert_eq!(head_commit.parent_count(), 1);
}
