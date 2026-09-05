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

// TUI snapshot + behavioral tests for the move commit selection mode.

#[allow(dead_code)]
mod common;
use common::{TuiTestHarness, create_fragmap, simple_cluster};

use git_tailor::{
    CommitInfo, Oid, VirtualOid,
    app::{AppAction, AppMode, AppState, KeyCommand},
    fragmap::TouchKind,
    views,
};

fn make_app_in_move_select(source_index: usize, insert_before: usize) -> AppState {
    let mut app = common::app_state_from_commit_summaries(&[
        "Oldest commit on branch",
        "Middle commit",
        "Newest commit (HEAD)",
    ]);
    app.list.selection_index = source_index;
    app.mode = AppMode::MoveSelect {
        source_index,
        insert_before,
    };
    app
}

// ── Rendering tests ──────────────────────────────────────────────────

#[test]
fn test_move_select_footer_renders() {
    let mut harness = TuiTestHarness::typical();

    // Source is the newest commit (index 2), insertion cursor at 1
    let mut app = make_app_in_move_select(2, 1);

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
    }));
}

#[test]
fn test_move_select_separator_at_top() {
    let mut harness = TuiTestHarness::short();

    // Source is the newest commit (index 2), insertion at 0 (top)
    let mut app = make_app_in_move_select(2, 0);

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
    }));
}

#[test]
fn test_move_select_separator_at_middle() {
    let mut harness = TuiTestHarness::short();

    // Source is index 0 (oldest), insertion at 2 (first valid middle position)
    let mut app = make_app_in_move_select(0, 2);

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
    }));
}

#[test]
fn test_move_select_source_highlighted() {
    let mut harness = TuiTestHarness::short();

    // Source is middle commit (index 1), insertion at 0
    let mut app = make_app_in_move_select(1, 0);

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
    }));
}

#[test]
fn test_move_select_reversed() {
    let mut harness = TuiTestHarness::short();

    let mut app = make_app_in_move_select(2, 0);
    app.list.reverse = true;

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
    }));
}

// ── Key handling tests ───────────────────────────────────────────────

#[test]
fn test_move_navigation_skips_source() {
    // Source is index 1 (middle), start insertion at 0
    let mut app = make_app_in_move_select(1, 0);

    // MoveDown should skip both no-op positions (1 and 2) and land on 3
    views::move_select::handle_key(KeyCommand::MoveDown, &mut app);
    assert_eq!(
        app.mode,
        AppMode::MoveSelect {
            source_index: 1,
            insert_before: 3,
        }
    );

    // MoveUp should skip both no-op positions (2 and 1) and land on 0
    views::move_select::handle_key(KeyCommand::MoveUp, &mut app);
    assert_eq!(
        app.mode,
        AppMode::MoveSelect {
            source_index: 1,
            insert_before: 0,
        }
    );
}

#[test]
fn test_move_navigation_clamps_at_boundaries() {
    // Source is index 2 (last), start insertion at 0
    let mut app = make_app_in_move_select(2, 0);

    // MoveUp at 0 should stay at 0
    views::move_select::handle_key(KeyCommand::MoveUp, &mut app);
    assert_eq!(
        app.mode,
        AppMode::MoveSelect {
            source_index: 2,
            insert_before: 0,
        }
    );

    // MoveDown from 0 → 1
    views::move_select::handle_key(KeyCommand::MoveDown, &mut app);
    assert_eq!(
        app.mode,
        AppMode::MoveSelect {
            source_index: 2,
            insert_before: 1,
        }
    );
}

#[test]
fn test_move_navigation_reverse_inverts_direction() {
    let mut app = make_app_in_move_select(2, 0);
    app.list.reverse = true;

    // In reverse mode, MoveUp increases index
    views::move_select::handle_key(KeyCommand::MoveUp, &mut app);
    assert_eq!(
        app.mode,
        AppMode::MoveSelect {
            source_index: 2,
            insert_before: 1,
        }
    );

    // In reverse mode, MoveDown decreases index
    views::move_select::handle_key(KeyCommand::MoveDown, &mut app);
    assert_eq!(
        app.mode,
        AppMode::MoveSelect {
            source_index: 2,
            insert_before: 0,
        }
    );
}

