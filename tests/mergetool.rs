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

use git_tailor::{
    mergetool,
    repo::{GitRepo, RebaseOutcome},
};

// ---------------------------------------------------------------------------
// resolve_merge_tool_cmd
// ---------------------------------------------------------------------------

#[test]
fn resolve_merge_tool_cmd_returns_none_when_no_config() {
    let test = common::TestRepo::new();
    // Explicitly override any global merge.tool by setting it to an empty
    // string in the local config. An empty name has no builtin cmd and no
    // mergetool..cmd, so the function must return None.
    test.set_config("merge.tool", "");
    let git_repo = test.git_repo();
    assert!(mergetool::resolve_merge_tool_cmd(&git_repo).is_none());
}

#[test]
fn resolve_merge_tool_cmd_returns_builtin_for_vimdiff() {
    let test = common::TestRepo::new();
    test.set_config("merge.tool", "vimdiff");
    let git_repo = test.git_repo();
    let cmd = mergetool::resolve_merge_tool_cmd(&git_repo).unwrap();
    assert!(
        cmd.contains("vimdiff"),
        "expected vimdiff in cmd, got: {cmd}"
    );
    assert!(cmd.contains("$LOCAL"), "expected $LOCAL placeholder: {cmd}");
    assert!(
        cmd.contains("$MERGED"),
        "expected $MERGED placeholder: {cmd}"
    );
}

#[test]
fn resolve_merge_tool_cmd_returns_builtin_for_kdiff3() {
    let test = common::TestRepo::new();
    test.set_config("merge.tool", "kdiff3");
    let git_repo = test.git_repo();
    let cmd = mergetool::resolve_merge_tool_cmd(&git_repo).unwrap();
    assert!(
        cmd.starts_with("kdiff3"),
        "expected kdiff3 command, got: {cmd}"
    );
    assert!(
        cmd.contains("$MERGED"),
        "expected $MERGED placeholder: {cmd}"
    );
}

#[test]
fn resolve_merge_tool_cmd_returns_builtin_for_meld() {
    let test = common::TestRepo::new();
    test.set_config("merge.tool", "meld");
    let git_repo = test.git_repo();
    let cmd = mergetool::resolve_merge_tool_cmd(&git_repo).unwrap();
    assert_eq!(cmd, "meld $LOCAL $MERGED $REMOTE");
}

#[test]
fn resolve_merge_tool_cmd_prefers_custom_cmd_over_builtin() {
    let test = common::TestRepo::new();
    test.set_config("merge.tool", "vimdiff");
    test.set_config("mergetool.vimdiff.cmd", "my-special-vimdiff $MERGED");
    let git_repo = test.git_repo();
    let cmd = mergetool::resolve_merge_tool_cmd(&git_repo).unwrap();
    assert_eq!(cmd, "my-special-vimdiff $MERGED");
}

#[test]
fn resolve_merge_tool_cmd_returns_custom_cmd_for_unknown_tool() {
    let test = common::TestRepo::new();
    test.set_config("merge.tool", "my-fancy-tool");
    test.set_config(
        "mergetool.my-fancy-tool.cmd",
        "fancy $LOCAL $REMOTE $MERGED",
    );
    let git_repo = test.git_repo();
    let cmd = mergetool::resolve_merge_tool_cmd(&git_repo).unwrap();
    assert_eq!(cmd, "fancy $LOCAL $REMOTE $MERGED");
}

#[test]
fn resolve_merge_tool_cmd_returns_none_for_unknown_tool_without_cmd() {
    let test = common::TestRepo::new();
    test.set_config("merge.tool", "totally-unknown-tool-xyz");
    let git_repo = test.git_repo();
    // No mergetool.<name>.cmd set and not a known builtin → None.
    assert!(mergetool::resolve_merge_tool_cmd(&git_repo).is_none());
}

// ---------------------------------------------------------------------------
// read_index_stage / stage_file / read_conflicting_files (via Git2Repo)
// ---------------------------------------------------------------------------

/// Set up a drop-commit conflict and return the resulting ConflictState.
fn make_conflict(test: &common::TestRepo) -> git_tailor::repo::ConflictState {
    let _base = test.commit_file("a.txt", "base\n", "base");
    let to_drop = test.commit_file("a.txt", "base\ndropped\n", "add dropped line");
    let head = test.commit_file("a.txt", "base\ndropped\nhead\n", "add head line");

    let git_repo = test.git_repo();
    match git_repo
        .drop_commit(&to_drop.to_string(), &head.to_string())
        .unwrap()
    {
        RebaseOutcome::Conflict(state) => state,
        RebaseOutcome::Complete => panic!("expected conflict"),
    }
}

