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

// TUI snapshot tests for the squash target selection dialog.

#[allow(dead_code)]
mod common;
use common::{TuiTestHarness, create_fragmap, simple_cluster};

use git_tailor::{
    CommitInfo, Oid, VirtualOid,
    app::{AppAction, AppMode, AppState, KeyCommand, SquashMode, SquashSource},
    fragmap::TouchKind,
    views,
    views::theme::Theme,
};

fn make_app_in_squash_select(source_index: usize, selection_index: usize) -> AppState {
    let mut app = common::app_state_from_commit_summaries(&[
        "Oldest commit on branch",
        "Middle commit",
        "Newest commit (HEAD)",
    ]);
    app.list.selection_index = selection_index;
    app.mode = AppMode::SquashSelect {
        source_index,
        squash_mode: SquashMode::Squash,
    };
    app
}

#[test]
fn test_squash_footer_renders() {
    let mut harness = TuiTestHarness::typical();

    let mut app = make_app_in_squash_select(2, 2);

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
    }));
}

#[test]
fn test_squash_footer_source_different_from_selection() {
    let mut harness = TuiTestHarness::typical();

    // Source is index 2, but user has navigated selection to index 0
    let mut app = make_app_in_squash_select(2, 0);

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
    }));
}

#[test]
fn test_squash_confirm_returns_prepare_squash() {
    let mut app = make_app_in_squash_select(2, 0);

    let result = views::squash_select::handle_key(KeyCommand::Confirm, &mut app);
    match result {
        AppAction::PrepareSquash {
            source, target_oid, ..
        } => {
            assert_eq!(
                source,
                SquashSource::Commit {
                    oid: Oid::from("333333333333"),
                    message: "Newest commit (HEAD)".to_string(),
                }
            );
            assert_eq!(target_oid, Oid::from("111111111111"));
        }
        other => panic!("Expected PrepareSquash, got {:?}", other),
    }
    assert_eq!(app.mode, AppMode::CommitList);
}

#[test]
fn test_squash_into_self_blocked() {
    let mut app = make_app_in_squash_select(1, 1);

    let result = views::squash_select::handle_key(KeyCommand::Confirm, &mut app);
    assert!(matches!(result, AppAction::Handled));
    assert!(app.status.message.is_some());
    assert!(app.status.is_error);
    // Mode stays in SquashSelect
    assert!(matches!(app.mode, AppMode::SquashSelect { .. }));
}

#[test]
fn test_squash_into_staged_blocked() {
    let mut app = AppState::new();
    app.list.commits = vec![
        common::create_test_commit("aaa111bbb222", "Real commit"),
        CommitInfo {
            oid: VirtualOid::Staged,
            summary: "staged".to_string(),
            ..common::create_test_commit("staged", "staged")
        },
    ];
    app.list.selection_index = 1;
    app.mode = AppMode::SquashSelect {
        source_index: 0,
        squash_mode: SquashMode::Squash,
    };

    let result = views::squash_select::handle_key(KeyCommand::Confirm, &mut app);
    assert!(matches!(result, AppAction::Handled));
    assert!(app.status.is_error);
}

#[test]
fn test_squash_esc_cancels() {
    let mut app = make_app_in_squash_select(2, 0);

    let result = views::squash_select::handle_key(KeyCommand::Quit, &mut app);
    assert!(matches!(result, AppAction::Handled));
    assert_eq!(app.mode, AppMode::CommitList);
}

/// A commit list with one real commit followed by the two synthetic
/// working-tree rows, as the loader builds it.
fn app_with_worktree_rows() -> AppState {
    let mut app = AppState::new();
    app.list.commits = vec![
        common::create_test_commit("aaa111bbb222", "Real commit"),
        CommitInfo {
            oid: VirtualOid::Staged,
            summary: "staged".to_string(),
            ..common::create_test_commit("staged", "staged")
        },
        CommitInfo {
            oid: VirtualOid::Unstaged,
            summary: "unstaged".to_string(),
            ..common::create_test_commit("unstaged", "unstaged")
        },
    ];
    app.mode = AppMode::CommitList;
    app
}

