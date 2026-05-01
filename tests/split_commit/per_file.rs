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
    let msg1 = split1.summary().unwrap_or("");
    let msg2 = split2.summary().unwrap_or("");
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
