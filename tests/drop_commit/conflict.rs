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
use git_tailor::{
    Oid,
    repo::{GitRepo, RebaseOutcome},
};

#[test]
fn drop_returns_conflict_when_descendant_depends_on_dropped_commit() {
    let test = common::TestRepo::new();

    let _base = test.commit_file("a.txt", "line1\n", "base");
    let to_drop = test.commit_file("a.txt", "line1\nline2\n", "add line2");
    // This commit modifies the same area that to_drop introduced, so dropping
    // to_drop will conflict when rebasing this descendant.
    let head = test.commit_file("a.txt", "line1\nline2\nline3\n", "add line3");

    let git_repo = test.git_repo();
    let result = git_repo
        .drop_commit(&Oid::from(to_drop), &Oid::from(head))
        .unwrap();

    match result {
        RebaseOutcome::Conflict(state) => {
            assert_eq!(state.conflicting_commit_oid, Oid::from(head));
            assert!(
                state.remaining_oids.is_empty(),
                "no commits after the conflicting one"
            );
            assert_eq!(state.original_branch_oid, Oid::from(head));
        }
        RebaseOutcome::Complete => panic!("expected Conflict, got Complete"),
    }
}

#[test]
fn drop_conflict_state_has_correct_remaining_oids() {
    let test = common::TestRepo::new();

    let _base = test.commit_file("a.txt", "v1\n", "base");
    let to_drop = test.commit_file("a.txt", "v2\n", "change a");
    // First descendant will conflict (depends on dropped change).
    let child1 = test.commit_file("a.txt", "v3\n", "change a again");
    let child2 = test.commit_file("b.txt", "b1\n", "add b");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();
    let result = git_repo
        .drop_commit(&Oid::from(to_drop), &head_oid)
        .unwrap();

    match result {
        RebaseOutcome::Conflict(state) => {
            assert_eq!(state.conflicting_commit_oid, Oid::from(child1));
            assert_eq!(state.remaining_oids, vec![Oid::from(child2)]);
        }
        RebaseOutcome::Complete => panic!("expected Conflict, got Complete"),
    }
}