#[test]
fn test_move_confirm_returns_execute_move() {
    let mut app = make_app_in_move_select(2, 0);
    app.reference_oid = Oid::from("ref000");

    let result = views::move_select::handle_key(KeyCommand::Confirm, &mut app);
    match result {
        AppAction::ExecuteMove {
            source_oid,
            insert_after_oid,
        } => {
            assert_eq!(source_oid, Oid::from("333333333333"));
            assert_eq!(insert_after_oid, Some(Oid::from("ref000")));
        }
        other => panic!("expected ExecuteMove, got {other:?}"),
    }
    assert_eq!(app.mode, AppMode::CommitList);
}

#[test]
fn test_move_confirm_at_same_position_shows_error() {
    // source_index == insert_before → no-op
    let mut app = make_app_in_move_select(1, 1);

    let result = views::move_select::handle_key(KeyCommand::Confirm, &mut app);
    assert!(matches!(result, AppAction::Handled));
    // Should remain in MoveSelect mode
    assert!(matches!(app.mode, AppMode::MoveSelect { .. }));
    assert!(app.status.is_error);
}

#[test]
fn test_move_confirm_at_source_plus_one_shows_error() {
    // insert_before == source_index + 1 → also a no-op (insert after self)
    let mut app = make_app_in_move_select(1, 2);

    let result = views::move_select::handle_key(KeyCommand::Confirm, &mut app);
    assert!(matches!(result, AppAction::Handled));
    assert!(matches!(app.mode, AppMode::MoveSelect { .. }));
    assert!(app.status.is_error);
}

#[test]
fn test_move_esc_cancels() {
    let mut app = make_app_in_move_select(2, 0);

    let result = views::move_select::handle_key(KeyCommand::Quit, &mut app);
    assert!(matches!(result, AppAction::Handled));
    assert_eq!(app.mode, AppMode::CommitList);
}

/// Build an app with three commits and a fragmap so the commit list render
/// exercises the fragmap-matrix indexing that crashed in the regression below.
fn app_with_fragmap_in_move_select(source_index: usize, insert_before: usize) -> AppState {
    let mut app = common::app_state_from_commit_summaries(&[
        "Oldest commit on branch",
        "Middle commit",
        "Newest commit (HEAD)",
    ]);
    app.fragmap = Some(create_fragmap(
        vec!["111111111111", "222222222222", "333333333333"],
        vec![simple_cluster(
            "file.rs",
            1,
            10,
            &["111111111111", "222222222222", "333333333333"],
        )],
        vec![
            vec![TouchKind::Added],
            vec![TouchKind::Modified],
            vec![TouchKind::Modified],
        ],
    ));
    app.list.selection_index = source_index;
    app.mode = AppMode::MoveSelect {
        source_index,
        insert_before,
    };
    app
}

// Regression: while navigating in MoveSelect, `viewport_selection_for_separator`
// writes a scroll anchor into `selection_index` that can land one or two slots
// past the last commit (intentional: the scroll math clamps it). That is only
// safe while the mode stays MoveSelect, where no consumer indexes the list with
// it. When the move is confirmed (then conflicts, falling back to a CommitList
// render behind the dialog) the stale anchor must not leak out — it used to crash
// the fragmap/footer render with an out-of-bounds index.
#[test]
fn test_move_down_to_end_then_confirm_keeps_selection_in_bounds() {
    let mut harness = TuiTestHarness::short();
    // Source is the oldest commit (index 0); start at the first valid slot.
    let mut app = app_with_fragmap_in_move_select(0, 2);

    // Move the insertion cursor down to the very end. The scroll anchor
    // (selection_index) now points past the last commit (len + 1).
    views::move_select::handle_key(KeyCommand::MoveDown, &mut app);

    // Confirm the move (returns ExecuteMove and switches back to CommitList).
    let action = views::move_select::handle_key(KeyCommand::Confirm, &mut app);
    assert!(matches!(action, AppAction::ExecuteMove { .. }));
    assert_eq!(app.mode, AppMode::CommitList);

    // The stale anchor must not leak out as the selection.
    assert!(
        app.list.selection_index < app.list.commits.len(),
        "selection_index {} out of bounds for {} commits",
        app.list.selection_index,
        app.list.commits.len()
    );

    // Rendering the commit list (as the conflict dialog does in the background
    // when the move conflicts) must not panic on the stale selection.
    let _ = harness.render(|frame| views::commit_list::render(&mut app, frame));
}