/// Working-tree changes can be folded straight into a commit, so the rows are
/// valid squash sources.
#[test]
fn test_squash_starts_from_a_worktree_row() {
    for (index, row) in [(1, "staged"), (2, "unstaged")] {
        let mut app = app_with_worktree_rows();
        app.list.selection_index = index;

        app.enter_squash_select();

        assert_eq!(
            app.mode,
            AppMode::SquashSelect {
                source_index: index,
                squash_mode: SquashMode::Squash,
            },
            "{row} row should be a valid squash source"
        );
        assert!(!app.status.is_error, "{row}");
    }
}

/// Folding a row in needs only one commit to fold into, unlike a commit source
/// which needs another besides itself.
#[test]
fn test_worktree_row_folds_into_a_lone_commit() {
    let mut app = app_with_worktree_rows();
    app.list.selection_index = 2;

    app.enter_fixup_select();

    assert!(matches!(app.mode, AppMode::SquashSelect { .. }));
    assert!(!app.status.is_error);
}

/// Confirming a row source names the row, not a commit OID.
#[test]
fn test_worktree_row_confirm_returns_a_worktree_source() {
    let mut app = app_with_worktree_rows();
    app.list.selection_index = 0;
    app.mode = AppMode::SquashSelect {
        source_index: 2,
        squash_mode: SquashMode::Fixup,
    };

    match views::squash_select::handle_key(KeyCommand::Confirm, &mut app) {
        AppAction::PrepareSquash {
            source, target_oid, ..
        } => {
            assert_eq!(
                source,
                SquashSource::Worktree(git_tailor::repo::WorktreeSource::Unstaged)
            );
            assert_eq!(target_oid, Oid::from("aaa111bbb222"));
        }
        other => panic!("Expected PrepareSquash, got {other:?}"),
    }
}

/// The other working-tree row is not something changes can be folded into, and
/// it sits below the source so the cursor can reach it.
#[test]
fn test_worktree_row_cannot_target_the_other_row() {
    let mut app = app_with_worktree_rows();
    app.list.selection_index = 1;
    app.mode = AppMode::SquashSelect {
        source_index: 2,
        squash_mode: SquashMode::Fixup,
    };

    let result = views::squash_select::handle_key(KeyCommand::Confirm, &mut app);
    assert!(matches!(result, AppAction::Handled));
    assert!(app.status.is_error);
    assert!(matches!(app.mode, AppMode::SquashSelect { .. }));
}

/// With no commits on the branch at all there is nothing to fold a row into.
#[test]
fn test_worktree_row_blocked_without_any_commit() {
    let mut app = AppState::new();
    app.list.commits = vec![CommitInfo {
        oid: VirtualOid::Unstaged,
        summary: "unstaged".to_string(),
        ..common::create_test_commit("unstaged", "unstaged")
    }];
    app.list.selection_index = 0;
    app.mode = AppMode::CommitList;

    app.enter_fixup_select();

    assert_eq!(app.mode, AppMode::CommitList);
    assert!(app.status.is_error);
}

/// A lone commit has nothing earlier to fold into.
#[test]
fn test_squash_blocked_on_single_commit() {
    let mut app = AppState::new();
    app.list.commits = vec![common::create_test_commit("aaa111bbb222", "Only commit")];
    app.list.selection_index = 0;
    app.mode = AppMode::CommitList;

    app.enter_squash_select();

    assert_eq!(app.mode, AppMode::CommitList);
    assert!(app.status.is_error);
}

#[test]
fn test_fixup_blocked_on_single_commit() {
    let mut app = AppState::new();
    app.list.commits = vec![common::create_test_commit("aaa111bbb222", "Only commit")];
    app.list.selection_index = 0;
    app.mode = AppMode::CommitList;

    app.enter_fixup_select();

    assert_eq!(app.mode, AppMode::CommitList);
    assert!(app.status.is_error);
}

#[test]
fn test_squash_navigation_moves_selection() {
    let mut app = make_app_in_squash_select(2, 2);

    views::squash_select::handle_key(KeyCommand::MoveUp, &mut app);
    assert_eq!(app.list.selection_index, 1);

    views::squash_select::handle_key(KeyCommand::MoveUp, &mut app);
    assert_eq!(app.list.selection_index, 0);

    views::squash_select::handle_key(KeyCommand::MoveDown, &mut app);
    assert_eq!(app.list.selection_index, 1);
}

