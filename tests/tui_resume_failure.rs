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

// TUI tests for the conflict dialog after an attempt to resume it failed.

#[allow(dead_code)]
mod common;
use common::TuiTestHarness;

use git_tailor::{
    Oid,
    app::{AppMode, AppState, ResumeFailure},
    repo::ConflictState,
    views,
};

/// With the branch elsewhere, Enter and Esc both refuse until it is back, so
/// the dialog must not claim it is still on the paused commit.
#[test]
fn a_moved_branch_names_the_commit_to_put_it_back_on() {
    let mut harness = TuiTestHarness::typical();
    let mut app = AppState::new();
    app.list.commits = vec![common::create_test_commit("abc123def456", "Add head line")];
    app.mode = AppMode::RebaseConflict(Box::new(ConflictState {
        operation_label: "Drop".to_string(),
        new_tip_oid: Oid::from("c5068579".to_string() + &"0".repeat(32)),
        conflicting_commit_oid: Oid::from("abc123def456"),
        conflicting_files: vec![std::path::PathBuf::from("a.txt")],
        ..Default::default()
    }));
    app.resume_failure = Some(ResumeFailure {
        why: "The branch moved since this was loaded.".to_string(),
        branch_moved: true,
    });

    let buffer = harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::conflict::render_conflict(&mut app, frame);
    });
    let screen: String = buffer.content().iter().map(|cell| cell.symbol()).collect();

    assert!(!screen.contains("on the commit it paused"), "{screen}");
    assert!(screen.contains("c5068579"), "{screen}");
    assert!(screen.contains("next start"), "{screen}");
}