// Same stale-anchor regression as above, via the Esc-cancel exit path.
#[test]
fn test_move_down_then_cancel_keeps_selection_in_bounds() {
    let mut harness = TuiTestHarness::short();
    let mut app = app_with_fragmap_in_move_select(0, 2);

    views::move_select::handle_key(KeyCommand::MoveDown, &mut app);
    let result = views::move_select::handle_key(KeyCommand::Quit, &mut app);

    assert!(matches!(result, AppAction::Handled));
    assert_eq!(app.mode, AppMode::CommitList);
    assert!(
        app.list.selection_index < app.list.commits.len(),
        "selection_index {} out of bounds for {} commits",
        app.list.selection_index,
        app.list.commits.len()
    );

    let _ = harness.render(|frame| views::commit_list::render(&mut app, frame));
}

#[test]
fn test_enter_move_select_blocks_on_synthetic() {
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
    app.mode = AppMode::CommitList;

    app.enter_move_select();

    // Should still be in CommitList (blocked)
    assert_eq!(app.mode, AppMode::CommitList);
    assert!(app.status.is_error);
}

#[test]
fn test_enter_move_select_sets_correct_indices() {
    let mut app = common::app_state_from_commit_summaries(&["First", "Second", "Third"]);
    app.list.selection_index = 2;
    app.mode = AppMode::CommitList;

    app.enter_move_select();

    assert_eq!(
        app.mode,
        AppMode::MoveSelect {
            source_index: 2,
            insert_before: 1,
        }
    );
}

#[test]
fn test_enter_move_select_oldest_commit_starts_at_valid_position() {
    let mut app = common::app_state_from_commit_summaries(&["First", "Second", "Third"]);
    app.list.selection_index = 0;
    app.mode = AppMode::CommitList;

    app.enter_move_select();

    // source=0 → no-ops are 0 and 1, so first valid position is 2
    assert_eq!(
        app.mode,
        AppMode::MoveSelect {
            source_index: 0,
            insert_before: 2,
        }
    );
}

#[test]
fn test_enter_move_select_single_commit_blocked() {
    let mut app = AppState::new();
    app.list.commits = vec![common::create_test_commit("aaa111bbb222", "Only commit")];
    app.list.selection_index = 0;
    app.mode = AppMode::CommitList;

    app.enter_move_select();

    assert_eq!(app.mode, AppMode::CommitList);
    assert!(app.status.is_error);
}

