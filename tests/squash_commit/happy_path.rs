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
fn squash_adjacent_commits_source_is_head() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "base\n", "base");
    let target = test.commit_file("a.txt", "target\n", "target commit");
    let source = test.commit_file("b.txt", "source\n", "source commit");

    let git_repo = test.git_repo();
    let result = git_repo
        .squash_commits(
            &Oid::from(source),
            &Oid::from(target),
            "squashed message",
            &Oid::from(source),
        )
        .unwrap();

    assert_rebase_complete!(result);

    assert_history!(&test, base, &["squashed message"]);

    assert_file_contents_at_head!(&test.repo, "a.txt", "target\n");
    assert_file_contents_at_head!(&test.repo, "b.txt", "source\n");
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
            &Oid::from(source),
            &Oid::from(target),
            "squashed",
            &Oid::from(source),
        )
        .unwrap();

    assert_rebase_complete!(result);

    assert_history!(&test, base, &["squashed", "middle commit"]);

    // Final tree should have all three files
    assert_file_contents_at_head!(&test.repo, "a.txt", "target\n");
    assert_file_contents_at_head!(&test.repo, "b.txt", "source\n");
    assert_file_contents_at_head!(&test.repo, "c.txt", "middle\n");

    // Verify middle was properly squash-excluded
    let commits = test.commits_from_head(base);
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
            &Oid::from(source),
            &Oid::from(target),
            "squashed",
            &Oid::from(after),
        )
        .unwrap();

    assert_rebase_complete!(result);

    assert_history!(&test, base, &["squashed", "after commit"]);

    assert_file_contents_at_head!(&test.repo, "a.txt", "target\n");
    assert_file_contents_at_head!(&test.repo, "b.txt", "source\n");
    assert_file_contents_at_head!(&test.repo, "c.txt", "after\n");
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
            &Oid::from(source),
            &Oid::from(target),
            custom_message,
            &Oid::from(source),
        )
        .unwrap();

    let commits = test.commits_from_head(base);
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
            &Oid::from(source),
            &Oid::from(target),
            "squashed",
            &Oid::from(source),
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
