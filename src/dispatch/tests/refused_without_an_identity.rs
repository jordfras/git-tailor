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

//! See `RepoWrite::refuse_if_cannot_commit`: refused before the editor, the
//! picker or the stash, and only by what would write a commit.

use super::*;
use crate::dispatch::autofixup::handle_prepare_autofixup_confirm;
use crate::dispatch::conflict::refuse_resume;
use crate::dispatch::split::{handle_prepare_split_out_files, handle_prepare_split_out_hunks};
use git_tailor::repo::{ConflictState, Resume};

fn without_identity() -> MockRepo {
    MockRepo {
        no_identity: true,
        commit_diff: Some(three_hunk_commit_diff()),
        ..MockRepo::default()
    }
}

/// A list loaded from the mock's HEAD, so only the identity can refuse.
fn listed() -> AppState {
    let mut app = AppState::default();
    app.list.tip = Oid::from("a".repeat(40));
    app.list.branch = "refs/heads/main".to_string();
    app
}

fn assert_refused(repo: &MockRepo, app: &AppState) {
    assert_eq!(app.mode, AppMode::CommitList, "no editor, picker or dialog");
    assert_eq!(repo.autostash_save_calls.get(), 0, "nothing may be stashed");
    let message = app.status.message.as_deref().unwrap_or_default();
    assert!(message.contains("user.name"), "{message:?}");
}

#[test]
fn drop_is_refused_before_stashing() {
    let mut repo = without_identity();
    let mut app = listed();
    handle_execute_drop(
        &mut repo,
        &mut app,
        Oid::from("c".repeat(40)),
        Oid::from("a".repeat(40)),
    )
    .unwrap();
    assert_refused(&repo, &app);
}

/// For a split per hunk group, the count is a fragmap computation.
#[test]
fn split_is_refused_before_counting_or_confirming() {
    let mut repo = MockRepo {
        count_per_file: SPLIT_CONFIRM_THRESHOLD + 1,
        ..without_identity()
    };
    let mut app = listed();
    handle_prepare_split(
        &mut repo,
        &mut app,
        SplitStrategy::PerFile,
        Oid::from("c".repeat(40)),
        &mut NoProgress,
    )
    .unwrap();
    assert_refused(&repo, &app);
}

#[test]
fn split_out_files_is_refused_before_the_picker() {
    let mut repo = without_identity();
    let mut app = listed();
    handle_prepare_split_out_files(&mut repo, &mut app, Oid::from("c".repeat(40))).unwrap();
    assert_refused(&repo, &app);
}

#[test]
fn split_out_hunks_is_refused_before_the_picker() {
    let mut repo = without_identity();
    let mut app = listed();
    handle_prepare_split_out_hunks(&mut repo, &mut app, Oid::from("c".repeat(40)), 3).unwrap();
    assert_refused(&repo, &app);
}

#[test]
fn autofixup_is_refused_before_its_dialog() {
    let mut repo = without_identity();
    let mut app = listed();
    handle_prepare_autofixup_confirm(&mut repo, &mut app).unwrap();
    assert_refused(&repo, &app);
}

/// The conflict is still journaled, so the dialog has to come back: without
/// it there is no way to continue or abort short of restarting.
#[test]
fn refused_conflict_continue_reopens_the_dialog() {
    let repo = without_identity();
    let mut app = listed();

    let resumed = refuse_resume(&repo, &mut app, make_conflict_state());

    assert!(resumed.is_none());
    assert!(matches!(app.mode, AppMode::RebaseConflict(_)));
    let why = &app.resume_failure.as_ref().unwrap().why;
    assert!(why.contains("user.name"), "{why}");
}

/// What is left of a carry conflict settles the working tree and writes no
/// commit; refusing it would leave only the abort, which rewinds a fold that
/// already landed.
#[test]
fn carry_conflict_continue_needs_no_identity() {
    let repo = without_identity();
    let mut app = listed();
    let state = ConflictState {
        resume: Resume::CarryRow(crate::mock_repo::mock_lifted_row()),
        ..make_conflict_state()
    };

    assert!(refuse_resume(&repo, &mut app, state).is_some());
}

#[test]
fn undo_needs_no_identity() {
    let mut repo = MockRepo {
        undo_skips_autostash: true,
        ..without_identity()
    };
    let mut app = AppState::default();
    handle_undo(&mut repo, &mut app).unwrap();
    assert_eq!(app.status.message.as_deref(), Some("Undid stage all"));
}
