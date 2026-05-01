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
fn auto_stage_resolved_conflicts_stages_externally_edited_file() {
    // Simulates the user resolving a conflict in an external editor (e.g.
    // VS Code) and saving the file without explicitly staging it. The
    // auto_stage_resolved_conflicts method should detect the resolution
    // and stage the file so that rebase_continue succeeds.
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "line1\n", "base");
    let to_drop = test.commit_file("a.txt", "line1\nline2\n", "add line2");
    let head = test.commit_file("a.txt", "line1\nline2\nline3\n", "add line3");

    let git_repo = test.git_repo();
    let result = git_repo
        .drop_commit(&Oid::from(to_drop), &Oid::from(head))
        .unwrap();

    let state = expect_rebase_conflict!(result);

    // Simulate external editor: write resolved content but do NOT stage.
    test.write_file("a.txt", "line1\nline3\n");

    // Index still has conflict entries at this point.
    assert!(
        !git_repo.read_conflicting_files().is_empty(),
        "conflict entries should still be in the index before auto-staging"
    );

    // Auto-stage should detect the file no longer has conflict markers.
    git_repo
        .auto_stage_resolved_conflicts(&state.conflicting_files)
        .unwrap();

    // Conflict entries should be cleared now.
    assert!(
        git_repo.read_conflicting_files().is_empty(),
        "conflict entries should be cleared after auto-staging"
    );

    // rebase_continue should succeed.
    let result = git_repo.rebase_continue(&state).unwrap();
    assert_rebase_complete!(result);

    let commits = test.commits_from_head(base);
    assert_eq!(commits.len(), 1);
    assert_file_contents_at_head!(&test.repo, "a.txt", "line1\nline3\n");
}

#[test]
fn auto_stage_does_not_stage_file_with_conflict_markers() {
    // If the working-tree file still contains conflict markers,
    // auto_stage_resolved_conflicts must leave it alone.
    let test = common::TestRepo::new();

    let _base = test.commit_file("a.txt", "line1\n", "base");
    let to_drop = test.commit_file("a.txt", "line1\nline2\n", "add line2");
    let head = test.commit_file("a.txt", "line1\nline2\nline3\n", "add line3");

    let git_repo = test.git_repo();
    let result = git_repo
        .drop_commit(&Oid::from(to_drop), &Oid::from(head))
        .unwrap();

    let state = expect_rebase_conflict!(result);

    // Working tree already has conflict markers from write_conflicts_to_workdir.
    // auto_stage should NOT clear them.
    git_repo
        .auto_stage_resolved_conflicts(&state.conflicting_files)
        .unwrap();

    assert!(
        !git_repo.read_conflicting_files().is_empty(),
        "conflict entries should still be present when markers remain"
    );
}
