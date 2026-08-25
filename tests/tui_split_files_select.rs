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

// Tests for the "split out file(s)" two-pane file picker dialog.

#[allow(dead_code)]
mod common;
use common::TuiTestHarness;

use git_tailor::{
    DeltaStatus, DiffLine, DiffLineKind, FileDiff, Hunk, Oid,
    app::{AppAction, AppMode, AppState, KeyCommand},
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

fn file_diff(path: &str, status: DeltaStatus, hunks: Vec<Hunk>) -> FileDiff {
    FileDiff {
        old_path: Some(path.to_string()),
        new_path: Some(path.to_string()),
        status,
        is_binary: false,
        hunks,
    }
}

/// Three files: a.txt has two hunks, b.txt and c.txt have one each.
fn make_entries() -> Vec<FileDiff> {
    vec![
        file_diff(
            "a.txt",
            DeltaStatus::Modified,
            vec![
                hunk(1, "a-old-1", "a-new-1"),
                hunk(10, "a-old-2", "a-new-2"),
            ],
        ),
        file_diff(
            "b.txt",
            DeltaStatus::Modified,
            vec![hunk(1, "b-old", "b-new")],
        ),
        file_diff("c.txt", DeltaStatus::Added, vec![hunk(1, "", "c-new")]),
    ]
}

fn make_app_in_files_select(file_index: usize, selected: &[usize]) -> AppState {
    let mut app = common::app_state_from_commit_summaries(&["Change a, b, c", "Add feature X"]);
    app.list.selection_index = 0;
    app.mode = AppMode::SplitFilesSelect {
        commit_oid: Oid::from("111111111111"),
        files: make_entries(),
        file_index,
        selected: selected.iter().copied().collect(),
        preview_h_scroll: 0,
        preview_v_scroll: 0,
    };
    app
}

#[test]
fn test_split_files_select_first_highlighted_none_selected() {
    let mut harness = TuiTestHarness::typical();
    let mut app = make_app_in_files_select(0, &[]);

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::split_files_select::render(&mut app, frame);
    }));
}

#[test]
fn test_split_files_select_second_highlighted_first_selected() {
    let mut harness = TuiTestHarness::typical();
    let mut app = make_app_in_files_select(1, &[0]);

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::split_files_select::render(&mut app, frame);
    }));
}

/// MoveDown steps the cursor to the next file.
#[test]
fn test_move_down_steps_cursor() {
    let mut app = make_app_in_files_select(0, &[]);

    views::split_files_select::handle_key(KeyCommand::MoveDown, &mut app);

    match &app.mode {
        AppMode::SplitFilesSelect { file_index, .. } => assert_eq!(*file_index, 1),
        other => panic!("Expected SplitFilesSelect, got {:?}", other),
    }
}

/// `Space` toggles the file under the cursor in and out of the selection.
#[test]
fn test_toggle_file_select_adds_then_removes() {
    let mut app = make_app_in_files_select(1, &[]);

    views::split_files_select::handle_key(KeyCommand::TogglePickerItem, &mut app);
    match &app.mode {
        AppMode::SplitFilesSelect { selected, .. } => assert!(selected.contains(&1)),
        other => panic!("Expected SplitFilesSelect, got {:?}", other),
    }

    views::split_files_select::handle_key(KeyCommand::TogglePickerItem, &mut app);
    match &app.mode {
        AppMode::SplitFilesSelect { selected, .. } => assert!(!selected.contains(&1)),
        other => panic!("Expected SplitFilesSelect, got {:?}", other),
    }
}

/// Confirming with files explicitly marked splits out exactly those files'
/// paths, translated from picker row indices.
#[test]
fn test_confirm_with_selection_returns_execute_split_out_files() {
    let mut app = make_app_in_files_select(0, &[0, 2]);

    let result = views::split_files_select::handle_key(KeyCommand::Confirm, &mut app);

    match result {
        AppAction::ExecuteSplitOutFiles {
            commit_oid,
            mut file_paths,
        } => {
            assert_eq!(commit_oid, Oid::from("111111111111"));
            file_paths.sort();
            assert_eq!(file_paths, vec!["a.txt".to_string(), "c.txt".to_string()]);
        }
        other => panic!("Expected ExecuteSplitOutFiles, got {:?}", other),
    }
    assert_eq!(app.mode, AppMode::CommitList);
}

