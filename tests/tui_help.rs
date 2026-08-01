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

// TUI snapshot tests for the help dialog.

#[allow(dead_code)]
mod common;
use common::TuiTestHarness;

use git_tailor::{
    app::{AppMode, AppState},
    views,
};

fn make_app_in_help() -> AppState {
    let mut app =
        common::app_state_from_commit_summaries(&["Refactor parser module", "Add feature X"]);
    app.selection_index = 0;
    app.mode = AppMode::Help(Box::new(AppMode::CommitList));
    app
}

fn make_app_in_detail_help() -> AppState {
    let mut app =
        common::app_state_from_commit_summaries(&["Refactor parser module", "Add feature X"]);
    app.selection_index = 0;
    app.mode = AppMode::Help(Box::new(AppMode::CommitDetail));
    app
}

/// Full-size terminal — all keybindings visible without scrolling.
#[test]
fn test_help_dialog_full_size() {
    let mut harness = TuiTestHarness::typical();
    let mut app = make_app_in_help();

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::help::render(&AppMode::CommitList, &mut app, frame);
    }));
}

/// Short terminal — content taller than the dialog area, scrollbar visible.
#[test]
fn test_help_dialog_short_terminal_scrollbar() {
    let mut harness = TuiTestHarness::short();
    let mut app = make_app_in_help();

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::help::render(&AppMode::CommitList, &mut app, frame);
    }));
}

/// Short terminal, scrolled to middle — scrollbar thumb moves down.
#[test]
fn test_help_dialog_scrolled() {
    let mut harness = TuiTestHarness::short();
    let mut app = make_app_in_help();
    app.dialog.offset = 5;

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::help::render(&AppMode::CommitList, &mut app, frame);
    }));
}

/// A dialog that grows (terminal resized taller, or content shrunk) must not
/// leave the stored scroll offset past the new maximum.
///
/// `Dialog::render` clamps for *display*, so the frame itself looks right — but
/// a stale stored offset makes `MoveUp` appear frozen, because every press
/// decrements from the stale value and the display stays pinned at the real
/// maximum until it catches up.
#[test]
fn test_dialog_scroll_offset_reclamped_when_the_dialog_grows() {
    let mut app = make_app_in_help();

    // Short terminal: scroll the help dialog all the way to the bottom.
    let mut short = TuiTestHarness::short();
    short.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::help::render(&AppMode::CommitList, &mut app, frame);
    });
    app.dialog.to_end();
    let scrolled = app.dialog.offset;
    assert!(
        scrolled > 0,
        "help content must overflow the short terminal"
    );

    // Now render the same dialog in a taller terminal: more fits, so the
    // maximum drops below the offset we scrolled to.
    let mut tall = TuiTestHarness::typical();
    tall.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::help::render(&AppMode::CommitList, &mut app, frame);
    });

    assert!(
        app.dialog.max < scrolled,
        "test is not exercising the shrink: max {} did not drop below {scrolled}",
        app.dialog.max
    );
    assert_eq!(
        app.dialog.offset, app.dialog.max,
        "stored offset must be re-clamped to the new maximum, otherwise \
         scrolling up is unresponsive until it falls back below it"
    );
}

/// Commit detail context — shows only detail-view keybindings.
#[test]
fn test_help_dialog_commit_detail_context() {
    let mut harness = TuiTestHarness::typical();
    let mut app = make_app_in_detail_help();

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::help::render(&AppMode::CommitDetail, &mut app, frame);
    }));
}
