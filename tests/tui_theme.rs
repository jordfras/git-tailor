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

// Snapshot tests verifying that PlainTheme, ClassicTheme, and HighlightTheme
// each render all role/relation combinations correctly.
//
// Shared fragmap layout (3 commits × 3 clusters):
//
//   Commits:  A (idx 0)  B (idx 1)  C (idx 2)
//   Cluster 0 (squashable): A=Added,    B=connector, C=Modified
//   Cluster 1 (conflicting): A=Added,   B=Modified,  C=Modified
//   Cluster 2 (C only):     A=none,     B=none,      C=Added
//
// With default focus=0 (A):
//   Cluster 0 — A: Current/Squashable  B: Related/Squashable connector
//               C: Related/Squashable
//   Cluster 1 — A: Current/Conflict    B: Related/Conflict connector
//               C: Related/Conflict
//   Cluster 2 — A: no cell             B: no connector (nothing above in col)
//               C: Unrelated/Conflict  (C's only cluster → Conflict per Commit scope)
//
// With focus=2 (C) in HighlightTheme:
//   Cluster 0 — A: Related/Squashable  B: Related/Squashable connector
//               C: Current/Squashable
//   Cluster 1 — A: Related/Conflict    B: Related/Conflict connector
//               C: Current/Conflict
//   Cluster 2 — A: Unrelated (no cell) B: Unrelated (no connector)
//               C: Current/Conflict

#[allow(dead_code)]
mod common;
use common::{TuiTestHarness, create_fragmap, simple_cluster};

use git_tailor::{app::AppState, fragmap::TouchKind, views, views::theme::Theme};
use ratatui::style::Color;

fn make_fragmap() -> git_tailor::fragmap::FragMap {
    create_fragmap(
        vec!["aaa1", "bbb2", "ccc3"],
        vec![
            simple_cluster("config.rs", 10, 20, &["aaa1", "ccc3"]),
            simple_cluster("parser.rs", 1, 50, &["aaa1", "bbb2", "ccc3"]),
            simple_cluster("unique.rs", 1, 5, &["ccc3"]),
        ],
        vec![
            // A: touches cluster0 and cluster1, not cluster2
            vec![TouchKind::Added, TouchKind::Added, TouchKind::None],
            // B: touches cluster1 only
            vec![TouchKind::None, TouchKind::Modified, TouchKind::None],
            // C: touches all three clusters
            vec![TouchKind::Modified, TouchKind::Modified, TouchKind::Added],
        ],
    )
}

fn make_app(theme: Theme, focus: usize) -> AppState {
    let mut app = AppState::new();
    app.list.commits = vec![
        common::create_test_commit("aaa1", "Add config and parser"),
        common::create_test_commit("bbb2", "Refactor parser"),
        common::create_test_commit("ccc3", "Fix config, parser, unique"),
    ];
    app.list.selection_index = focus;
    app.fragmap = Some(make_fragmap());
    app.theme = theme;
    app
}

// ---------------------------------------------------------------------------
// PlainTheme
// ---------------------------------------------------------------------------

/// PlainTheme: focus on commit A (idx 0).
/// Covers Current+Squashable, Current+Conflict, Related+Squashable,
/// Related+Conflict, Related squashable/conflict connectors, and the
/// Unrelated square (C in cluster2 — C is not fully squashable under Commit
/// scope because cluster2 has no earlier partner).
#[test]
fn test_theme_plain_focus_first() {
    let mut harness = TuiTestHarness::short();
    let mut app = make_app(Theme::Plain, 0);

    insta::assert_debug_snapshot!(
        harness.render(|frame| views::commit_list::render(&mut app, frame))
    );
}

/// PlainTheme: focus on commit C (idx 2).
/// Swaps which squares are Current vs Related relative to the first test.
#[test]
fn test_theme_plain_focus_last() {
    let mut harness = TuiTestHarness::short();
    let mut app = make_app(Theme::Plain, 2);

    insta::assert_debug_snapshot!(
        harness.render(|frame| views::commit_list::render(&mut app, frame))
    );
}

// ---------------------------------------------------------------------------
// ClassicTheme
// ---------------------------------------------------------------------------

/// ClassicTheme: focus on commit A. Squares render as spaces with colored
/// backgrounds; connectors use colored backgrounds too.
#[test]
fn test_theme_classic_focus_first() {
    let mut harness = TuiTestHarness::short();
    let mut app = make_app(Theme::Classic, 0);

    insta::assert_debug_snapshot!(
        harness.render(|frame| views::commit_list::render(&mut app, frame))
    );
}

/// ClassicTheme: focus on commit C.
#[test]
fn test_theme_classic_focus_last() {
    let mut harness = TuiTestHarness::short();
    let mut app = make_app(Theme::Classic, 2);

    insta::assert_debug_snapshot!(
        harness.render(|frame| views::commit_list::render(&mut app, frame))
    );
}

// ---------------------------------------------------------------------------
// HighlightTheme
// ---------------------------------------------------------------------------