#[test]
fn test_squash_clamps_to_source_index() {
    // source_index=1 (middle), start at 1 — cannot move to index 2 (later)
    let mut app = make_app_in_squash_select(1, 1);

    views::squash_select::handle_key(KeyCommand::MoveDown, &mut app);
    assert_eq!(app.list.selection_index, 1);

    views::squash_select::handle_key(KeyCommand::MoveUp, &mut app);
    assert_eq!(app.list.selection_index, 0);

    views::squash_select::handle_key(KeyCommand::MoveDown, &mut app);
    assert_eq!(app.list.selection_index, 1);

    views::squash_select::handle_key(KeyCommand::MoveDown, &mut app);
    assert_eq!(app.list.selection_index, 1);
}

#[test]
fn test_squash_clamps_in_reverse_mode() {
    let mut app = make_app_in_squash_select(1, 1);
    app.list.reverse = true;

    // In reverse, MoveUp maps to move_down (increase index) — clamped at source
    views::squash_select::handle_key(KeyCommand::MoveUp, &mut app);
    assert_eq!(app.list.selection_index, 1);

    // In reverse, MoveDown maps to move_up (decrease index) — allowed
    views::squash_select::handle_key(KeyCommand::MoveDown, &mut app);
    assert_eq!(app.list.selection_index, 0);
}

#[test]
fn test_squash_page_down_clamped() {
    let mut app = make_app_in_squash_select(1, 0);
    app.list.visible_height = 10;

    // PageDown would jump past source_index — clamped to 1
    views::squash_select::handle_key(KeyCommand::PageDown, &mut app);
    assert_eq!(app.list.selection_index, 1);
}

/// Squash mode with fragmap: source is commit 2 (selected), commit 0 is
/// squashable (shares cluster cleanly), commit 1 is unrelated.
/// Source should have magenta bg, squashable candidate should be yellow,
/// unrelated should be dim.
#[test]
fn test_squash_candidate_coloring_with_fragmap() {
    let mut harness = TuiTestHarness::short();

    let mut app = AppState::new();
    app.list.commits = vec![
        common::create_test_commit("aaaa11112222", "Add config file"),
        common::create_test_commit("bbbb33334444", "Unrelated change"),
        common::create_test_commit("cccc55556666", "Fix config typo"),
    ];
    app.list.selection_index = 0; // navigated to commit 0 as target
    app.mode = AppMode::SquashSelect {
        source_index: 2,
        squash_mode: SquashMode::Squash,
    };
    // Documented colours below are the Plain theme's; pin it for stability.
    app.theme = Theme::Plain;

    // Cluster 0: commits 0 and 2 both touch it, commit 1 does not → squashable
    app.fragmap = Some(create_fragmap(
        vec!["aaaa11112222", "bbbb33334444", "cccc55556666"],
        vec![
            simple_cluster("config.rs", 10, 20, &["aaaa11112222", "cccc55556666"]),
            simple_cluster("other.rs", 1, 5, &["bbbb33334444"]),
        ],
        vec![
            vec![TouchKind::Added, TouchKind::None],
            vec![TouchKind::None, TouchKind::Modified],
            vec![TouchKind::Modified, TouchKind::None],
        ],
    ));

    insta::assert_debug_snapshot!(
        harness.render(|frame| views::commit_list::render(&mut app, frame))
    );
}

