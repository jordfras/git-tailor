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

// Tests for the "split out hunk(s)" two-pane hunk picker dialog.

#[allow(dead_code)]
mod common;
use common::TuiTestHarness;

use git_tailor::{
    DiffLine, DiffLineKind, Hunk, Oid,
    app::{AppAction, AppMode, AppState, HunkPickerEntry, KeyCommand},
    views,
};

fn hunk(old_start: u32, old_text: &str, new_text: &str) -> Hunk {
    Hunk {
        old_start,
        old_lines: 1,
        new_start: old_start,
        new_lines: 1,
        lines: vec![
            DiffLine {
                kind: DiffLineKind::Deletion,
                content: format!("{old_text}\n"),
            },
            DiffLine {
                kind: DiffLineKind::Addition,
                content: format!("{new_text}\n"),
            },
        ],
    }
}

/// Three hunks: a.txt has two, b.txt has one.
fn make_entries() -> Vec<HunkPickerEntry> {
    vec![
        HunkPickerEntry {
            delta_idx: 0,
            hunk_idx: 0,
            file_path: "a.txt".to_string(),
            hunk: hunk(1, "a-old-1", "a-new-1"),
        },
        HunkPickerEntry {
            delta_idx: 0,
            hunk_idx: 1,
            file_path: "a.txt".to_string(),
            hunk: hunk(10, "a-old-2", "a-new-2"),
        },
        HunkPickerEntry {
            delta_idx: 1,
            hunk_idx: 0,
            file_path: "b.txt".to_string(),
            hunk: hunk(1, "b-old-1", "b-new-1"),
        },
    ]
}

fn make_app_in_hunks_select(hunk_index: usize, selected: &[usize]) -> AppState {
    make_app_in_hunks_select_with_context(hunk_index, selected, 3)
}

fn make_app_in_hunks_select_with_context(
    hunk_index: usize,
    selected: &[usize],
    context_lines: u32,
) -> AppState {
    let mut app = common::app_state_from_commit_summaries(&["Change a and b", "Add feature X"]);
    app.list.selection_index = 0;
    app.mode = AppMode::SplitHunksSelect {
        commit_oid: Oid::from("111111111111"),
        hunks: make_entries(),
        hunk_index,
        selected: selected.iter().copied().collect(),
        context_lines,
        preview_h_scroll: 0,
        preview_v_scroll: 0,
    };
    app
}

#[test]
fn test_split_hunks_select_first_highlighted_none_selected() {
    let mut harness = TuiTestHarness::typical();
    let mut app = make_app_in_hunks_select(0, &[]);

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::split_hunks_select::render(&mut app, frame);
    }));
}

#[test]
fn test_split_hunks_select_second_highlighted_first_selected() {
    let mut harness = TuiTestHarness::typical();
    let mut app = make_app_in_hunks_select(1, &[0]);

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::split_hunks_select::render(&mut app, frame);
    }));
}

/// MoveDown steps the cursor to the next hunk.
#[test]
fn test_move_down_steps_cursor() {
    let mut app = make_app_in_hunks_select(0, &[]);

    views::split_hunks_select::handle_key(KeyCommand::MoveDown, &mut app);

    match &app.mode {
        AppMode::SplitHunksSelect { hunk_index, .. } => assert_eq!(*hunk_index, 1),
        other => panic!("Expected SplitHunksSelect, got {:?}", other),
    }
}

/// `Space` toggles the hunk under the cursor in and out of the selection.
#[test]
fn test_toggle_hunk_select_adds_then_removes() {
    let mut app = make_app_in_hunks_select(1, &[]);

    views::split_hunks_select::handle_key(KeyCommand::TogglePickerItem, &mut app);
    match &app.mode {
        AppMode::SplitHunksSelect { selected, .. } => assert!(selected.contains(&1)),
        other => panic!("Expected SplitHunksSelect, got {:?}", other),
    }

    views::split_hunks_select::handle_key(KeyCommand::TogglePickerItem, &mut app);
    match &app.mode {
        AppMode::SplitHunksSelect { selected, .. } => assert!(!selected.contains(&1)),
        other => panic!("Expected SplitHunksSelect, got {:?}", other),
    }
}

