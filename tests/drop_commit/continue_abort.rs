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
fn drop_continue_after_resolving_conflict() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "line1\n", "base");
    let to_drop = test.commit_file("a.txt", "line1\nline2\n", "add line2");
    let head = test.commit_file("a.txt", "line1\nline2\nline3\n", "add line3");

    let git_repo = test.git_repo();
    let result = git_repo
        .drop_commit(&Oid::from(to_drop), &Oid::from(head))
        .unwrap();

    let state = expect_rebase_conflict!(result);

    // Simulate user resolving the conflict: write the resolved content,
    // clear conflict entries, then stage the file.
    test.write_file("a.txt", "line1\nline3\n");
    let mut index = test.repo.index().unwrap();
    index
        .conflict_remove(std::path::Path::new("a.txt"))
        .unwrap();
    index.add_path(std::path::Path::new("a.txt")).unwrap();
    index.write().unwrap();

    let result = git_repo.rebase_continue(&state).unwrap();
    assert_rebase_complete!(result);

    let commits = test.commits_from_head(base);
    assert_eq!(commits.len(), 1);

    assert_file_contents_at_head!(&test.repo, "a.txt", "line1\nline3\n");
}

#[test]
fn drop_continue_with_unresolved_conflicts_stays_in_conflict_mode() {
    // Calling rebase_continue when the index still has conflict markers
    // must return Conflict (same OIDs, refreshed file list) instead of an
    // error — leaving the repo in a usable state so the user can keep
    // editing or abort.
    let test = common::TestRepo::new();
    let state = test.make_drop_conflict();
    let git_repo = test.git_repo();

    // Do NOT resolve the conflict — just call continue immediately.
    let result = git_repo.rebase_continue(&state).unwrap();

    match result {
        RebaseOutcome::Conflict(new_state) => {
            // Same commit still conflicting.
            assert_eq!(
                new_state.conflicting_commit_oid,
                state.conflicting_commit_oid
            );
            assert_eq!(new_state.original_branch_oid, state.original_branch_oid);
            assert_eq!(new_state.remaining_oids, state.remaining_oids);
            // File list must show the still-unresolved file.
            assert!(
                !new_state.conflicting_files.is_empty(),
                "conflicting_files should be populated"
            );
            assert!(
                new_state.conflicting_files.iter().any(|f| f == "a.txt"),
                "a.txt should be listed as conflicting, got {:?}",
                new_state.conflicting_files
            );
            // The flag must be set so the dialog can warn the user.
            assert!(
                new_state.still_unresolved,
                "still_unresolved should be true when continuing with unresolved conflicts"
            );
        }
        RebaseOutcome::Complete => panic!("expected Conflict since index was not resolved"),
    }

    // Repo must still be in a state where abort works cleanly.
    git_repo.rebase_abort(&state).unwrap();
    let restored = test.repo.head().unwrap().target().unwrap();
    assert_eq!(
        Oid::from(restored),
        state.original_branch_oid,
        "abort should restore original HEAD"
    );
}

#[test]
fn drop_abort_restores_original_branch() {
    let test = common::TestRepo::new();
    let state = test.make_drop_conflict();
    let git_repo = test.git_repo();

    git_repo.rebase_abort(&state).unwrap();

    // Branch should be back to the original HEAD.
    let current_head = test.repo.head().unwrap().target().unwrap();
    assert_eq!(
        Oid::from(current_head),
        state.original_branch_oid,
        "HEAD should be restored to original after abort"
    );

    assert_file_contents_at_head!(&test.repo, "a.txt", "base\ndropped\nhead\n");
}