/// Confirming with nothing explicitly marked falls back to just the file
/// under the cursor, so a single file can be split out in one keypress.
#[test]
fn test_confirm_with_no_selection_falls_back_to_cursor() {
    let mut app = make_app_in_files_select(1, &[]);

    let result = views::split_files_select::handle_key(KeyCommand::Confirm, &mut app);

    match result {
        AppAction::ExecuteSplitOutFiles { file_paths, .. } => {
            assert_eq!(file_paths, vec!["b.txt".to_string()]);
        }
        other => panic!("Expected ExecuteSplitOutFiles, got {:?}", other),
    }
}

/// A deleted file (no `new_path`) resolves its identity to `old_path` — both
/// for display and for what's sent to the backend on confirm.
#[test]
fn test_confirm_resolves_deleted_file_to_old_path() {
    let mut app = make_app_in_files_select(0, &[]);
    let deleted = FileDiff {
        old_path: Some("removed.txt".to_string()),
        new_path: None,
        status: DeltaStatus::Deleted,
        is_binary: false,
        hunks: vec![hunk(1, "gone", "")],
    };
    if let AppMode::SplitFilesSelect {
        files, file_index, ..
    } = &mut app.mode
    {
        files.push(deleted);
        *file_index = 3; // the newly appended deleted entry
    }

    let result = views::split_files_select::handle_key(KeyCommand::Confirm, &mut app);

    match result {
        AppAction::ExecuteSplitOutFiles { file_paths, .. } => {
            assert_eq!(file_paths, vec!["removed.txt".to_string()]);
        }
        other => panic!("Expected ExecuteSplitOutFiles, got {:?}", other),
    }
}

/// Esc cancels the picker and returns to CommitList without executing anything.
#[test]
fn test_cancel_returns_to_commit_list() {
    let mut app = make_app_in_files_select(0, &[0]);

    let result = views::split_files_select::handle_key(KeyCommand::Quit, &mut app);

    assert!(matches!(result, AppAction::Handled));
    assert_eq!(app.mode, AppMode::CommitList);
}

fn preview_scroll(app: &AppState) -> (usize, usize) {
    match &app.mode {
        AppMode::SplitFilesSelect {
            preview_h_scroll,
            preview_v_scroll,
            ..
        } => (*preview_h_scroll, *preview_v_scroll),
        other => panic!("Expected SplitFilesSelect, got {:?}", other),
    }
}

/// `←`/`→` scroll the diff preview horizontally.
#[test]
fn test_scroll_left_right_adjust_preview_h_scroll() {
    let mut app = make_app_in_files_select(0, &[]);

    views::split_files_select::handle_key(KeyCommand::ScrollRight, &mut app);
    views::split_files_select::handle_key(KeyCommand::ScrollRight, &mut app);
    assert_eq!(preview_scroll(&app), (2, 0));

    views::split_files_select::handle_key(KeyCommand::ScrollLeft, &mut app);
    assert_eq!(preview_scroll(&app), (1, 0));
}

/// `Ctrl-↑`/`Ctrl-↓` scroll the diff preview vertically.
#[test]
fn test_ctrl_up_down_adjust_preview_v_scroll() {
    let mut app = make_app_in_files_select(0, &[]);

    views::split_files_select::handle_key(KeyCommand::ScrollListDown, &mut app);
    views::split_files_select::handle_key(KeyCommand::ScrollListDown, &mut app);
    assert_eq!(preview_scroll(&app), (0, 2));

    views::split_files_select::handle_key(KeyCommand::ScrollListUp, &mut app);
    assert_eq!(preview_scroll(&app), (0, 1));
}

/// Moving to a different file resets both preview scroll offsets.
#[test]
fn test_moving_file_cursor_resets_preview_scroll() {
    let mut app = make_app_in_files_select(0, &[]);
    views::split_files_select::handle_key(KeyCommand::ScrollRight, &mut app);
    views::split_files_select::handle_key(KeyCommand::ScrollListDown, &mut app);
    assert_eq!(preview_scroll(&app), (1, 1));

    views::split_files_select::handle_key(KeyCommand::MoveDown, &mut app);

    assert_eq!(preview_scroll(&app), (0, 0));
}

