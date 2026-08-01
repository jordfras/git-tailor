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
fn split_per_file_creates_two_commits() {
    let test = common::TestRepo::new();

    // Base commit so there is a parent
    let base = test.commit_files(&[("a.txt", "alpha\n"), ("b.txt", "beta\n")], "base");

    // Commit that touches both files — this is the one we'll split
    let to_split = test.commit_files(&[("a.txt", "alpha2\n"), ("b.txt", "beta2\n")], "big change");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    git_repo
        .split_commit_per_file(&Oid::from(to_split), &head_oid)
        .unwrap();

    // There should now be 3 commits above base: base, split-1, split-2
    let commits_above_base = test.commits_from_head(base);
    assert_eq!(
        commits_above_base.len(),
        2,
        "expected 2 split commits above base"
    );

    let split1_oid = commits_above_base[0];
    let split2_oid = commits_above_base[1];

    // Commit messages should carry (n/total) numbering
    let split1 = test.repo.find_commit(split1_oid).unwrap();
    let split2 = test.repo.find_commit(split2_oid).unwrap();
    let msg1 = split1.summary().ok().flatten().unwrap_or("");
    let msg2 = split2.summary().ok().flatten().unwrap_or("");
    assert!(msg1.contains("(1/2)"), "expected (1/2) in: {}", msg1);
    assert!(msg2.contains("(2/2)"), "expected (2/2) in: {}", msg2);

    // split1 should only change one file relative to base
    let diff1 = {
        let parent_tree = test
            .repo
            .find_commit(split1.parent_id(0).unwrap())
            .unwrap()
            .tree()
            .unwrap();
        let commit_tree = split1.tree().unwrap();
        test.repo
            .diff_tree_to_tree(Some(&parent_tree), Some(&commit_tree), None)
            .unwrap()
    };
    assert_eq!(
        diff1.deltas().len(),
        1,
        "split1 should touch exactly 1 file"
    );

    // split2 should only change the other file relative to split1
    let diff2 = {
        let parent_tree = test
            .repo
            .find_commit(split2.parent_id(0).unwrap())
            .unwrap()
            .tree()
            .unwrap();
        let commit_tree = split2.tree().unwrap();
        test.repo
            .diff_tree_to_tree(Some(&parent_tree), Some(&commit_tree), None)
            .unwrap()
    };
    assert_eq!(
        diff2.deltas().len(),
        1,
        "split2 should touch exactly 1 file"
    );

    // Together the two split commits contain all the changes from the original
    assert_file_contents!(&test.repo, split2_oid, "a.txt", "alpha2\n");
    assert_file_contents!(&test.repo, split2_oid, "b.txt", "beta2\n");
}

#[test]
fn split_per_file_rebases_descendants() {
    let test = common::TestRepo::new();

    let base = test.commit_files(&[("a.txt", "alpha\n"), ("b.txt", "beta\n")], "base");

    let to_split = test.commit_files(&[("a.txt", "alpha2\n"), ("b.txt", "beta2\n")], "big change");

    // A descendant commit that only touches a third file — should rebase cleanly
    test.commit_file("c.txt", "gamma\n", "add c");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    git_repo
        .split_commit_per_file(&Oid::from(to_split), &head_oid)
        .unwrap();

    // We should now have: base → split1 → split2 → rebased-c
    let commits_above_base = test.commits_from_head(base);
    assert_eq!(
        commits_above_base.len(),
        3,
        "expected 2 split commits + 1 rebased descendant"
    );

    // Rebased descendant should still have the c.txt content
    let rebased_tip = *commits_above_base.last().unwrap();
    assert_file_contents!(&test.repo, rebased_tip, "c.txt", "gamma\n");
    // And all files from the split should also be present
    assert_file_contents!(&test.repo, rebased_tip, "a.txt", "alpha2\n");
    assert_file_contents!(&test.repo, rebased_tip, "b.txt", "beta2\n");
}

#[test]
fn split_per_file_refuses_single_file_commit() {
    let test = common::TestRepo::new();
    test.commit_file("a.txt", "alpha\n", "base");
    let only_one = test.commit_file("a.txt", "alpha2\n", "single file");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    let result = git_repo.split_commit_per_file(&Oid::from(only_one), &head_oid);
    assert!(
        result.is_err(),
        "should fail when commit touches only 1 file"
    );
}