/// Squash mode with fragmap: source is commit 2, commit 0 would conflict
/// (commit 1 also touches the same cluster, creating a conflict).
#[test]
fn test_squash_candidate_coloring_conflicting() {
    let mut harness = TuiTestHarness::short();

    let mut app = AppState::new();
    app.list.commits = vec![
        common::create_test_commit("aaaa11112222", "Add parser"),
        common::create_test_commit("bbbb33334444", "Refactor parser"),
        common::create_test_commit("cccc55556666", "Fix parser bug"),
    ];
    app.list.selection_index = 0; // navigated to commit 0 as target
    app.mode = AppMode::SquashSelect {
        source_index: 2,
        squash_mode: SquashMode::Squash,
    };
    // Documented colours below are the Plain theme's; pin it for stability.
    app.theme = Theme::Plain;

    // All three commits touch cluster 0 → conflicting between 0 and 2
    app.fragmap = Some(create_fragmap(
        vec!["aaaa11112222", "bbbb33334444", "cccc55556666"],
        vec![simple_cluster(
            "parser.rs",
            10,
            30,
            &["aaaa11112222", "bbbb33334444", "cccc55556666"],
        )],
        vec![
            vec![TouchKind::Added],
            vec![TouchKind::Modified],
            vec![TouchKind::Modified],
        ],
    ));

    insta::assert_debug_snapshot!(
        harness.render(|frame| views::commit_list::render(&mut app, frame))
    );
}

/// Squash mode with source_index=1 (middle commit): commit at index 2 (newest)
/// should be dimmed with DarkGray to indicate it is an unreachable target.
#[test]
fn test_squash_dims_later_commits() {
    let mut harness = TuiTestHarness::short();

    let mut app = make_app_in_squash_select(1, 0);

    insta::assert_debug_snapshot!(
        harness.render(|frame| views::commit_list::render(&mut app, frame))
    );
}

#[test]
fn test_fixup_confirm_returns_prepare_fixup() {
    let mut app = common::app_state_from_commit_summaries(&[
        "Oldest commit on branch",
        "Middle commit",
        "Newest commit (HEAD)",
    ]);
    app.list.selection_index = 0;
    app.mode = AppMode::SquashSelect {
        source_index: 2,
        squash_mode: SquashMode::Fixup,
    };

    let result = views::squash_select::handle_key(KeyCommand::Confirm, &mut app);
    match result {
        AppAction::PrepareSquash {
            source,
            target_oid,
            squash_mode,
            ..
        } => {
            assert_eq!(
                source,
                SquashSource::Commit {
                    oid: Oid::from("333333333333"),
                    message: "Newest commit (HEAD)".to_string(),
                }
            );
            assert_eq!(target_oid, Oid::from("111111111111"));
            assert_eq!(squash_mode, SquashMode::Fixup);
        }
        other => panic!("Expected PrepareSquash with Fixup, got {:?}", other),
    }
    assert_eq!(app.mode, AppMode::CommitList);
}

#[test]
fn test_fixup_footer_renders() {
    let mut harness = TuiTestHarness::typical();

    let mut app = common::app_state_from_commit_summaries(&[
        "Oldest commit on branch",
        "Middle commit",
        "Newest commit (HEAD)",
    ]);
    app.list.selection_index = 0;
    app.mode = AppMode::SquashSelect {
        source_index: 2,
        squash_mode: SquashMode::Fixup,
    };

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
    }));
}

/// `squash_select` is the only `handle_list_navigation` caller that passes both a
/// real page size and `reverse`, so it is the only test that would notice the
/// paging arithmetic changing.
#[test]
fn test_squash_select_pages_in_both_display_orders() {
    use git_tailor::app::KeyCommand;

    // (key, expected selection oldest-first, expected newest-first)
    // Paging down lands on 9, not further: squash_select clamps the cursor to
    // the source index, and the source is the last commit here.
    let cases = [(KeyCommand::PageUp, 1, 9), (KeyCommand::PageDown, 9, 1)];

    for (key, forward, reversed) in cases {
        for (reverse, expected) in [(false, forward), (true, reversed)] {
            let summaries: Vec<String> = (0..10).map(|i| format!("commit {i}")).collect();
            let refs: Vec<&str> = summaries.iter().map(String::as_str).collect();
            let mut app = common::app_state_from_commit_summaries(&refs);
            app.list.selection_index = 5;
            app.list.visible_height = 5; // a page is 4 rows
            app.list.reverse = reverse;
            app.mode = AppMode::SquashSelect {
                source_index: 9,
                squash_mode: SquashMode::Squash,
            };

            views::squash_select::handle_key(key, &mut app);

            assert_eq!(
                app.list.selection_index, expected,
                "{key:?} with reverse={reverse} should select {expected}"
            );
        }
    }
}