/// Rendering clamps an out-of-range scroll offset down to the content's
/// actual size and writes the clamped value back.
#[test]
fn test_render_clamps_preview_scroll_to_content_size() {
    let mut harness = TuiTestHarness::typical();
    let mut app = make_app_in_files_select(0, &[]);
    if let AppMode::SplitFilesSelect {
        preview_h_scroll,
        preview_v_scroll,
        ..
    } = &mut app.mode
    {
        *preview_h_scroll = 9999;
        *preview_v_scroll = 9999;
    }

    harness.render(|frame| {
        views::split_files_select::render(&mut app, frame);
    });

    let (h, v) = preview_scroll(&app);
    assert!(h < 9999, "h_scroll should be clamped, got {h}");
    assert!(v < 9999, "v_scroll should be clamped, got {v}");
}

/// Snapshot: many files (more than fit in the list pane) show a vertical
/// scrollbar, and a file with a long line shows a horizontal scrollbar in the
/// preview pane.
#[test]
fn test_split_files_select_scrollbars_visible() {
    let mut harness = TuiTestHarness::typical();
    let mut app = make_app_in_files_select(0, &[]);

    let mut files: Vec<FileDiff> = (0..30)
        .map(|i| {
            file_diff(
                &format!("file{i}.txt"),
                DeltaStatus::Modified,
                vec![hunk((i as u32) * 20 + 1, "short", "short")],
            )
        })
        .collect();
    files[0] = file_diff(
        "file0.txt",
        DeltaStatus::Modified,
        vec![Hunk {
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
        }],
    );
    if let AppMode::SplitFilesSelect { files: f, .. } = &mut app.mode {
        *f = files;
    }

    insta::assert_debug_snapshot!(harness.render(|frame| {
        views::split_files_select::render(&mut app, frame);
    }));
}

/// Characterization: moving the cursor past the bottom of the list pane scrolls
/// by exactly one row, and the last file pins the offset at `max`.
///
/// Asserts against the *measured* visible height rather than a literal, so the
/// test still means something if the dialog's chrome changes.
#[test]
fn test_split_files_select_scrolls_the_cursor_into_view() {
    let mut harness = TuiTestHarness::typical();

    const FILE_COUNT: usize = 30;
    let files: Vec<FileDiff> = (0..FILE_COUNT)
        .map(|i| {
            file_diff(
                &format!("file{i}.txt"),
                DeltaStatus::Modified,
                vec![hunk((i as u32) * 20 + 1, "short", "short")],
            )
        })
        .collect();

    let mut app = common::app_state_from_commit_summaries(&["Change many files", "Add feature X"]);
    app.list.selection_index = 0;
    app.mode = AppMode::SplitFilesSelect {
        commit_oid: Oid::from("111111111111"),
        files,
        file_index: 0,
        selected: Default::default(),
        preview_h_scroll: 0,
        preview_v_scroll: 0,
    };

    // Render once so the list pane's height is measured.
    harness.render(|frame| views::split_files_select::render(&mut app, frame));
    let vh = app.dialog.visible_height;
    assert!(
        vh > 0 && vh < FILE_COUNT,
        "premise: the list must overflow the pane, got vh={vh}"
    );

    // The last row that fits needs no scrolling.
    for _ in 0..vh - 1 {
        views::split_files_select::handle_key(KeyCommand::MoveDown, &mut app);
    }
    assert_eq!(app.dialog.offset, 0, "cursor at vh-1 still fits");

    // One more moves the viewport by exactly one row.
    views::split_files_select::handle_key(KeyCommand::MoveDown, &mut app);
    assert_eq!(app.dialog.offset, 1);

    // Reaching the last file pins the offset at the bottom.
    for _ in 0..FILE_COUNT {
        views::split_files_select::handle_key(KeyCommand::MoveDown, &mut app);
    }
    assert_eq!(app.dialog.offset, FILE_COUNT - vh);
    assert_eq!(app.dialog.offset, app.dialog.max);

    // And back to the top.
    for _ in 0..FILE_COUNT {
        views::split_files_select::handle_key(KeyCommand::MoveUp, &mut app);
    }
    assert_eq!(app.dialog.offset, 0);
}

/// Characterization: before the first render the visible height is unknown, so
/// navigation must leave the offset alone rather than scroll blind.
#[test]
fn test_split_files_select_does_not_scroll_before_the_first_render() {
    let mut app = make_app_in_files_select(0, &[]);
    assert_eq!(app.dialog.visible_height, 0);

    views::split_files_select::handle_key(KeyCommand::MoveDown, &mut app);

    assert_eq!(app.dialog.offset, 0);
}