#[test]
fn split_per_file_refuses_dirty_overlap() {
    let test = common::TestRepo::new();
    test.commit_files(&[("a.txt", "alpha\n"), ("b.txt", "beta\n")], "base");
    let to_split = test.commit_files(&[("a.txt", "alpha2\n"), ("b.txt", "beta2\n")], "big change");

    // Stage a change to a.txt (overlaps with the commit being split)
    test.write_file("a.txt", "DIRTY\n");
    test.stage_file("a.txt");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    let result = git_repo.split_commit_per_file(&Oid::from(to_split), &head_oid);
    assert!(result.is_err(), "should fail when staged changes overlap");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("a.txt"),
        "error should mention the overlapping file, got: {}",
        msg
    );
}

#[test]
fn split_per_file_handles_submodule_delta() {
    // A commit that changes a regular file AND bumps a gitlink (submodule
    // pointer) must not crash.  apply_to_tree cannot handle gitlink entries
    // (mode 0o160000) because libgit2 tries to patch them as blobs; the fix
    // routes gitlink deltas through TreeUpdateBuilder instead.
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "alpha\n", "base");

    let head_commit = test.repo.find_commit(base).unwrap();
    let base_tree = head_commit.tree().unwrap();

    let new_blob_oid = test.repo.blob(b"alpha2\n").unwrap();
    // Any valid OID can act as the fake submodule commit pointer.
    let fake_sub_oid = base;

    let combined_tree_oid = git2::build::TreeUpdateBuilder::new()
        .upsert("a.txt", new_blob_oid, git2::FileMode::Blob)
        .upsert("sub", fake_sub_oid, git2::FileMode::Commit)
        .create_updated(&test.repo, &base_tree)
        .unwrap();

    let sig = git2::Signature::now("Test User", "test@example.com").unwrap();
    let combined_tree = test.repo.find_tree(combined_tree_oid).unwrap();
    let to_split = test
        .repo
        .commit(
            Some("HEAD"),
            &sig,
            &sig,
            "change file and bump submodule",
            &combined_tree,
            &[&head_commit],
        )
        .unwrap();

    // HEAD now has {a.txt="alpha2\n", sub=gitlink} but the index/workdir still
    // reflect the old state.  A Mixed reset syncs the index with HEAD so
    // check_dirty_overlap does not false-positive on a.txt and sub.
    let obj = test.repo.find_object(to_split, None).unwrap();
    test.repo.reset(&obj, git2::ResetType::Mixed, None).unwrap();
    // Update the workdir file so diff_index_to_workdir has nothing to report.
    test.write_file("a.txt", "alpha2\n");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    git_repo
        .split_commit_per_file(&Oid::from(to_split), &head_oid)
        .unwrap();

    let commits_above_base = test.commits_from_head(base);
    assert_eq!(commits_above_base.len(), 2, "expected 2 split commits");

    let tip_oid = *commits_above_base.last().unwrap();
    let tip_tree = test.repo.find_commit(tip_oid).unwrap().tree().unwrap();

    assert!(
        tip_tree.get_path(std::path::Path::new("a.txt")).is_ok(),
        "a.txt should be present in final split commit"
    );

    let sub_entry = tip_tree
        .get_path(std::path::Path::new("sub"))
        .expect("submodule entry should be present in final split commit");
    assert_eq!(
        sub_entry.filemode(),
        0o160000,
        "sub should have gitlink mode"
    );
    assert_eq!(
        sub_entry.id(),
        fake_sub_oid,
        "sub should point at expected OID"
    );
}

#[test]
fn split_per_file_preserves_commit_message_body() {
    let test = common::TestRepo::new();

    test.commit_files(&[("a.txt", "alpha\n"), ("b.txt", "beta\n")], "base");
    let to_split = test.commit_files(
        &[("a.txt", "alpha2\n"), ("b.txt", "beta2\n")],
        "big change\n\nThis is the body.\nIt has multiple lines.",
    );

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();
    git_repo
        .split_commit_per_file(&Oid::from(to_split), &head_oid)
        .unwrap();

    let base_oid = test
        .repo
        .find_commit(to_split)
        .unwrap()
        .parent_id(0)
        .unwrap();
    let commits_above_base = test.commits_from_head(base_oid);
    assert_eq!(commits_above_base.len(), 2);

    for oid in &commits_above_base {
        let commit = test.repo.find_commit(*oid).unwrap();
        let msg = commit.message().unwrap_or("");
        assert!(
            msg.contains("This is the body."),
            "expected body in commit message, got: {:?}",
            msg
        );
        assert!(
            msg.contains("big change"),
            "expected summary in commit message, got: {:?}",
            msg
        );
    }
}

