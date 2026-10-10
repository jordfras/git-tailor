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

// TUI tests for the conflict dialog of a paused autofixup batch.

#[allow(dead_code)]
mod common;
use common::TuiTestHarness;

use git_tailor::{
    Oid,
    app::{AppMode, AppState},
    repo::{ConflictState, Resume, SquashContext},
    views,
};

/// The batch is the operation the user started, so the dialog names it — and
/// still says the squash step is what conflicted.
#[test]
fn an_autofixup_squash_conflict_names_the_batch_and_the_squash() {
    let mut harness = TuiTestHarness::typical();
    let mut app = AppState::new();
    app.list.commits = vec![common::create_test_commit("abc123def456", "Add T2")];
    app.mode = AppMode::RebaseConflict(Box::new(ConflictState {
        operation_label: "Autofixup".to_string(),
        conflicting_commit_oid: Oid::from("abc123def456"),
        conflicting_files: vec![std::path::PathBuf::from("c.txt")],
        resume: Resume::Squash(SquashContext::default()),
        ..Default::default()
    }));

    let buffer = harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::conflict::render_conflict(&mut app, frame);
    });
    let screen: String = buffer.content().iter().map(|cell| cell.symbol()).collect();

    assert!(screen.contains("Autofixup Conflict"), "{screen}");
    assert!(
        screen.contains("The squash itself caused the conflict."),
        "{screen}"
    );
}