/// HighlightTheme: focus on commit A (idx 0).
/// Cluster 0 and 1 are related to A → full squares/heavy connectors.
/// Cluster 2 is unrelated to A → medium square (◼) for C's cell.
#[test]
fn test_theme_highlight_focus_first() {
    let mut harness = TuiTestHarness::short();
    let mut app = make_app(Theme::Highlight, 0);

    insta::assert_debug_snapshot!(
        harness.render(|frame| views::commit_list::render(&mut app, frame))
    );
}

/// HighlightTheme: focus on commit C (idx 2).
/// All three clusters are touched by C → all columns are Related/Current;
/// no Unrelated squares appear.
#[test]
fn test_theme_highlight_focus_last() {
    let mut harness = TuiTestHarness::short();
    let mut app = make_app(Theme::Highlight, 2);

    insta::assert_debug_snapshot!(
        harness.render(|frame| views::commit_list::render(&mut app, frame))
    );
}

/// Two commits that each conflict with their own predecessor, so both shades of
/// red are on screen at once.
///
///   cluster0: A, C, D      cluster1: B, C, D      cluster2: A, D
///
/// C leads back to A in one column and B in the other, so it can fold into
/// neither; D likewise leads back to C twice and A once. The shared fixture
/// above cannot show this — it has only one commit that conflicts, so the only
/// other red on screen is a connector.
fn make_two_conflict_fragmap() -> git_tailor::fragmap::FragMap {
    create_fragmap(
        vec!["aaa1", "bbb2", "ccc3", "ddd4"],
        vec![
            simple_cluster("config.rs", 10, 20, &["aaa1", "ccc3", "ddd4"]),
            simple_cluster("parser.rs", 1, 50, &["bbb2", "ccc3", "ddd4"]),
            simple_cluster("unique.rs", 1, 5, &["aaa1", "ddd4"]),
        ],
        vec![
            vec![TouchKind::Added, TouchKind::None, TouchKind::Added],
            vec![TouchKind::None, TouchKind::Added, TouchKind::None],
            vec![TouchKind::Modified, TouchKind::Modified, TouchKind::None],
            vec![
                TouchKind::Modified,
                TouchKind::Modified,
                TouchKind::Modified,
            ],
        ],
    )
}

/// A conflict on the selected commit's own row is a brighter red than the same
/// relation between two commits below it. Both shades have to appear in one
/// render for the distinction to be worth anything, which is what this asserts
/// -- a snapshot covers it only incidentally, and would go on passing if the
/// two collapsed to one color.
#[test]
fn test_theme_highlight_conflict_square_brighter_on_selected_row() {
    let mut harness = TuiTestHarness::short();
    let mut app = AppState::new();
    app.list.commits = vec![
        common::create_test_commit("aaa1", "Add config and parser"),
        common::create_test_commit("bbb2", "Refactor parser"),
        common::create_test_commit("ccc3", "Fix config and parser"),
        common::create_test_commit("ddd4", "Tidy up all three"),
    ];
    app.list.selection_index = 2;
    app.fragmap = Some(make_two_conflict_fragmap());
    app.theme = Theme::Highlight;
    let buffer = harness.render(|frame| views::commit_list::render(&mut app, frame));

    let mut on_selected_row = Vec::new();
    let mut on_other_rows = Vec::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            let cell = buffer.cell((x, y)).unwrap();
            // Squares only: connectors are red too, and are drawn with a
            // different glyph.
            if cell.symbol() != "\u{2588}" {
                continue;
            }
            match cell.fg {
                Color::LightRed => on_selected_row.push((x, y)),
                Color::Red => on_other_rows.push((x, y)),
                _ => {}
            }
        }
    }

    assert!(
        !on_selected_row.is_empty(),
        "expected a bright red square on the selected commit's row"
    );
    assert!(
        !on_other_rows.is_empty(),
        "expected a plain red square on another commit's row, so the two shades \
         can be compared"
    );

    // Against the selected commit's own row, found by its SHA rather than
    // assumed from the layout. Checking only that the two shades fall on
    // different rows would pass just as well with the roles swapped, which is
    // the one thing this test exists to rule out.
    let selected_y = (0..buffer.area.height)
        .find(|&y| row_text(&buffer, y).contains("ccc3"))
        .expect("the selected commit is on screen");
    for (_, y) in &on_selected_row {
        assert_eq!(
            *y, selected_y,
            "the brighter red belongs to the selected commit's row"
        );
    }
    for (_, y) in &on_other_rows {
        assert_ne!(
            *y, selected_y,
            "a plain red square turned up on the selected row"
        );
    }
}

/// The text of one rendered row, for finding a commit by what it says.
fn row_text(buffer: &ratatui::buffer::Buffer, y: u16) -> String {
    (0..buffer.area.width)
        .map(|x| buffer.cell((x, y)).unwrap().symbol())
        .collect()
}

/// HighlightTheme: focus on commit B (idx 1).
/// B only touches cluster1 → cluster0 and cluster2 are Unrelated columns;
/// cluster1 is Related/Current for B.
#[test]
fn test_theme_highlight_focus_middle() {
    let mut harness = TuiTestHarness::short();
    let mut app = make_app(Theme::Highlight, 1);

    insta::assert_debug_snapshot!(
        harness.render(|frame| views::commit_list::render(&mut app, frame))
    );
}