/// Confirming with hunks explicitly marked splits out exactly those hunks'
/// (delta_idx, hunk_idx) pairs, translated from picker row indices.
#[test]
fn test_confirm_with_selection_returns_execute_split_out_hunks() {
    let mut app = make_app_in_hunks_select(0, &[0, 2]);

    let result = views::split_hunks_select::handle_key(KeyCommand::Confirm, &mut app);

    match result {
        AppAction::ExecuteSplitOutHunks {
            commit_oid,
            mut hunks,
            context_lines,
        } => {
            assert_eq!(commit_oid, Oid::from("111111111111"));
            hunks.sort();
            assert_eq!(hunks, vec![(0, 0), (1, 0)]);
            assert_eq!(context_lines, git_tailor::repo::DEFAULT_CONTEXT_LINES);
        }
        other => panic!("Expected ExecuteSplitOutHunks, got {:?}", other),
    }
    assert_eq!(app.mode, AppMode::CommitList);
}

/// `+` requests a refresh at one more context line than the picker currently
/// has loaded — the dialog itself doesn't own the repository call, so it just
/// asks main.rs to redo `PrepareSplitOutHunks` at the new level.
#[test]
fn test_increase_context_requests_refresh_at_context_plus_one() {
    let mut app = make_app_in_hunks_select_with_context(0, &[], 3);

    let result = views::split_hunks_select::handle_key(KeyCommand::IncreaseContext, &mut app);

    match result {
        AppAction::PrepareSplitOutHunks {
            commit_oid,
            context_lines,
        } => {
            assert_eq!(commit_oid, Oid::from("111111111111"));
            assert_eq!(context_lines, 4);
        }
        other => panic!("Expected PrepareSplitOutHunks, got {:?}", other),
    }
}

/// `-` requests a refresh at one fewer context line, floored at 0.
#[test]
fn test_decrease_context_requests_refresh_at_context_minus_one() {
    let mut app = make_app_in_hunks_select_with_context(0, &[], 3);

    let result = views::split_hunks_select::handle_key(KeyCommand::DecreaseContext, &mut app);

    match result {
        AppAction::PrepareSplitOutHunks { context_lines, .. } => {
            assert_eq!(context_lines, 2);
        }
        other => panic!("Expected PrepareSplitOutHunks, got {:?}", other),
    }
}

#[test]
fn test_decrease_context_floors_at_zero() {
    let mut app = make_app_in_hunks_select_with_context(0, &[], 0);

    let result = views::split_hunks_select::handle_key(KeyCommand::DecreaseContext, &mut app);

    match result {
        AppAction::PrepareSplitOutHunks { context_lines, .. } => {
            assert_eq!(context_lines, 0);
        }
        other => panic!("Expected PrepareSplitOutHunks, got {:?}", other),
    }
}

/// Confirming with nothing explicitly marked falls back to just the hunk
/// under the cursor, so a single hunk can be split out in one keypress.
#[test]
fn test_confirm_with_no_selection_falls_back_to_cursor() {
    let mut app = make_app_in_hunks_select(1, &[]);

    let result = views::split_hunks_select::handle_key(KeyCommand::Confirm, &mut app);

    match result {
        AppAction::ExecuteSplitOutHunks { hunks, .. } => {
            assert_eq!(hunks, vec![(0, 1)]);
        }
        other => panic!("Expected ExecuteSplitOutHunks, got {:?}", other),
    }
}

/// Esc cancels the picker and returns to CommitList without executing anything.
#[test]
fn test_cancel_returns_to_commit_list() {
    let mut app = make_app_in_hunks_select(0, &[0]);

    let result = views::split_hunks_select::handle_key(KeyCommand::Quit, &mut app);

    assert!(matches!(result, AppAction::Handled));
    assert_eq!(app.mode, AppMode::CommitList);
}

fn preview_scroll(app: &AppState) -> (usize, usize) {
    match &app.mode {
        AppMode::SplitHunksSelect {
            preview_h_scroll,
            preview_v_scroll,
            ..
        } => (*preview_h_scroll, *preview_v_scroll),
        other => panic!("Expected SplitHunksSelect, got {:?}", other),
    }
}

/// `←`/`→` scroll the diff preview horizontally — plain arrows are already
/// taken by the hunk list cursor, so this (like the commit detail view) uses
/// unmodified Left/Right rather than a modifier key.
#[test]
fn test_scroll_left_right_adjust_preview_h_scroll() {
    let mut app = make_app_in_hunks_select(0, &[]);

    views::split_hunks_select::handle_key(KeyCommand::ScrollRight, &mut app);
    views::split_hunks_select::handle_key(KeyCommand::ScrollRight, &mut app);
    assert_eq!(preview_scroll(&app), (2, 0));

    views::split_hunks_select::handle_key(KeyCommand::ScrollLeft, &mut app);
    assert_eq!(preview_scroll(&app), (1, 0));
}

