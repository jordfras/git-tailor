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
    ($repo:expr, $base:expr, $expected:expr) => {{
        let oids = common::commits_from_head($repo, $base);
        let expected: &[&str] = $expected;
        assert_eq!(
            oids.len(),
            expected.len(),
            "expected {} commit(s) above base, got {}",
            expected.len(),
            oids.len()
        );
        for (i, (oid, exp)) in oids.iter().zip(expected.iter()).enumerate() {
            let commit = $repo.find_commit(*oid).unwrap();
            let summary = commit.summary().unwrap_or("");
            assert_eq!(
                summary, *exp,
                "commit[{i}] (oldest-first): expected summary {:?}, got {:?}",
                exp, summary
            );
        }
    }};
}