#[test]
fn test_move_select_fragmap_highlight_tracks_separator() {
    // Regression for T217: the highlighted row in the hunk group matrix must be
    // the separator row (the "▶ move here" placeholder), not a commit row two
    // positions below it.
    //
    // After navigation, `viewport_selection_for_separator` sets
    // `selection_index = insert_before + 1` as a scroll anchor.  The fragmap
    // highlight must NOT blindly follow this scroll anchor — the separator row
    // is already visually distinct via COLOR_ACTION_INSERT_BG, and no commit
    // row should receive COLOR_SELECTED_FRAGMAP_BG.
    let mut harness = TuiTestHarness::short(); // 80×10

    let mut app = common::app_state_from_commit_summaries(&[
        "Oldest commit on branch",
        "Middle commit",
        "Newest commit (HEAD)",
    ]);

    // All three commits touch the same cluster so the Hunk groups column renders.
    app.fragmap = Some(create_fragmap(
        vec!["111111111111", "222222222222", "333333333333"],
        vec![simple_cluster(
            "file.rs",
            1,
            10,
            &["111111111111", "222222222222", "333333333333"],
        )],
        vec![
            vec![TouchKind::Added],
            vec![TouchKind::Modified],
            vec![TouchKind::Modified],
        ],
    ));

    // source=2 (newest), insert_before=0 (top-of-list insertion).
    // `viewport_selection_for_separator(0, reverse=false)` returns 0+1=1.
    // That value is the scroll anchor written to `selection_index` after each
    // navigation key; we set it here to reproduce the live state.
    app.list.selection_index = 1;
    app.mode = AppMode::MoveSelect {
        source_index: 2,
        insert_before: 0,
    };

    let buffer = harness.render(|frame| {
        views::commit_list::render(&mut app, frame);
    });

    // Expected visual layout (header at y=0, footer at y=9):
    //   y=1: ▶ move here  (separator, insert_before=0) ← this is the "selected" row
    //   y=2: commit 0     (Oldest commit on branch)
    //   y=3: commit 1     (Middle commit)
    //   y=4: commit 2     (source commit, teal bg)
    //
    // The separator row is the visual indicator for the insertion point; it
    // already carries COLOR_ACTION_INSERT_BG (Rgb(40,40,100)) across all
    // columns.  No commit row should receive COLOR_SELECTED_FRAGMAP_BG
    // (Rgb(60,60,80)) — that was the bug: the fragmap highlight was
    // incorrectly landing on y=3 (two rows below the separator) because
    // `is_selected` was derived from the scroll anchor rather than from the
    // insertion-point row.
    //
    // The fragmap column begins at x=72 (x=71 is the │ separator).
    let fragmap_x: u16 = 72;
    let insert_bg = ratatui::style::Color::Rgb(40, 40, 100);
    let selection_bg = ratatui::style::Color::Rgb(60, 60, 80);

    // The separator row's fragmap cell must carry the insert indicator color.
    assert_eq!(
        buffer.cell((fragmap_x, 1)).unwrap().bg,
        insert_bg,
        "fragmap cell on the separator row (y=1) should have the insert indicator color"
    );
    // No commit row should be highlighted with the selection color.
    assert_ne!(
        buffer.cell((fragmap_x, 2)).unwrap().bg,
        selection_bg,
        "fragmap row y=2 (commit 0) must not carry the selection highlight"
    );
    assert_ne!(
        buffer.cell((fragmap_x, 3)).unwrap().bg,
        selection_bg,
        "fragmap row y=3 (commit 1, previously wrong highlight) must not carry the selection highlight"
    );
}

/// Paging in both display orders. The existing reverse coverage is MoveUp /
/// MoveDown only, but paging is where the step size and the mirroring interact,
/// and `advance_insert` folds both into one `up ^ reverse`.
///
/// Source at index 2 keeps the no-op positions (2 and 3) clear of every landing
/// spot below, so the assertions are about paging rather than no-op skipping.
#[test]
fn test_move_navigation_pages_in_both_display_orders() {
    use git_tailor::app::KeyCommand;

    // (key, expected insert_before oldest-first, expected newest-first)
    let cases = [(KeyCommand::PageUp, 4, 10), (KeyCommand::PageDown, 10, 4)];

    for (key, forward, reversed) in cases {
        for (reverse, expected) in [(false, forward), (true, reversed)] {
            let summaries: Vec<String> = (0..10).map(|i| format!("commit {i}")).collect();
            let refs: Vec<&str> = summaries.iter().map(String::as_str).collect();
            let mut app = common::app_state_from_commit_summaries(&refs);
            app.list.selection_index = 2;
            app.list.visible_height = 5; // a page is 4 rows
            app.list.reverse = reverse;
            app.mode = AppMode::MoveSelect {
                source_index: 2,
                insert_before: 8,
            };

            views::move_select::handle_key(key, &mut app);

            match app.mode {
                AppMode::MoveSelect { insert_before, .. } => assert_eq!(
                    insert_before, expected,
                    "{key:?} with reverse={reverse} should land on {expected}"
                ),
                ref other => panic!("expected MoveSelect, got {other:?}"),
            }
        }
    }
}
