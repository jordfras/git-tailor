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

/// Assert that a `RebaseOutcome` is `Complete`, panicking with a descriptive
/// message (including the actual value) if it is not.
#[macro_export]
macro_rules! assert_rebase_complete {
    ($outcome:expr) => {{
        let outcome = $outcome;
        assert!(
            matches!(outcome, git_tailor::repo::RebaseOutcome::Complete),
            "expected RebaseOutcome::Complete, got {:?}",
            outcome
        )
    }};
}

/// Extract the `ConflictState` from a `RebaseOutcome::Conflict`, panicking if
/// the outcome is `Complete`.
#[macro_export]
macro_rules! expect_rebase_conflict {
    ($outcome:expr) => {
        match $outcome {
            git_tailor::repo::RebaseOutcome::Conflict(s) => *s,
            git_tailor::repo::RebaseOutcome::Complete => {
                panic!("expected RebaseOutcome::Conflict, got Complete")
            }
        }
    };
}

/// Assert the commit history above `base` matches `expected_summaries` (oldest → newest).
///
/// Panics point to the call site. Checks count then each summary in order.
#[macro_export]
macro_rules! assert_history {
    ($test:expr, $base:expr, $expected:expr) => {{
        let oids = $test.commits_from_head($base);
        let expected: &[&str] = $expected;
        assert_eq!(
            oids.len(),
            expected.len(),
            "expected {} commit(s) above base, got {}",
            expected.len(),
            oids.len()
        );
        for (i, (oid, exp)) in oids.iter().zip(expected.iter()).enumerate() {
            let commit = $test.repo.find_commit(*oid).unwrap();
            let summary = commit.summary().ok().flatten().unwrap_or("");
            assert_eq!(
                summary, *exp,
                "commit[{i}] (oldest-first): expected summary {:?}, got {:?}",
                exp, summary
            );
        }
    }};
}

/// Read a file from a specific commit tree.
///
/// Private helper for [`assert_file_contents!`].
pub fn file_content_at(repo: &git2::Repository, commit_oid: git2::Oid, path: &str) -> String {
    let commit = repo.find_commit(commit_oid).unwrap();
    let tree = commit.tree().unwrap();
    let entry = tree.get_path(std::path::Path::new(path)).unwrap();
    let blob = repo
        .find_blob(entry.id())
        .expect("tree entry should be a blob");
    String::from_utf8_lossy(blob.content()).into_owned()
}

/// Assert the contents of a file at a specific commit match the expected string.
///
/// Includes the file path in the failure message and panics at the call site.
#[macro_export]
macro_rules! assert_file_contents {
    ($repo:expr, $oid:expr, $path:expr, $expected:expr) => {{
        let actual = common::assert::file_content_at($repo, $oid, $path);
        assert_eq!(
            actual, $expected,
            "file {:?} at commit {:?}: expected {:?}, got {:?}",
            $path, $oid, $expected, actual
        );
    }};
}

/// Assert the contents of a file at HEAD match the expected string.
///
/// Includes the file path in the failure message and panics at the call site.
#[macro_export]
macro_rules! assert_file_contents_at_head {
    ($repo:expr, $path:expr, $expected:expr) => {{
        assert_file_contents!(
            $repo,
            $repo.head().unwrap().target().unwrap(),
            $path,
            $expected
        );
    }};
}