#[test]
fn read_index_stage_returns_none_when_no_conflict_entry() {
    let test = common::TestRepo::new();
    test.commit_file("a.txt", "content\n", "initial");
    let git_repo = test.git_repo();
    // No conflict — stage 2 (ours) for a.txt is a normal stage-0 entry, not a
    // conflict stage. read_index_stage should return None for stages 1–3.
    let result = git_repo.read_index_stage("a.txt", 2).unwrap();
    assert!(
        result.is_none(),
        "expected None for non-conflicted file at stage 2"
    );
}

#[test]
fn read_index_stage_returns_content_after_conflict() {
    let test = common::TestRepo::new();
    let state = make_conflict(&test);
    let git_repo = test.git_repo();

    // Stage 2 = ours (the cherry-pick source). Must have non-empty content.
    let ours = git_repo
        .read_index_stage(&state.conflicting_files[0], 2)
        .unwrap();
    assert!(
        ours.is_some(),
        "stage 2 (ours) should be present after conflict"
    );
    assert!(!ours.unwrap().is_empty());

    // Stage 3 = theirs. Must also be present.
    let theirs = git_repo
        .read_index_stage(&state.conflicting_files[0], 3)
        .unwrap();
    assert!(
        theirs.is_some(),
        "stage 3 (theirs) should be present after conflict"
    );
}

#[test]
fn stage_file_clears_conflict_entries_in_index() {
    let test = common::TestRepo::new();
    let state = make_conflict(&test);
    let git_repo = test.git_repo();

    // Sanity: there is a conflict.
    assert!(!state.conflicting_files.is_empty());
    let path = &state.conflicting_files[0];

    // Write a resolved version of the file to the working tree.
    let workdir = test.repo.workdir().unwrap();
    std::fs::write(workdir.join(path), "resolved content\n").unwrap();

    // Stage it — this is the core of the bug fix.
    git_repo.stage_file(path).unwrap();

    // Index should no longer report this file as conflicted.
    let remaining = git_repo.read_conflicting_files();
    assert!(
        !remaining.contains(path),
        "file should no longer be in conflict after staging: {remaining:?}"
    );
    assert!(
        remaining.is_empty(),
        "index should have no conflicts: {remaining:?}"
    );
}

// ---------------------------------------------------------------------------
// run_for_all_files end-to-end pipeline
// ---------------------------------------------------------------------------

/// Full pipeline test: configure a shell command that resolves by copying LOCAL
/// (the ours-side) to MERGED, then verify the index conflict is cleared and
/// drop_commit_continue can complete successfully.
#[test]
fn run_for_all_files_stages_file_and_clears_conflict() {
    let test = common::TestRepo::new();

    let _base = test.commit_file("a.txt", "base\n", "base");
    let to_drop = test.commit_file("a.txt", "base\ndropped\n", "add dropped line");
    let head = test.commit_file("a.txt", "base\ndropped\nhead\n", "add head line");

    let git_repo = test.git_repo();
    let state = match git_repo
        .drop_commit(&to_drop.to_string(), &head.to_string())
        .unwrap()
    {
        RebaseOutcome::Conflict(s) => s,
        RebaseOutcome::Complete => panic!("expected conflict"),
    };

    assert!(
        !state.conflicting_files.is_empty(),
        "expected conflict files"
    );
    let workdir = git_repo.workdir().unwrap();

    // Use 'cp $LOCAL $MERGED' as the "merge tool" — takes the ours-side content.
    let cmd = "cp $LOCAL $MERGED";
    mergetool::run_for_all_files(cmd, &workdir, &git_repo, &state.conflicting_files)
        .expect("run_for_all_files should succeed");

    // Conflict must be cleared in the index.
    let remaining = git_repo.read_conflicting_files();
    assert!(
        remaining.is_empty(),
        "index should have no conflicts after tool run: {remaining:?}"
    );

    // drop_commit_continue must now complete without another conflict.
    let refreshed_state = git_tailor::repo::ConflictState {
        conflicting_files: git_repo.read_conflicting_files(),
        still_unresolved: false,
        ..state
    };
    let outcome = git_repo.drop_commit_continue(&refreshed_state).unwrap();
    assert!(
        matches!(outcome, RebaseOutcome::Complete),
        "expected Complete after resolution, got {outcome:?}"
    );
}
