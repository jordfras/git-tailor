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

// TUI snapshot tests for the bulk autofixup confirmation dialog.

#[allow(dead_code)]
mod common;
use common::TuiTestHarness;

use git_tailor::{
    Oid,
    app::{AppAction, AppMode, AppState, KeyCommand, PendingAutofixup, SquashMode},
    autofixup::AutofixupPair,
    views,
};

fn pair(
    source_oid: &str,
    source_summary: &str,
    target_oid: &str,
    target_summary: &str,
    mode: SquashMode,
) -> AutofixupPair {
    AutofixupPair {
        source_oid: Oid::from(source_oid),
        target_oid: Oid::from(target_oid),
        source_summary: source_summary.to_string(),
        target_summary: target_summary.to_string(),
        source_message: source_summary.to_string(),
        target_message: target_summary.to_string(),
        mode,
    }
}

fn make_app_in_autofixup_confirm(
    pairs: Vec<AutofixupPair>,
    selected_group: usize,
    message_overrides: std::collections::HashMap<String, String>,
) -> AppState {
    let mut app = AppState::new();
    app.list.commits = vec![
        common::create_test_commit("abc123def456", "Add parser"),
        common::create_test_commit("def456ghi789", "fixup! Add parser"),
    ];
    app.list.selection_index = 0;
    app.mode = AppMode::AutofixupConfirm(PendingAutofixup {
        pairs,
        head_oid: Oid::from("def456ghi789abcdef012"),
        reference_oid: Oid::from("000000000000abcdef012"),
        selected_group,
        message_overrides,
    });
    app
}

#[test]
fn test_autofixup_confirm_dialog_single_pair() {
    let mut harness = TuiTestHarness::typical();

    let mut app = make_app_in_autofixup_confirm(
        vec![pair(
            "abc123def456",
            "fixup! Add parser",
            "def456ghi789",
            "Add parser",
            SquashMode::Fixup,
        )],
        0,
        Default::default(),
    );

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::autofixup::render_autofixup_confirm(&mut app, frame);
    }));
}

#[test]
fn test_autofixup_confirm_dialog_groups_stacked_fixups_under_one_target() {
    let mut harness = TuiTestHarness::typical();

    let mut app = make_app_in_autofixup_confirm(
        vec![
            pair(
                "111111111111",
                "fixup! Add parser",
                "abc123def456",
                "Add parser",
                SquashMode::Fixup,
            ),
            pair(
                "222222222222",
                "fixup! Add parser",
                "abc123def456",
                "Add parser",
                SquashMode::Fixup,
            ),
            pair(
                "333333333333",
                "squash! Add lexer",
                "444444444444",
                "Add lexer",
                SquashMode::Squash,
            ),
        ],
        1,
        Default::default(),
    );

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::autofixup::render_autofixup_confirm(&mut app, frame);
    }));
}

#[test]
fn test_autofixup_confirm_dialog_shows_edited_indicator() {
    let mut harness = TuiTestHarness::typical();

    let mut app = make_app_in_autofixup_confirm(
        vec![pair(
            "abc123def456",
            "fixup! Add parser",
            "def456ghi789",
            "Add parser",
            SquashMode::Fixup,
        )],
        0,
        std::collections::HashMap::from([(
            "Add parser".to_string(),
            "Add parser (with a fix)\n".to_string(),
        )]),
    );

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::autofixup::render_autofixup_confirm(&mut app, frame);
    }));
}

#[test]
fn test_autofixup_confirm_dialog_long_summary() {
    let mut harness = TuiTestHarness::typical();

    let mut app = make_app_in_autofixup_confirm(
        vec![pair(
            "abc123def456",
            "fixup! Refactor the entire parser module to use trait-based dispatching",
            "def456ghi789",
            "Add parser",
            SquashMode::Fixup,
        )],
        0,
        Default::default(),
    );

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::autofixup::render_autofixup_confirm(&mut app, frame);
    }));
}

/// A long *target* summary is truncated too, and still leaves room for the
/// "(edited)" marker rather than pushing it onto a wrapped line.
#[test]
fn test_autofixup_confirm_dialog_long_target_summary() {
    let mut harness = TuiTestHarness::typical();

    let mut overrides = std::collections::HashMap::new();
    overrides.insert(
        "Refactor the entire parser module to use trait-based dispatching".to_string(),
        "edited\n".to_string(),
    );

    let mut app = make_app_in_autofixup_confirm(
        vec![pair(
            "abc123def456",
            "fixup! Add parser",
            "def456ghi789",
            "Refactor the entire parser module to use trait-based dispatching",
            SquashMode::Fixup,
        )],
        0,
        overrides,
    );

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::autofixup::render_autofixup_confirm(&mut app, frame);
    }));
}

