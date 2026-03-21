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

#[test]
fn test_merge_base_with_branch_name() {
    let test = common::TestRepo::new();

    let c1 = test.commit_file("file.txt", "initial", "Initial commit");
    let _c2 = test.commit_file("file.txt", "on main", "Main commit");

    test.create_branch("feature", c1);
    test.checkout("refs/heads/feature");
    let _c3 = test.commit_file("feature.txt", "feature work", "Feature commit");

    let merge_base = test
        .repo
        .merge_base(
            test.repo.refname_to_id("refs/heads/feature").unwrap(),
            test.repo.refname_to_id("refs/heads/master").unwrap(),
        )
        .unwrap();

    assert_eq!(merge_base, c1);
}

#[test]
fn test_merge_base_with_tag() {
    let test = common::TestRepo::new();

    let c1 = test.commit_file("file.txt", "v1", "Version 1");
    test.create_tag("v1.0", c1);

    let c2 = test.commit_file("file.txt", "v2", "Version 2");

    let head_oid = test.repo.head().unwrap().target().unwrap();
    let tag_obj = test.repo.revparse_single("v1.0").unwrap();
    // Peel the tag to get the commit it points to
    let tag_commit_oid = tag_obj.peel_to_commit().unwrap().id();

    let merge_base = test.repo.merge_base(head_oid, tag_commit_oid).unwrap();

    assert_eq!(merge_base, c1);
    assert_eq!(head_oid, c2);
}

#[test]
fn test_merge_base_with_short_hash() {
    let test = common::TestRepo::new();

    let c1 = test.commit_file("file.txt", "base", "Base commit");
    let c1_short = &c1.to_string()[..7];

    let c2 = test.commit_file("file.txt", "next", "Next commit");

    let resolved = test.repo.revparse_single(c1_short).unwrap();
    assert_eq!(resolved.id(), c1);

    let head_oid = test.repo.head().unwrap().target().unwrap();
    let merge_base = test.repo.merge_base(head_oid, resolved.id()).unwrap();

    assert_eq!(merge_base, c1);
    assert_eq!(head_oid, c2);
}

#[test]
fn test_merge_base_with_long_hash() {
    let test = common::TestRepo::new();

    let c1 = test.commit_file("file.txt", "base", "Base commit");
    let c1_long = c1.to_string();

    let c2 = test.commit_file("file.txt", "next", "Next commit");

    let resolved = test.repo.revparse_single(&c1_long).unwrap();
    assert_eq!(resolved.id(), c1);

    let head_oid = test.repo.head().unwrap().target().unwrap();
    let merge_base = test.repo.merge_base(head_oid, resolved.id()).unwrap();

    assert_eq!(merge_base, c1);
    assert_eq!(head_oid, c2);
}

#[test]
fn test_merge_base_same_commit() {
    let test = common::TestRepo::new();

    let c1 = test.commit_file("file.txt", "content", "Commit");

    let merge_base = test.repo.merge_base(c1, c1).unwrap();
    assert_eq!(merge_base, c1);
}

#[test]
fn test_merge_base_with_diverged_branches() {
    let test = common::TestRepo::new();

    let base = test.commit_file("file.txt", "base", "Base");

    test.create_branch("branch-a", base);
    test.checkout("refs/heads/branch-a");
    let _a1 = test.commit_file("a.txt", "a1", "A1");
    let _a2 = test.commit_file("a.txt", "a2", "A2");

    test.checkout("refs/heads/master");
    test.create_branch("branch-b", base);
    test.checkout("refs/heads/branch-b");
    let _b1 = test.commit_file("b.txt", "b1", "B1");
    let _b2 = test.commit_file("b.txt", "b2", "B2");

    let a_oid = test.repo.refname_to_id("refs/heads/branch-a").unwrap();
    let b_oid = test.repo.refname_to_id("refs/heads/branch-b").unwrap();

    let merge_base = test.repo.merge_base(a_oid, b_oid).unwrap();
    assert_eq!(merge_base, base);
}

// ---------------------------------------------------------------------------
// default_branch tests
// ---------------------------------------------------------------------------

/// Helper: register a fake remote and point refs/remotes/origin/HEAD at the
/// given branch tracking ref.  No network activity; everything is local.
fn set_origin_head(repo: &git2::Repository, branch: &str) {
    // Create the tracking branch ref so origin/HEAD has a valid target.
    let tracking_refname = format!("refs/remotes/origin/{branch}");
    let head_oid = repo.head().unwrap().target().unwrap();
    repo.reference(&tracking_refname, head_oid, true, "test setup")
        .unwrap();

    // Create refs/remotes/origin/HEAD as a symbolic ref pointing at the
    // tracking branch.
    let symbolic_target = tracking_refname.clone();
    repo.reference_symbolic(
        "refs/remotes/origin/HEAD",
        &symbolic_target,
        true,
        "test setup",
    )
    .unwrap();
}

#[test]
fn default_branch_returns_origin_head_when_set() {
    let test = common::TestRepo::new();
    test.commit_file("a.txt", "content", "initial");
    set_origin_head(&test.repo, "main");

    let git_repo = test.git_repo();
    assert_eq!(git_repo.default_branch(), Some("origin/main".to_string()));
}

#[test]
fn default_branch_returns_none_when_origin_head_absent() {
    let test = common::TestRepo::new();
    test.commit_file("a.txt", "content", "initial");
    // No remote configured at all.
    let git_repo = test.git_repo();
    assert_eq!(git_repo.default_branch(), None);
}

#[test]
fn default_branch_reflects_non_main_default() {
    let test = common::TestRepo::new();
    test.commit_file("a.txt", "content", "initial");
    set_origin_head(&test.repo, "master");

    let git_repo = test.git_repo();
    assert_eq!(git_repo.default_branch(), Some("origin/master".to_string()));
}
