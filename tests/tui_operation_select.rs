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

// TUI snapshot tests for the operation picker dialog. The menu is filtered by
// the kind of the selected row (real commit / staged / unstaged).

#[allow(dead_code)]
mod common;
use common::TuiTestHarness;

use git_tailor::{CommitInfo, VirtualOid, app::AppState, views};
use ratatui::buffer::Buffer;

/// A synthetic working-tree row (Staged / Unstaged) as the loader produces.
fn synthetic_row(oid: VirtualOid, summary: &str) -> CommitInfo {
    CommitInfo {
        oid,
        summary: summary.to_string(),
        author: None,
        date: None,
        parent_oids: vec![],
        message: summary.to_string(),
        author_email: None,
        author_date: None,
        committer: None,
        committer_email: None,
        commit_date: None,
    }
}

fn render_picker(app: &mut AppState) -> Buffer {
    let mut harness = TuiTestHarness::typical();
    harness.render(|frame| {
        views::commit_list::render(app, frame);
        views::operation_select::render(app, frame);
    })
}

#[test]
fn test_operation_picker_real_commit() {
    let mut app = common::app_state_from_commit_summaries(&["Add feature X", "Refactor parser"]);
    app.selection_index = 1; // a non-oldest real commit (offers the full menu)
    app.enter_operation_select();
    insta::assert_debug_snapshot!(render_picker(&mut app));
}

#[test]
fn test_operation_picker_oldest_commit_omits_move() {
    let mut app = common::app_state_from_commit_summaries(&["Add feature X", "Refactor parser"]);
    app.selection_index = 0; // the oldest commit — Move is not offered
    app.enter_operation_select();
    insta::assert_debug_snapshot!(render_picker(&mut app));
}

#[test]
fn test_operation_picker_unstaged_row() {
    let mut app = common::app_state_from_commit_summaries(&["Add feature X"]);
    app.commits
        .push(synthetic_row(VirtualOid::Staged, "staged"));
    app.commits
        .push(synthetic_row(VirtualOid::Unstaged, "unstaged"));
    app.selection_index = app.commits.len() - 1; // the Unstaged row
    app.enter_operation_select();
    insta::assert_debug_snapshot!(render_picker(&mut app));
}

#[test]
fn test_operation_picker_staged_row() {
    let mut app = common::app_state_from_commit_summaries(&["Add feature X"]);
    app.commits
        .push(synthetic_row(VirtualOid::Staged, "staged"));
    app.commits
        .push(synthetic_row(VirtualOid::Unstaged, "unstaged"));
    app.selection_index = app.commits.len() - 2; // the Staged row
    app.enter_operation_select();
    insta::assert_debug_snapshot!(render_picker(&mut app));
}

/// A long non-ASCII header must truncate on a character boundary, not a byte
/// offset (which would panic mid-codepoint).
#[test]
fn test_operation_picker_long_non_ascii_summary_does_not_panic() {
    let mut app = common::app_state_from_commit_summaries(&[
        "Réfactor le café très naïve — обновление — 日本語のまとめ for the parser",
    ]);
    app.selection_index = 0;
    app.enter_operation_select();
    let mut harness = TuiTestHarness::typical();
    let _ = harness.render(|frame| views::operation_select::render(&mut app, frame));
}