// ---------------------------------------------------------------------------
// Key handling (handle_confirm_key)
// ---------------------------------------------------------------------------

fn two_target_groups() -> Vec<AutofixupPair> {
    vec![
        pair(
            "111111111111",
            "fixup! Add parser",
            "abc123def456",
            "Add parser",
            SquashMode::Fixup,
        ),
        pair(
            "222222222222",
            "squash! Add lexer",
            "333333333333",
            "Add lexer",
            SquashMode::Squash,
        ),
    ]
}

fn selected_group_index(app: &AppState) -> usize {
    match &app.mode {
        AppMode::AutofixupConfirm(pending) => pending.selected_group,
        other => panic!("expected AutofixupConfirm, got {other:?}"),
    }
}

#[test]
fn test_autofixup_confirm_move_down_selects_next_group() {
    let mut app = make_app_in_autofixup_confirm(two_target_groups(), 0, Default::default());

    let result = views::autofixup::handle_confirm_key(KeyCommand::MoveDown, &mut app);
    assert!(matches!(result, AppAction::Handled));
    assert_eq!(selected_group_index(&app), 1);
}

#[test]
fn test_autofixup_confirm_move_up_clamps_at_the_first_group() {
    let mut app = make_app_in_autofixup_confirm(two_target_groups(), 0, Default::default());

    let result = views::autofixup::handle_confirm_key(KeyCommand::MoveUp, &mut app);
    assert!(matches!(result, AppAction::Handled));
    assert_eq!(selected_group_index(&app), 0);
}

#[test]
fn test_autofixup_confirm_move_down_clamps_at_the_last_group() {
    let mut app = make_app_in_autofixup_confirm(two_target_groups(), 1, Default::default());

    let result = views::autofixup::handle_confirm_key(KeyCommand::MoveDown, &mut app);
    assert!(matches!(result, AppAction::Handled));
    assert_eq!(selected_group_index(&app), 1);
}

#[test]
fn test_autofixup_confirm_reword_targets_the_selected_group() {
    let mut app = make_app_in_autofixup_confirm(two_target_groups(), 1, Default::default());

    let result = views::autofixup::handle_confirm_key(KeyCommand::Reword, &mut app);
    match result {
        AppAction::PrepareAutofixupEditMessage {
            target_summary,
            template,
        } => {
            assert_eq!(target_summary, "Add lexer");
            // Target's own message is live/editable; the source folding into
            // it is commented out (git-autosquash style).
            assert!(template.starts_with("Add lexer\n"));
            assert!(template.contains("# squash! Add lexer"));
        }
        other => panic!("expected PrepareAutofixupEditMessage, got {other:?}"),
    }
    // Editing doesn't leave the confirmation dialog.
    assert!(matches!(app.mode, AppMode::AutofixupConfirm(_)));
}

#[test]
fn test_autofixup_confirm_enter_executes_the_whole_batch_with_overrides() {
    let overrides = std::collections::HashMap::from([(
        "Add parser".to_string(),
        "Custom message\n".to_string(),
    )]);
    let mut app = make_app_in_autofixup_confirm(two_target_groups(), 0, overrides.clone());

    let result = views::autofixup::handle_confirm_key(KeyCommand::Confirm, &mut app);
    match result {
        AppAction::ExecuteAutofixup {
            head_oid,
            reference_oid,
            pairs,
            message_overrides,
        } => {
            assert_eq!(head_oid, Oid::from("def456ghi789abcdef012"));
            assert_eq!(reference_oid, Oid::from("000000000000abcdef012"));
            assert_eq!(pairs.len(), 2);
            assert_eq!(message_overrides, overrides);
        }
        other => panic!("expected ExecuteAutofixup, got {other:?}"),
    }
    assert_eq!(app.mode, AppMode::CommitList);
}

#[test]
fn test_autofixup_confirm_esc_cancels() {
    let mut app = make_app_in_autofixup_confirm(two_target_groups(), 0, Default::default());

    let result = views::autofixup::handle_confirm_key(KeyCommand::Quit, &mut app);
    assert!(matches!(result, AppAction::Handled));
    assert_eq!(app.mode, AppMode::CommitList);
}

#[test]
fn test_autofixup_confirm_dialog_narrow_terminal() {
    let mut harness = TuiTestHarness::very_narrow();

    let mut app = make_app_in_autofixup_confirm(
        vec![pair(
            "abc123def456",
            "fixup! Add parser",
            "def456ghi789",
            "Add parser",
            SquashMode::Fixup,
        )],
        0,
        Default::default(),
    );

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
        views::autofixup::render_autofixup_confirm(&mut app, frame);
    }));
}
