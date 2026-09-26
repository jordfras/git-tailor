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

//! Splitting commits whose changes include some with no hunks: a binary file,
//! an empty file, a mode change.

use crate::common;
use crate::common::prelude::*;

/// What each commit from `base` up to HEAD changes, oldest first: a path for a
/// content change, the path with " +x" for a mode change.
fn changes_per_commit(test: &common::TestRepo, base: git2::Oid) -> Vec<Vec<String>> {
    let mut before = test.repo.find_commit(base).unwrap().tree().unwrap();
    test.commits_from_head(base)
        .into_iter()
        .map(|oid| {
            let after = test.repo.find_commit(oid).unwrap().tree().unwrap();
            let diff = test
                .repo
                .diff_tree_to_tree(Some(&before), Some(&after), None)
                .unwrap();
            let mut changes = Vec::new();
            for delta in diff.deltas() {
                let path = delta
                    .new_file()
                    .path()
                    .or(delta.old_file().path())
                    .unwrap()
                    .display()
                    .to_string();
                if delta.old_file().id() != delta.new_file().id() {
                    changes.push(path.clone());
                }
                if delta.old_file().mode() != delta.new_file().mode()
                    && delta.status() == git2::Delta::Modified
                {
                    changes.push(format!("{path} +x"));
                }
            }
            before = after;
            changes
        })
        .collect()
}

#[test]
fn split_per_file_handles_a_binary_file_that_is_not_last() {
    let test = common::TestRepo::new();
    let base = test.commit_files(
        &[("a.txt", "a1\n"), ("bin.dat", "\0old\0"), ("z.txt", "z1\n")],
        "base",
    );
    let to_split = test.commit_files(
        &[("a.txt", "a2\n"), ("bin.dat", "\0new\0"), ("z.txt", "z2\n")],
        "change",
    );

    let mut git_repo = test.git_repo();
    git_repo
        .split_commit_per_file(&Oid::from(to_split), &Oid::from(to_split))
        .unwrap();

    assert_eq!(
        changes_per_commit(&test, base),
        [vec!["a.txt"], vec!["bin.dat"], vec!["z.txt"]]
    );
    assert_eq!(test.head_tree_id(), test.tree_id(to_split));
}