#[test]
fn split_per_file_handles_submodule_delta_not_last() {
    // Same gitlink hazard as `split_per_file_handles_submodule_delta`, but with
    // the submodule sorting *before* the regular file. Deltas are path-ordered,
    // so there the gitlink is the last delta — and the last piece is committed
    // with the original tree verbatim, which would bypass the gitlink handling
    // entirely. Here `sub` < `z.txt`, so the gitlink still goes through
    // `apply_gitlink_delta_to_tree`.
    let test = common::TestRepo::new();

    let base = test.commit_file("z.txt", "zulu\n", "base");

    let head_commit = test.repo.find_commit(base).unwrap();
    let base_tree = head_commit.tree().unwrap();

    let new_blob_oid = test.repo.blob(b"zulu2\n").unwrap();
    // Any valid OID can act as the fake submodule commit pointer.
    let fake_sub_oid = base;

    let combined_tree_oid = git2::build::TreeUpdateBuilder::new()
        .upsert("z.txt", new_blob_oid, git2::FileMode::Blob)
        .upsert("sub", fake_sub_oid, git2::FileMode::Commit)
        .create_updated(&test.repo, &base_tree)
        .unwrap();

    let sig = git2::Signature::now("Test User", "test@example.com").unwrap();
    let combined_tree = test.repo.find_tree(combined_tree_oid).unwrap();
    let to_split = test
        .repo
        .commit(
            Some("HEAD"),
            &sig,
            &sig,
            "bump submodule and change file",
            &combined_tree,
            &[&head_commit],
        )
        .unwrap();

    let obj = test.repo.find_object(to_split, None).unwrap();
    test.repo.reset(&obj, git2::ResetType::Mixed, None).unwrap();
    test.write_file("z.txt", "zulu2\n");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    git_repo
        .split_commit_per_file(&Oid::from(to_split), &head_oid)
        .unwrap();

    let commits_above_base = test.commits_from_head(base);
    assert_eq!(commits_above_base.len(), 2, "expected 2 split commits");

    // The gitlink sorts first, so the *first* split piece is the one built by
    // apply_gitlink_delta_to_tree.
    let first_oid = commits_above_base[0];
    let first_tree = test.repo.find_commit(first_oid).unwrap().tree().unwrap();
    let sub_entry = first_tree
        .get_path(std::path::Path::new("sub"))
        .expect("submodule entry should be present in the first split commit");
    assert_eq!(
        sub_entry.filemode(),
        0o160000,
        "sub should have gitlink mode"
    );
    assert_eq!(
        sub_entry.id(),
        fake_sub_oid,
        "sub should point at expected OID"
    );
    assert_eq!(
        first_tree
            .get_path(std::path::Path::new("z.txt"))
            .unwrap()
            .id(),
        base_tree
            .get_path(std::path::Path::new("z.txt"))
            .unwrap()
            .id(),
        "the first piece must not yet contain the z.txt change"
    );

    let tip_tree = test
        .repo
        .find_commit(*commits_above_base.last().unwrap())
        .unwrap()
        .tree()
        .unwrap();
    assert_eq!(
        tip_tree
            .get_path(std::path::Path::new("z.txt"))
            .unwrap()
            .id(),
        new_blob_oid,
        "the final piece must contain the z.txt change"
    );
}

#[test]
fn split_per_file_last_piece_has_original_tree() {
    let test = common::TestRepo::new();

    test.commit_files(
        &[("a.txt", "a\n"), ("b.txt", "b\n"), ("c.txt", "c\n")],
        "base",
    );
    let to_split = test.commit_files(
        &[("a.txt", "a2\n"), ("b.txt", "b2\n"), ("c.txt", "c2\n")],
        "change all three",
    );
    let original_tree = test.tree_id(to_split);

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();
    git_repo
        .split_commit_per_file(&Oid::from(to_split), &head_oid)
        .unwrap();

    assert_eq!(
        test.head_tree_id(),
        original_tree,
        "the last split piece must reproduce the original commit's tree"
    );
}

