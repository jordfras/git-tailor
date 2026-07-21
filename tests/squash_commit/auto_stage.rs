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
fn squash_finalize_after_external_conflict_resolution_without_staging() {
    // Simulates the user resolving a squash-time conflict in an external
    // editor (e.g. VS Code) without explicitly staging the file. After
    // calling auto_stage_resolved_conflicts, squash_finalize should succeed.
    use git_tailor::app::SquashMode;
    use git_tailor::repo::SquashContext;

    let test = common::TestRepo::new();

    let _base = test.commit_file("a.txt", "original\n", "base");
    let target = test.commit_file("a.txt", "target\n", "target changes a");
    let _mid = test.commit_file("a.txt", "mid\n", "mid changes a");
    let source = test.commit_file("a.txt", "source\n", "source changes a");

    let git_repo = test.git_repo();
    let head = git_repo.head_oid().unwrap();

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

    // Simulate external editor: write resolved content but do NOT stage.
    test.write_file("a.txt", "resolved\n");

    // Index still has conflict entries.
    assert!(
        !git_repo.read_conflicting_files().is_empty(),
        "conflict entries should still be in index before auto-staging"
    );

    // Auto-stage should detect the resolution.
    git_repo
        .auto_stage_resolved_conflicts(&state.conflicting_files)
        .unwrap();

    assert!(
        git_repo.read_conflicting_files().is_empty(),
        "conflict entries should be cleared after auto-staging"
    );

    let ctx = SquashContext {
        base_oid: state.squash_context.as_ref().unwrap().base_oid.clone(),
        source_oid: Oid::from(source),
        target_oid: Oid::from(target),
        combined_message: "combined".to_string(),
        descendant_oids: vec![],
        squash_mode: SquashMode::Squash,
    };

    let result = git_repo
        .squash_finalize(&ctx, "resolved squash", &state.original_branch_oid, None)
        .unwrap();

    assert_rebase_complete!(result);

    assert_file_contents_at_head!(&test.repo, "a.txt", "resolved\n");
}

/// Regression test: after resolving a squash-time conflict, the squash commit's
/// tree must not contain files that only exist in later (post-source) commits.
///
/// write_conflicts_to_workdir copies the cherry-pick merge index into the repo
/// index. If it doesn't clear stale entries first, files from HEAD leak into
/// the index. When squash_finalize reads that index to create the squash tree,
/// those leaked files produce wrong trees, which then cause spurious conflicts
/// (with empty merge bases) when cherry-picking descendants.
#[test]
fn squash_finalize_does_not_leak_descendant_files_into_squash_tree() {
    use git_tailor::app::SquashMode;
    use git_tailor::repo::SquashContext;

    let test = common::TestRepo::new();

    // Chain: base -> target -> mid -> source -> desc (HEAD)
    //   target, mid, source all modify a.txt (three-way squash conflict)
    //   desc adds b.txt (only present in HEAD, NOT in the squash trees)
    let _base = test.commit_file("a.txt", "original\n", "base");
    let target = test.commit_file("a.txt", "target version\n", "target");
    let _mid = test.commit_file("a.txt", "mid version\n", "mid");
    let source = test.commit_file("a.txt", "source version\n", "source");
    let _desc = test.commit_file("b.txt", "new file\n", "desc adds b.txt");

    let git_repo = test.git_repo();
    let head = git_repo.head_oid().unwrap();

    // Step 1: squash_try_combine detects the conflict
    let state = git_repo
        .squash_try_combine(
            &Oid::from(source),
            &Oid::from(target),
            "squashed",
            SquashMode::Squash,
            &head,
        )
        .unwrap()
        .expect("should conflict on a.txt");

    // Step 2: simulate resolving the conflict
    test.write_file("a.txt", "resolved\n");
    git_repo.stage_file("a.txt").unwrap();

    // Step 3: finalize with an empty descendant list to isolate the squash
    //         commit's tree from any descendant cherry-pick effects.
    let ctx = SquashContext {
        base_oid: state.squash_context.as_ref().unwrap().base_oid.clone(),
        source_oid: Oid::from(source),
        target_oid: Oid::from(target),
        combined_message: "squashed".to_string(),
        descendant_oids: vec![],
        squash_mode: SquashMode::Squash,
    };

    let result = git_repo
        .squash_finalize(&ctx, "squashed", &state.original_branch_oid, None)
        .unwrap();

    assert_rebase_complete!(result);

    // The squash commit's tree must NOT contain b.txt — a file that only
    // exists in the desc commit (post-source). If it does, stale entries
    // from HEAD's index leaked through write_conflicts_to_workdir.
    let head_oid = test.repo.head().unwrap().target().unwrap();
    let squash_tree = test.repo.find_commit(head_oid).unwrap().tree().unwrap();
    assert!(
        squash_tree.get_path(std::path::Path::new("b.txt")).is_err(),
        "squash tree should NOT contain b.txt (leaked from HEAD's index)"
    );
}

/// Regression test: aborting a conflicted squash/fixup must leave no staged
/// changes. After `write_conflicts_to_workdir` clears the index and populates
/// it from the cherry-pick result only (rooted in target's tree, not HEAD's),
/// `rebase_abort` must fully reset the index back to HEAD's tree. If it
/// doesn't, the leftover entries appear as staged changes.
#[test]
fn rebase_abort_after_squash_conflict_leaves_no_staged_changes() {
    let test = common::TestRepo::new();

    // base -> target -> mid -> source -> extra(HEAD)
    // target/mid/source conflict on a.txt; extra adds b.txt (only in HEAD)
    let _base = test.commit_file("a.txt", "original\n", "base");
    let target = test.commit_file("a.txt", "target version\n", "target");
    let _mid = test.commit_file("a.txt", "mid version\n", "mid");
    let source = test.commit_file("a.txt", "source version\n", "source");
    let _extra = test.commit_file("b.txt", "extra file\n", "extra adds b.txt");

    let git_repo = test.git_repo();
    let head = git_repo.head_oid().unwrap();
    let original_head = head.clone();

    // Trigger the conflict
    let state = git_repo
        .squash_try_combine(
            &Oid::from(source),
            &Oid::from(target),
            "combined",
            SquashMode::Fixup,
            &head,
        )
        .unwrap()
        .expect("should conflict on a.txt");

    // Abort the operation
    git_repo.rebase_abort(&state).unwrap();

    // HEAD must be restored
    assert_eq!(
        git_repo.head_oid().unwrap(),
        original_head,
        "HEAD should be restored to original"
    );

    // No staged changes must remain
    assert!(
        git_repo.staged_diff(3).unwrap().is_none(),
        "no staged changes should remain after abort"
    );

    // No unstaged changes must remain either
    assert!(
        git_repo.unstaged_diff(3).unwrap().is_none(),
        "no unstaged changes should remain after abort"
    );
}
