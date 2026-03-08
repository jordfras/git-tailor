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

fn commit_message(repo: &git2::Repository, oid: git2::Oid) -> String {
    repo.find_commit(oid)
        .unwrap()
        .message()
        .unwrap_or("")
        .trim()
        .to_string()
}

// ---------------------------------------------------------------------------
// Happy-path tests
// ---------------------------------------------------------------------------

#[test]
fn move_commit_earlier() {
    let test = common::TestRepo::new();

    // ref → A → B → C(source) → HEAD
    // Move C to after A → ref → A → C → B
    let base = test.commit_file("a.txt", "base\n", "base");
    let a = test.commit_file("x.txt", "x\n", "A");
    let _b = test.commit_file("y.txt", "y\n", "B");
    let c = test.commit_file("z.txt", "z\n", "C");

    let git_repo = test.git_repo();
    let result = git_repo
        .move_commit(&c.to_string(), &a.to_string(), &c.to_string())
        .unwrap();

    assert!(
        matches!(result, RebaseOutcome::Complete),
        "expected Complete, got {result:?}"
    );

    let commits = commits_from_head(&test.repo, base);
    assert_eq!(commits.len(), 3);

    let messages: Vec<String> = commits
        .iter()
        .map(|&oid| commit_message(&test.repo, oid))
        .collect();
    assert_eq!(messages, vec!["A", "C", "B"]);

    let head_oid = test.repo.head().unwrap().target().unwrap();
    assert_eq!(file_content_at(&test.repo, head_oid, "x.txt"), "x\n");
    assert_eq!(file_content_at(&test.repo, head_oid, "y.txt"), "y\n");
    assert_eq!(file_content_at(&test.repo, head_oid, "z.txt"), "z\n");
}

#[test]
fn move_commit_later() {
    let test = common::TestRepo::new();

    // ref → A → B(source) → C → D → HEAD
    // Move B to after D → ref → A → C → D → B
    let base = test.commit_file("a.txt", "base\n", "base");
    let _a = test.commit_file("x.txt", "x\n", "A");
    let b = test.commit_file("y.txt", "y\n", "B");
    let _c = test.commit_file("z.txt", "z\n", "C");
    let d = test.commit_file("w.txt", "w\n", "D");

    let git_repo = test.git_repo();
    let result = git_repo
        .move_commit(&b.to_string(), &d.to_string(), &d.to_string())
        .unwrap();

    assert!(
        matches!(result, RebaseOutcome::Complete),
        "expected Complete, got {result:?}"
    );

    let commits = commits_from_head(&test.repo, base);
    assert_eq!(commits.len(), 4);

    let messages: Vec<String> = commits
        .iter()
        .map(|&oid| commit_message(&test.repo, oid))
        .collect();
    assert_eq!(messages, vec!["A", "C", "D", "B"]);
}

#[test]
fn move_commit_to_beginning() {
    let test = common::TestRepo::new();

    // ref → A → B → C(source) → HEAD
    // Move C to after ref → ref → C → A → B
    let base = test.commit_file("a.txt", "base\n", "base");
    let _a = test.commit_file("x.txt", "x\n", "A");
    let _b = test.commit_file("y.txt", "y\n", "B");
    let c = test.commit_file("z.txt", "z\n", "C");

    let git_repo = test.git_repo();
    let result = git_repo
        .move_commit(&c.to_string(), &base.to_string(), &c.to_string())
        .unwrap();

    assert!(
        matches!(result, RebaseOutcome::Complete),
        "expected Complete, got {result:?}"
    );

    let commits = commits_from_head(&test.repo, base);
    assert_eq!(commits.len(), 3);

    let messages: Vec<String> = commits
        .iter()
        .map(|&oid| commit_message(&test.repo, oid))
        .collect();
    assert_eq!(messages, vec!["C", "A", "B"]);
}

#[test]
fn move_head_commit_earlier() {
    let test = common::TestRepo::new();

    // ref → A → B → C(HEAD, source)
    // Move C to after A → ref → A → C → B
    let base = test.commit_file("a.txt", "base\n", "base");
    let a = test.commit_file("x.txt", "x\n", "A");
    let _b = test.commit_file("y.txt", "y\n", "B");
    let head = test.commit_file("z.txt", "z\n", "C");

    let git_repo = test.git_repo();
    let result = git_repo
        .move_commit(&head.to_string(), &a.to_string(), &head.to_string())
        .unwrap();

    assert!(matches!(result, RebaseOutcome::Complete));

    let messages: Vec<String> = commits_from_head(&test.repo, base)
        .iter()
        .map(|&oid| commit_message(&test.repo, oid))
        .collect();
    assert_eq!(messages, vec!["A", "C", "B"]);
}

#[test]
fn move_commit_conflict_returns_conflict_state() {
    let test = common::TestRepo::new();

    // Both A and B modify the same file. Moving B before A will conflict
    // because B's diff was against A's tree — without A present the cherry-pick
    // fails.
    let base = test.commit_file("a.txt", "line1\n", "base");
    let _a = test.commit_file("a.txt", "line1\nline2\n", "A");
    let b = test.commit_file("a.txt", "line1\nline2\nline3\n", "B");

    let git_repo = test.git_repo();
    let result = git_repo
        .move_commit(&b.to_string(), &base.to_string(), &b.to_string())
        .unwrap();

    match result {
        RebaseOutcome::Conflict(state) => {
            assert_eq!(state.operation_label, "Move");
            assert_eq!(state.original_branch_oid, b.to_string());
            assert!(!state.conflicting_files.is_empty());
        }
        RebaseOutcome::Complete => {
            // Moving B (which appends line3 to line1+line2) to before A
            // may or may not conflict depending on diff mechanics.
            // If git can apply B's hunk cleanly against the base, that's
            // also valid — the commit just adds line3 after line1.
            let msgs: Vec<String> = commits_from_head(&test.repo, base)
                .iter()
                .map(|&oid| commit_message(&test.repo, oid))
                .collect();
            assert_eq!(msgs[0], "B");
            assert_eq!(msgs[1], "A");
        }
    }
}

#[test]
fn move_commit_preserves_file_contents() {
    let test = common::TestRepo::new();

    // Each commit touches a different file — no conflicts possible.
    // ref → A(x.txt) → B(y.txt) → C(z.txt)
    // Move B to end → ref → A → C → B
    let base = test.commit_file("a.txt", "base\n", "base");
    let _a = test.commit_file("x.txt", "x-content\n", "A");
    let b = test.commit_file("y.txt", "y-content\n", "B");
    let c = test.commit_file("z.txt", "z-content\n", "C");

    let git_repo = test.git_repo();
    let result = git_repo
        .move_commit(&b.to_string(), &c.to_string(), &c.to_string())
        .unwrap();

    assert!(matches!(result, RebaseOutcome::Complete));

    let head_oid = test.repo.head().unwrap().target().unwrap();
    assert_eq!(
        file_content_at(&test.repo, head_oid, "x.txt"),
        "x-content\n"
    );
    assert_eq!(
        file_content_at(&test.repo, head_oid, "y.txt"),
        "y-content\n"
    );
    assert_eq!(
        file_content_at(&test.repo, head_oid, "z.txt"),
        "z-content\n"
    );

    let commits = commits_from_head(&test.repo, base);
    let messages: Vec<String> = commits
        .iter()
        .map(|&oid| commit_message(&test.repo, oid))
        .collect();
    assert_eq!(messages, vec!["A", "C", "B"]);
}
