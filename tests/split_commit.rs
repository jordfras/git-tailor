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

use git_tailor::repo::GitRepo;

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
        .split_commit_per_file(&to_split.to_string(), &head_oid)
        .unwrap();

    // There should now be 3 commits above base: base, split-1, split-2
    let commits_above_base = commits_from_head(&test.repo, base);
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
    assert_eq!(file_content_at(&test.repo, split2_oid, "a.txt"), "alpha2\n");
    assert_eq!(file_content_at(&test.repo, split2_oid, "b.txt"), "beta2\n");
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
        .split_commit_per_file(&to_split.to_string(), &head_oid)
        .unwrap();

    // We should now have: base → split1 → split2 → rebased-c
    let commits_above_base = commits_from_head(&test.repo, base);
    assert_eq!(
        commits_above_base.len(),
        3,
        "expected 2 split commits + 1 rebased descendant"
    );

    // Rebased descendant should still have the c.txt content
    let rebased_tip = *commits_above_base.last().unwrap();
    assert_eq!(file_content_at(&test.repo, rebased_tip, "c.txt"), "gamma\n");
    // And all files from the split should also be present
    assert_eq!(
        file_content_at(&test.repo, rebased_tip, "a.txt"),
        "alpha2\n"
    );
    assert_eq!(file_content_at(&test.repo, rebased_tip, "b.txt"), "beta2\n");
}

#[test]
fn split_per_file_refuses_single_file_commit() {
    let test = common::TestRepo::new();
    test.commit_file("a.txt", "alpha\n", "base");
    let only_one = test.commit_file("a.txt", "alpha2\n", "single file");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    let result = git_repo.split_commit_per_file(&only_one.to_string(), &head_oid);
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
    let repo_path = test.repo.workdir().unwrap();
    std::fs::write(repo_path.join("a.txt"), "DIRTY\n").unwrap();
    let mut index = test.repo.index().unwrap();
    index.add_path(std::path::Path::new("a.txt")).unwrap();
    index.write().unwrap();

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    let result = git_repo.split_commit_per_file(&to_split.to_string(), &head_oid);
    assert!(result.is_err(), "should fail when staged changes overlap");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("a.txt"),
        "error should mention the overlapping file, got: {}",
        msg
    );
}

// ---------------------------------------------------------------------------
// Per-hunk split tests
// ---------------------------------------------------------------------------

#[test]
fn split_per_hunk_single_file_two_hunks() {
    let test = common::TestRepo::new();

    // Base commit: file with two separate regions.
    let base = test.commit_file(
        "a.txt",
        "line1\nline2\nline3\nPAD1\nPAD2\nPAD3\nPAD4\nPAD5\nline6\nline7\nline8\n",
        "base",
    );

    // Commit that changes line1 AND line6 — produces two separate hunks
    // (with 0 context and enough padding between them).
    let to_split = test.commit_file(
        "a.txt",
        "LINE1\nline2\nline3\nPAD1\nPAD2\nPAD3\nPAD4\nPAD5\nLINE6\nline7\nline8\n",
        "two independent changes",
    );

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    git_repo
        .split_commit_per_hunk(&to_split.to_string(), &head_oid)
        .unwrap();

    // Should now have 2 commits above base
    let commits_above_base = commits_from_head(&test.repo, base);
    assert_eq!(
        commits_above_base.len(),
        2,
        "expected 2 split commits above base"
    );

    let split1 = test.repo.find_commit(commits_above_base[0]).unwrap();
    let split2 = test.repo.find_commit(commits_above_base[1]).unwrap();

    assert!(
        split1.summary().unwrap_or("").contains("(1/2)"),
        "expected (1/2) in first split commit, got: {}",
        split1.summary().unwrap_or("")
    );
    assert!(
        split2.summary().unwrap_or("").contains("(2/2)"),
        "expected (2/2) in second split commit, got: {}",
        split2.summary().unwrap_or("")
    );

    // Final content is intact at HEAD
    let tip = commits_above_base[1];
    assert_eq!(
        file_content_at(&test.repo, tip, "a.txt"),
        "LINE1\nline2\nline3\nPAD1\nPAD2\nPAD3\nPAD4\nPAD5\nLINE6\nline7\nline8\n"
    );
}

#[test]
fn split_per_hunk_two_files_one_hunk_each() {
    let test = common::TestRepo::new();

    let base = test.commit_files(&[("a.txt", "alpha\n"), ("b.txt", "beta\n")], "base");

    let to_split = test.commit_files(
        &[("a.txt", "alpha2\n"), ("b.txt", "beta2\n")],
        "change both",
    );

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    git_repo
        .split_commit_per_hunk(&to_split.to_string(), &head_oid)
        .unwrap();

    let commits_above_base = commits_from_head(&test.repo, base);
    assert_eq!(
        commits_above_base.len(),
        2,
        "expected 2 split commits (one per file's hunk)"
    );

    // The final tip contains both file changes
    let tip = *commits_above_base.last().unwrap();
    assert_eq!(file_content_at(&test.repo, tip, "a.txt"), "alpha2\n");
    assert_eq!(file_content_at(&test.repo, tip, "b.txt"), "beta2\n");
}

#[test]
fn split_per_hunk_refuses_single_hunk_commit() {
    let test = common::TestRepo::new();
    test.commit_file("a.txt", "alpha\n", "base");
    let only_one = test.commit_file("a.txt", "alpha2\n", "single hunk");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    let result = git_repo.split_commit_per_hunk(&only_one.to_string(), &head_oid);
    assert!(result.is_err(), "should fail when commit has only 1 hunk");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("fewer than 2 hunks"),
        "unexpected error message: {}",
        msg
    );
}
