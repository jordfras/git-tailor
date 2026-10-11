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

//! See [`crate::dispatch::listed_tip`].

use super::*;
use crate::dispatch::rewrite::handle_execute_move;
use crate::dispatch::split::{
    handle_execute_split_out_files, handle_execute_split_out_hunks, handle_prepare_split,
};

/// A list loaded from `bbbb…`, while the mock's HEAD is `aaaa…`.
fn stale() -> AppState {
    let mut app = AppState::default();
    app.list.tip = Oid::from("b".repeat(40));
    app
}

fn assert_refused_early(repo: &MockRepo, app: &AppState) {
    assert_eq!(repo.handed_tip, None, "the repository must not be asked");
    assert_eq!(repo.autostash_save_calls.get(), 0, "nothing may be stashed");
    let message = app.status.message.as_deref().unwrap_or_default();
    assert!(message.contains("press R"), "{message:?}");
}

#[test]
fn a_move() {
    let mut repo = MockRepo::default();
    let mut app = stale();
    handle_execute_move(&mut repo, &mut app, Oid::from("c".repeat(40)), None).unwrap();
    assert_refused_early(&repo, &app);
}

/// Another branch on the same commit has the same tip, and the rewrite would
/// land on whichever branch HEAD is on now.
#[test]
fn after_switching_to_another_branch_on_the_same_commit() {
    let mut repo = MockRepo {
        branch: "refs/heads/experiment".to_string(),
        ..Default::default()
    };
    let mut app = AppState::default();
    app.list.tip = Oid::from("a".repeat(40));
    app.list.branch = "refs/heads/main".to_string();
    handle_execute_move(&mut repo, &mut app, Oid::from("c".repeat(40)), None).unwrap();
    assert_refused_early(&repo, &app);
}

/// Unable to tell whether the list is current, the operation does not
/// assume it is.
#[test]
fn when_head_cannot_be_read() {
    let mut repo = MockRepo {
        head_ok: false,
        ..Default::default()
    };
    let mut app = AppState::default();
    handle_execute_move(&mut repo, &mut app, Oid::from("c".repeat(40)), None).unwrap();
    assert_eq!(repo.handed_tip, None, "the repository must not be asked");
    assert!(app.status.is_error);
}

#[test]
fn a_split() {
    let mut repo = MockRepo {
        count_per_file: 20,
        ..Default::default()
    };
    let mut app = stale();
    handle_prepare_split(
        &mut repo,
        &mut app,
        SplitStrategy::PerFile,
        Oid::from("c".repeat(40)),
        &mut NoProgress,
    )
    .unwrap();
    assert_refused_early(&repo, &app);
    assert!(
        !matches!(app.mode, AppMode::SplitConfirm(_)),
        "no confirmation for a split that cannot happen"
    );
}

#[test]
fn splitting_out_files() {
    let mut repo = MockRepo::default();
    let mut app = stale();
    handle_execute_split_out_files(&mut repo, &mut app, Oid::from("c".repeat(40)), vec![]).unwrap();
    assert_refused_early(&repo, &app);
}

#[test]
fn splitting_out_hunks() {
    let mut repo = MockRepo::default();
    let mut app = stale();
    handle_execute_split_out_hunks(&mut repo, &mut app, Oid::from("c".repeat(40)), vec![], 3)
        .unwrap();
    assert_refused_early(&repo, &app);
}

#[test]
fn squashing_a_commit() {
    let mut repo = MockRepo::default();
    let mut app = stale();
    let prepared = prepare_source(&mut repo, &mut app, &commit_source(), "Squash").unwrap();
    assert!(prepared.is_none());
    assert_refused_early(&repo, &app);
}

#[test]
fn folding_a_working_tree_row() {
    let mut repo = MockRepo {
        lift: LiftOutcome::Lifted,
        ..Default::default()
    };
    let mut app = stale();
    let prepared = prepare_source(&mut repo, &mut app, &worktree_source(), "Squash").unwrap();
    assert!(prepared.is_none());
    assert_refused_early(&repo, &app);
}

#[test]
fn an_autofixup() {
    let mut repo = MockRepo::default();
    let mut app = stale();
    crate::dispatch::autofixup::handle_prepare_autofixup_confirm(&mut repo, &mut app).unwrap();
    assert!(!matches!(app.mode, AppMode::AutofixupConfirm(_)));
    assert_refused_early(&repo, &app);
}