/// Paging moves by one viewport, not to the very end of the list.
///
/// The picker used to pass its item count as the page size, so PageDown jumped
/// straight to the last file however long the list was — indistinguishable from
/// `End`, and useless for stepping through a big commit.
#[test]
fn test_split_files_select_pages_by_one_viewport() {
    let mut harness = TuiTestHarness::typical();

    const FILE_COUNT: usize = 30;
    let files: Vec<FileDiff> = (0..FILE_COUNT)
        .map(|i| {
            file_diff(
                &format!("file{i}.txt"),
                DeltaStatus::Modified,
                vec![hunk((i as u32) * 20 + 1, "short", "short")],
            )
        })
        .collect();

    let mut app = common::app_state_from_commit_summaries(&["Change many files", "Add feature X"]);
    app.list.selection_index = 0;
    app.mode = AppMode::SplitFilesSelect {
        commit_oid: Oid::from("111111111111"),
        files,
        file_index: 0,
        selected: Default::default(),
        preview_h_scroll: 0,
        preview_v_scroll: 0,
    };

    harness.render(|frame| views::split_files_select::render(&mut app, frame));
    let vh = app.dialog.visible_height;
    assert!(
        vh > 0 && vh < FILE_COUNT,
        "premise: the list must overflow, got vh={vh}"
    );

    views::split_files_select::handle_key(KeyCommand::PageDown, &mut app);

    let expected = vh - 1; // one row of overlap
    match &app.mode {
        AppMode::SplitFilesSelect { file_index, .. } => assert_eq!(
            *file_index, expected,
            "PageDown should advance one viewport, not jump to the end"
        ),
        other => panic!("Expected SplitFilesSelect, got {other:?}"),
    }
}

/// Render the file picker with `files`, cursor on `file_index`, and return the
/// rendered rows.
fn render_picker_rows(files: Vec<FileDiff>, file_index: usize) -> Vec<String> {
    let mut harness = TuiTestHarness::typical();
    let mut app = common::app_state_from_commit_summaries(&["Change files", "Add feature X"]);
    app.list.selection_index = 0;
    app.mode = AppMode::SplitFilesSelect {
        commit_oid: Oid::from("111111111111"),
        files,
        file_index,
        selected: Default::default(),
        preview_h_scroll: 0,
        preview_v_scroll: 0,
    };
    let buf = harness.render(|frame| views::split_files_select::render(&mut app, frame));
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .filter_map(|x| buf.cell((x, y)).map(|c| c.symbol().to_string()))
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect()
}

/// A file with no diff hunks — binary, empty, or mode-only — must say what
/// happened to it, the way the detail view does. Previewing a bare `path:`
/// header with nothing under it reads as an empty change.
#[test]
fn test_preview_marks_a_hunkless_file() {
    for (is_binary, status, expected) in [
        (true, DeltaStatus::Modified, "Binary file differs"),
        (false, DeltaStatus::Added, "(empty file)"),
        (false, DeltaStatus::Modified, "(no content changes)"),
    ] {
        let files = vec![
            FileDiff {
                old_path: Some("hunkless.bin".to_string()),
                new_path: Some("hunkless.bin".to_string()),
                status,
                is_binary,
                hunks: vec![],
            },
            file_diff(
                "b.txt",
                DeltaStatus::Modified,
                vec![hunk(1, "b-old", "b-new")],
            ),
        ];
        let rows = render_picker_rows(files, 0);
        assert!(
            rows.iter().any(|r| r.contains(expected)),
            "expected {expected:?} in the preview, got:\n{}",
            rows.join("\n")
        );
    }
}

/// A file that does have hunks keeps showing them, with no marker.
#[test]
fn test_preview_does_not_mark_a_file_with_hunks() {
    let rows = render_picker_rows(make_entries(), 0);
    assert!(rows.iter().any(|r| r.contains("a-new-1")));
    for marker in [
        "Binary file differs",
        "(empty file)",
        "(no content changes)",
    ] {
        assert!(
            !rows.iter().any(|r| r.contains(marker)),
            "unexpected {marker:?} in the preview"
        );
    }
}
