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

use git_tailor::{
    Oid, VirtualOid,
    app::AppState,
    fragmap::{FileSpan, FragMap, SpanCluster, SquashableScope, TouchKind},
    views,
    views::theme::Theme,
};
use ratatui::{Terminal, backend::TestBackend};

fn make_fragmap() -> FragMap {
    let cluster0 = SpanCluster {
        spans: vec![FileSpan {
            path: "config.rs".to_string(),
            start_line: 10,
            end_line: 20,
        }],
        commit_oids: vec![
            VirtualOid::Real(Oid::from("aaa1")),
            VirtualOid::Real(Oid::from("ccc3")),
        ],
    };
    let cluster1 = SpanCluster {
        spans: vec![FileSpan {
            path: "parser.rs".to_string(),
            start_line: 1,
            end_line: 50,
        }],
        commit_oids: vec![
            VirtualOid::Real(Oid::from("aaa1")),
            VirtualOid::Real(Oid::from("bbb2")),
            VirtualOid::Real(Oid::from("ccc3")),
        ],
    };
    let cluster2 = SpanCluster {
        spans: vec![FileSpan {
            path: "unique.rs".to_string(),
            start_line: 1,
            end_line: 5,
        }],
        commit_oids: vec![VirtualOid::Real(Oid::from("ccc3"))],
    };
    FragMap {
        commits: vec![
            VirtualOid::Real(Oid::from("aaa1")),
            VirtualOid::Real(Oid::from("bbb2")),
            VirtualOid::Real(Oid::from("ccc3")),
        ],
        clusters: vec![cluster0, cluster1, cluster2],
        matrix: vec![
            // A: touches cluster0 and cluster1, not cluster2
            vec![TouchKind::Added, TouchKind::Added, TouchKind::None],
            // B: touches cluster1 only
            vec![TouchKind::None, TouchKind::Modified, TouchKind::None],
            // C: touches all three clusters
            vec![TouchKind::Modified, TouchKind::Modified, TouchKind::Added],
        ],
    }
}

fn make_app(theme: Theme, focus: usize) -> AppState {
    let mut app = AppState::new();
    app.commits = vec![
        common::create_test_commit("aaa1", "Add config and parser"),
        common::create_test_commit("bbb2", "Refactor parser"),
        common::create_test_commit("ccc3", "Fix config, parser, unique"),
    ];
    app.selection_index = focus;
    app.squashable_scope = SquashableScope::Commit;
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
    let backend = TestBackend::new(80, 10);
    let mut terminal = Terminal::new(backend.clone()).unwrap();
    let mut app = make_app(Theme::Plain, 0);

    terminal
        .draw(|frame| views::commit_list::render(&mut app, frame))
        .unwrap();

    insta::assert_debug_snapshot!(terminal.backend().buffer().clone());
}

/// PlainTheme: focus on commit C (idx 2).
/// Swaps which squares are Current vs Related relative to the first test.
#[test]
fn test_theme_plain_focus_last() {
    let backend = TestBackend::new(80, 10);
    let mut terminal = Terminal::new(backend.clone()).unwrap();
    let mut app = make_app(Theme::Plain, 2);

    terminal
        .draw(|frame| views::commit_list::render(&mut app, frame))
        .unwrap();

    insta::assert_debug_snapshot!(terminal.backend().buffer().clone());
}

// ---------------------------------------------------------------------------
// ClassicTheme
// ---------------------------------------------------------------------------

/// ClassicTheme: focus on commit A. Squares render as spaces with colored
/// backgrounds; connectors use colored backgrounds too.
#[test]
fn test_theme_classic_focus_first() {
    let backend = TestBackend::new(80, 10);
    let mut terminal = Terminal::new(backend.clone()).unwrap();
    let mut app = make_app(Theme::Classic, 0);

    terminal
        .draw(|frame| views::commit_list::render(&mut app, frame))
        .unwrap();

    insta::assert_debug_snapshot!(terminal.backend().buffer().clone());
}

/// ClassicTheme: focus on commit C.
#[test]
fn test_theme_classic_focus_last() {
    let backend = TestBackend::new(80, 10);
    let mut terminal = Terminal::new(backend.clone()).unwrap();
    let mut app = make_app(Theme::Classic, 2);

    terminal
        .draw(|frame| views::commit_list::render(&mut app, frame))
        .unwrap();

    insta::assert_debug_snapshot!(terminal.backend().buffer().clone());
}

// ---------------------------------------------------------------------------
// HighlightTheme
// ---------------------------------------------------------------------------

/// HighlightTheme: focus on commit A (idx 0).
/// Cluster 0 and 1 are related to A → full squares/heavy connectors.
/// Cluster 2 is unrelated to A → medium square (◼) for C's cell.
#[test]
fn test_theme_highlight_focus_first() {
    let backend = TestBackend::new(80, 10);
    let mut terminal = Terminal::new(backend.clone()).unwrap();
    let mut app = make_app(Theme::Highlight, 0);

    terminal
        .draw(|frame| views::commit_list::render(&mut app, frame))
        .unwrap();

    insta::assert_debug_snapshot!(terminal.backend().buffer().clone());
}

/// HighlightTheme: focus on commit C (idx 2).
/// All three clusters are touched by C → all columns are Related/Current;
/// no Unrelated squares appear.
#[test]
fn test_theme_highlight_focus_last() {
    let backend = TestBackend::new(80, 10);
    let mut terminal = Terminal::new(backend.clone()).unwrap();
    let mut app = make_app(Theme::Highlight, 2);

    terminal
        .draw(|frame| views::commit_list::render(&mut app, frame))
        .unwrap();

    insta::assert_debug_snapshot!(terminal.backend().buffer().clone());
}

/// HighlightTheme: focus on commit B (idx 1).
/// B only touches cluster1 → cluster0 and cluster2 are Unrelated columns;
/// cluster1 is Related/Current for B.
#[test]
fn test_theme_highlight_focus_middle() {
    let backend = TestBackend::new(80, 10);
    let mut terminal = Terminal::new(backend.clone()).unwrap();
    let mut app = make_app(Theme::Highlight, 1);

    terminal
        .draw(|frame| views::commit_list::render(&mut app, frame))
        .unwrap();

    insta::assert_debug_snapshot!(terminal.backend().buffer().clone());
}
