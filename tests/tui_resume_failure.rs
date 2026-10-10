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

/// The dialog's text as one line: what lies between its side borders on each
/// row, so a sentence reads whole wherever it wrapped.
fn dialog_text(buffer: &ratatui::buffer::Buffer) -> String {
    let width = buffer.area.width as usize;
    let cells = buffer.content();
    let mut words = Vec::new();
    for row in cells.chunks(width) {
        let row: String = row.iter().map(|cell| cell.symbol()).collect();
        let (Some(start), Some(end)) = (row.find('│'), row.rfind('│')) else {
            continue;
        };
        if start < end {
            words.extend(
                row[start + '│'.len_utf8()..end]
                    .split_whitespace()
                    .map(str::to_string),
            );
        }
    }
    words.join(" ")
}

/// With the branch moved or another one checked out, Enter and Esc both refuse
/// until it is back, so the dialog says which branch, on which commit — and
/// how to drop the operation instead.
#[test]
fn a_moved_branch_names_the_branch_and_commit_to_return_to() {
    let mut harness = TuiTestHarness::typical();
    let mut app = AppState::new();
    app.list.commits = vec![common::create_test_commit("abc123def456", "Add head line")];
    app.mode = AppMode::RebaseConflict(Box::new(ConflictState {
        operation_label: "Drop".to_string(),
        branch_refname: "refs/heads/feature".to_string(),
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
    let text = dialog_text(&buffer);

    assert!(!text.contains("on the commit it paused"), "{text}");
    assert!(
        text.contains(
            "Enter and Esc both refuse until feature is checked out on c5068579 again \
             — or quit and run gt --clean-journal to drop the operation."
        ),
        "{text}"
    );
}