#[test]
fn test_scroll_left_floors_at_zero() {
    let mut app = make_app_in_hunks_select(0, &[]);

    views::split_hunks_select::handle_key(KeyCommand::ScrollLeft, &mut app);

    assert_eq!(preview_scroll(&app), (0, 0));
}

/// `Ctrl-↑`/`Ctrl-↓` scroll the diff preview vertically — plain arrows move
/// the hunk list cursor, so this reuses the same keys the commit list already
/// uses for "scroll without moving the primary selection".
#[test]
fn test_ctrl_up_down_adjust_preview_v_scroll() {
    let mut app = make_app_in_hunks_select(0, &[]);

    views::split_hunks_select::handle_key(KeyCommand::ScrollListDown, &mut app);
    views::split_hunks_select::handle_key(KeyCommand::ScrollListDown, &mut app);
    assert_eq!(preview_scroll(&app), (0, 2));

    views::split_hunks_select::handle_key(KeyCommand::ScrollListUp, &mut app);
    assert_eq!(preview_scroll(&app), (0, 1));
}

#[test]
fn test_ctrl_up_floors_at_zero() {
    let mut app = make_app_in_hunks_select(0, &[]);

    views::split_hunks_select::handle_key(KeyCommand::ScrollListUp, &mut app);

    assert_eq!(preview_scroll(&app), (0, 0));
}

/// Moving to a different hunk resets both preview scroll offsets — each
/// hunk's content is unrelated to the last, so a stale scroll position would
/// just look like the pane is confusingly clipped for no reason.
#[test]
fn test_moving_hunk_cursor_resets_preview_scroll() {
    let mut app = make_app_in_hunks_select(0, &[]);
    views::split_hunks_select::handle_key(KeyCommand::ScrollRight, &mut app);
    views::split_hunks_select::handle_key(KeyCommand::ScrollListDown, &mut app);
    assert_eq!(preview_scroll(&app), (1, 1));

    views::split_hunks_select::handle_key(KeyCommand::MoveDown, &mut app);

    assert_eq!(preview_scroll(&app), (0, 0));
}

/// Rendering clamps an out-of-range scroll offset down to the content's
/// actual size and writes the clamped value back, so repeatedly scrolling
/// past the edge doesn't let the stored offset grow unbounded.
#[test]
fn test_render_clamps_preview_scroll_to_content_size() {
    let mut harness = TuiTestHarness::typical();
    let mut app = make_app_in_hunks_select(0, &[]);
    if let AppMode::SplitHunksSelect {
        preview_h_scroll,
        preview_v_scroll,
        ..
    } = &mut app.mode
    {
        *preview_h_scroll = 9999;
        *preview_v_scroll = 9999;
    }

    harness.render(|frame| {
        views::split_hunks_select::render(&mut app, frame);
    });

    let (h, v) = preview_scroll(&app);
    assert!(h < 9999, "h_scroll should be clamped, got {h}");
    assert!(v < 9999, "v_scroll should be clamped, got {v}");
}

/// Snapshot: many hunks (more than fit in the list pane) show a vertical
/// scrollbar, and a hunk with a long line shows a horizontal scrollbar in the
/// preview pane.
#[test]
fn test_split_hunks_select_scrollbars_visible() {
    let mut harness = TuiTestHarness::typical();
    let mut app = make_app_in_hunks_select(0, &[]);

    let mut hunks: Vec<HunkPickerEntry> = (0..30)
        .map(|i| HunkPickerEntry {
            delta_idx: 0,
            hunk_idx: i,
            file_path: format!("file{i}.txt"),
            hunk: hunk((i as u32) * 20 + 1, "short", "short"),
        })
        .collect();
    hunks[0].hunk = Hunk {
        old_start: 1,
        old_lines: 1,
        new_start: 1,
        new_lines: 1,
        lines: vec![
            DiffLine {
                kind: DiffLineKind::Deletion,
                content: format!("{}\n", "x".repeat(200)),
            },
            DiffLine {
                kind: DiffLineKind::Addition,
                content: format!("{}\n", "y".repeat(200)),
            },
        ],
    };
    if let AppMode::SplitHunksSelect { hunks: h, .. } = &mut app.mode {
        *h = hunks;
    }

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::split_hunks_select::render(&mut app, frame);
    }));
}