#[test]
fn split_per_file_rename_last_piece_has_original_tree() {
    // Per-file is the one strategy whose final tree is accumulated through
    // repeated apply_to_tree rather than committed from the original tree, so
    // exercise it with a tree shape that is not a plain content edit: a rename
    // (delete + add, since per-file does not run rename detection).
    let test = common::TestRepo::new();

    test.commit_files(&[("foo.txt", "one\n"), ("keep.txt", "keep\n")], "base");
    test.rename_file(
        "foo.txt",
        "bar.txt",
        Some("one changed\n"),
        "rename and edit",
    );
    let to_split = test.commit_files(
        &[("bar.txt", "one changed twice\n"), ("keep.txt", "keep2\n")],
        "edit both",
    );
    let original_tree = test.tree_id(to_split);

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();
    git_repo
        .split_commit_per_file(&Oid::from(to_split), &head_oid)
        .unwrap();

    assert_eq!(
        test.head_tree_id(),
        original_tree,
        "the last split piece must reproduce the original commit's tree"
    );
}

#[test]
fn split_per_file_replay_preserves_descendant_trees() {
    // The descendant replay is conflict-free only because the last split piece
    // has the original commit's tree; cherry-picking onto it then reproduces
    // each descendant's tree byte-for-byte. Pin that end to end.
    let test = common::TestRepo::new();

    let base = test.commit_files(&[("a.txt", "a\n"), ("b.txt", "b\n")], "base");
    let to_split = test.commit_files(&[("a.txt", "a2\n"), ("b.txt", "b2\n")], "change both");
    let original_tree = test.tree_id(to_split);
    let d1 = test.commit_file("c.txt", "c\n", "descendant 1");
    let d2 = test.commit_file("d.txt", "d\n", "descendant 2");
    let d1_tree = test.tree_id(d1);
    let d2_tree = test.tree_id(d2);

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();
    git_repo
        .split_commit_per_file(&Oid::from(to_split), &head_oid)
        .unwrap();

    let trees = test.tree_ids_from_head(base);
    assert_eq!(trees.len(), 4, "2 split pieces + 2 descendants");
    assert_eq!(
        &trees[1..],
        &[original_tree, d1_tree, d2_tree],
        "the last split piece and both replayed descendants must keep their original trees"
    );
}

#[test]
fn split_per_file_rejects_a_merge_commit_in_the_replay_range() {
    // Same hazard as reword: a merge between the split commit and HEAD makes
    // the descendant revwalk unreliable, so refuse it with our own message.
    let test = common::TestRepo::new();

    test.commit_files(&[("a.txt", "a\n"), ("b.txt", "b\n")], "base");
    let to_split = test.commit_files(&[("a.txt", "a2\n"), ("b.txt", "b2\n")], "change both");
    let d1 = test.commit_file("c.txt", "c\n", "descendant");

    let sig = git2::Signature::now("Test User", "test@example.com").unwrap();
    let d1_commit = test.repo.find_commit(d1).unwrap();
    let target_commit = test.repo.find_commit(to_split).unwrap();
    let tree = d1_commit.tree().unwrap();
    test.repo
        .commit(
            Some("HEAD"),
            &sig,
            &sig,
            "a merge",
            &tree,
            &[&d1_commit, &target_commit],
        )
        .unwrap();

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();
    let head_before = head_oid.clone();

    let err = git_repo
        .split_commit_per_file(&Oid::from(to_split), &head_oid)
        .expect_err("a merge in the replay range must be refused");
    let msg = err.to_string();

    assert!(
        !msg.contains("mainline"),
        "should not leak libgit2's mainline wording: {msg}"
    );
    assert!(
        msg.to_lowercase().contains("merge"),
        "the error should name the merge commit as the reason: {msg}"
    );
    assert_eq!(
        git_repo.head_oid().unwrap(),
        head_before,
        "the branch must be left untouched"
    );
}
