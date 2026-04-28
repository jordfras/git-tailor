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

// TUI snapshot tests for the fragmap view.

#[allow(dead_code)]
mod common;
use common::TuiTestHarness;

use git_tailor::{
    CommitInfo, Oid, VirtualOid,
    app::AppState,
    fragmap::{FileSpan, FragMap, SpanCluster, SquashableScope, TouchKind},
    views,
};

/// Build a FragMap with the given commit OIDs, clusters, and matrix.
fn create_fragmap(
    commit_oids: Vec<&str>,
    clusters: Vec<SpanCluster>,
    matrix: Vec<Vec<TouchKind>>,
) -> FragMap {
    FragMap {
        commits: commit_oids
            .into_iter()
            .map(|s| VirtualOid::Real(Oid::from(s)))
            .collect(),
        clusters,
        matrix,
    }
}

fn simple_cluster(path: &str, start: u32, end: u32, oids: &[&str]) -> SpanCluster {
    SpanCluster {
        spans: vec![FileSpan {
            path: path.to_string(),
            start_line: start,
            end_line: end,
        }],
        commit_oids: oids
            .iter()
            .copied()
            .map(|s| VirtualOid::Real(Oid::from(s)))
            .collect(),
    }
}

/// Two commits touching the same cluster with no commits in between → squashable.
/// Expects gray squares with yellow connector.
#[test]
fn test_fragmap_squashable_pair() {
    let mut harness = TuiTestHarness::short();

    let mut app = AppState::new();
    app.commits = vec![
        common::create_test_commit("aaaa11112222", "Add config file"),
        common::create_test_commit("bbbb33334444", "Unrelated change"),
        common::create_test_commit("cccc55556666", "Fix config typo"),
    ];
    app.selection_index = 0;

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

/// Two commits touching the same cluster with a conflicting commit in between.
/// Expects white squares with red connector.
#[test]
fn test_fragmap_conflicting_pair() {
    let mut harness = TuiTestHarness::short();

    let mut app = AppState::new();
    app.commits = vec![
        common::create_test_commit("aaaa11112222", "Add parser"),
        common::create_test_commit("bbbb33334444", "Refactor parser"),
        common::create_test_commit("cccc55556666", "Fix parser bug"),
    ];
    app.selection_index = 0;

    // All three commits touch cluster 0 → commits 1 and 2 conflict with commit 0
    app.fragmap = Some(create_fragmap(
        vec!["aaaa11112222", "bbbb33334444", "cccc55556666"],
        vec![simple_cluster(
            "parser.rs",
            10,
            20,
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

/// Mixed columns: one squashable cluster and one conflicting cluster.
/// Tests that each column renders independently.
#[test]
fn test_fragmap_mixed_columns() {
    let mut harness = TuiTestHarness::new(80, 12);

    let mut app = AppState::new();
    app.commits = vec![
        common::create_test_commit("aaaa11112222", "Add feature A"),
        common::create_test_commit("bbbb33334444", "Touch both files"),
        common::create_test_commit("cccc55556666", "Fix feature A"),
        common::create_test_commit("dddd77778888", "Polish feature A"),
    ];
    app.selection_index = 0;

    // Cluster 0: commits 0, 1, 2, 3 all touch → conflicting chain
    // Cluster 1: commits 0 and 3 touch, 1 and 2 don't → squashable
    app.fragmap = Some(create_fragmap(
        vec![
            "aaaa11112222",
            "bbbb33334444",
            "cccc55556666",
            "dddd77778888",
        ],
        vec![
            simple_cluster(
                "feature_a.rs",
                10,
                30,
                &[
                    "aaaa11112222",
                    "bbbb33334444",
                    "cccc55556666",
                    "dddd77778888",
                ],
            ),
            simple_cluster("tests.rs", 1, 10, &["aaaa11112222", "dddd77778888"]),
        ],
        vec![
            vec![TouchKind::Added, TouchKind::Added],
            vec![TouchKind::Modified, TouchKind::None],
            vec![TouchKind::Modified, TouchKind::None],
            vec![TouchKind::Modified, TouchKind::Modified],
        ],
    ));

    insta::assert_debug_snapshot!(
        harness.render(|frame| views::commit_list::render(&mut app, frame))
    );
}

/// Fragmap rendering in reversed display order.
/// Verifies that commit-to-fragmap index mapping works correctly.
#[test]
fn test_fragmap_reversed() {
    let mut harness = TuiTestHarness::short();

    let mut app = AppState::new();
    app.commits = vec![
        common::create_test_commit("aaaa11112222", "Add config file"),
        common::create_test_commit("bbbb33334444", "Unrelated change"),
        common::create_test_commit("cccc55556666", "Fix config typo"),
    ];
    app.selection_index = 2;
    app.reverse = true;

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

/// Full (non-deduplicated) fragmap: two clusters with the same per-commit
/// activation pattern remain as separate columns (--full / deduplicate=false).
/// Visually this is identical to having two independent clusters side by side.
#[test]
fn test_fragmap_full_duplicate_columns_visible() {
    let mut harness = TuiTestHarness::new(80, 8);

    let mut app = AppState::new();
    app.commits = vec![
        common::create_test_commit("aaaa11112222", "Add handler"),
        common::create_test_commit("bbbb33334444", "Unrelated change"),
    ];
    app.selection_index = 0;

    // Two clusters both touched only by commit 0 — identical activation pattern.
    // In deduplicated (default) mode these would merge to one column; in full
    // mode they stay as two separate columns.
    app.fragmap = Some(create_fragmap(
        vec!["aaaa11112222", "bbbb33334444"],
        vec![
            simple_cluster("handler.rs", 1, 10, &["aaaa11112222"]),
            simple_cluster("handler.rs", 100, 110, &["aaaa11112222"]),
        ],
        vec![
            vec![TouchKind::Added, TouchKind::Added],
            vec![TouchKind::None, TouchKind::None],
        ],
    ));

    insta::assert_debug_snapshot!(
        harness.render(|frame| views::commit_list::render(&mut app, frame))
    );
}

#[test]
fn test_fragmap_adjacent_squashable() {
    let mut harness = TuiTestHarness::new(80, 8);

    let mut app = AppState::new();
    app.commits = vec![
        common::create_test_commit("aaaa11112222", "Add handler"),
        common::create_test_commit("bbbb33334444", "Fix handler"),
    ];
    app.selection_index = 0;

    app.fragmap = Some(create_fragmap(
        vec!["aaaa11112222", "bbbb33334444"],
        vec![simple_cluster(
            "handler.rs",
            5,
            15,
            &["aaaa11112222", "bbbb33334444"],
        )],
        vec![vec![TouchKind::Added], vec![TouchKind::Modified]],
    ));

    insta::assert_debug_snapshot!(
        harness.render(|frame| views::commit_list::render(&mut app, frame))
    );
}

/// Narrow terminal with many clusters — verifies horizontal scrolling.
/// Width 40: SHA(10) + gap(1) + Title(min 20) + gap(1) = 32, leaving ~8 chars for fragmap.
/// With 12 clusters and scroll_offset=4, should show clusters 4..12.
#[test]
fn test_fragmap_horizontal_scroll() {
    let mut harness = TuiTestHarness::new(40, 8);

    let oids: Vec<&str> = vec!["aaa1", "bbb2", "ccc3"];
    let commits: Vec<CommitInfo> = oids
        .iter()
        .map(|oid| common::create_test_commit(oid, &format!("Commit {}", oid)))
        .collect();

    // 12 clusters, each touched by exactly one commit
    let clusters: Vec<SpanCluster> = (0u32..12)
        .map(|i| simple_cluster("file.rs", i * 10, i * 10 + 5, &[oids[i as usize % 3]]))
        .collect();

    let matrix: Vec<Vec<TouchKind>> = (0..3)
        .map(|commit_idx| {
            (0..12)
                .map(|cluster_idx| {
                    if cluster_idx % 3 == commit_idx {
                        TouchKind::Added
                    } else {
                        TouchKind::None
                    }
                })
                .collect()
        })
        .collect();

    let mut app = AppState::new();
    app.commits = commits;
    app.selection_index = 0;
    app.fragmap_scroll_offset = 4;
    app.fragmap = Some(create_fragmap(oids, clusters, matrix));

    insta::assert_debug_snapshot!(
        harness.render(|frame| views::commit_list::render(&mut app, frame))
    );
}

/// C touches cluster0 (also touched by A) and cluster2 (only C).
/// With `Group` scope: the A↔C pair in cluster0 is squashable, so
/// B's connector is Yellow and C's cluster0 square is DarkGray.
#[test]
fn test_fragmap_squashable_scope_group() {
    let mut harness = TuiTestHarness::short();

    let mut app = AppState::new();
    app.commits = vec![
        common::create_test_commit("aaaa11112222", "Touch config"),
        common::create_test_commit("bbbb33334444", "Unrelated change"),
        common::create_test_commit("cccc55556666", "Touch config and unique"),
    ];
    app.selection_index = 0;
    app.squashable_scope = SquashableScope::Group;

    // Cluster 0: A and C touch it → squashable pair under Group scope.
    // Cluster 1: B only.
    // Cluster 2: C only (no earlier commit → C is NOT fully squashable under Commit scope).
    app.fragmap = Some(create_fragmap(
        vec!["aaaa11112222", "bbbb33334444", "cccc55556666"],
        vec![
            simple_cluster("config.rs", 10, 20, &["aaaa11112222", "cccc55556666"]),
            simple_cluster("other.rs", 1, 5, &["bbbb33334444"]),
            simple_cluster("unique.rs", 1, 5, &["cccc55556666"]),
        ],
        vec![
            vec![TouchKind::Added, TouchKind::None, TouchKind::None],
            vec![TouchKind::None, TouchKind::Modified, TouchKind::None],
            vec![TouchKind::Modified, TouchKind::None, TouchKind::Modified],
        ],
    ));

    insta::assert_debug_snapshot!(
        harness.render(|frame| views::commit_list::render(&mut app, frame))
    );
}

/// Same fragmap as `test_fragmap_squashable_scope_group` but with `Commit`
/// scope.  Because C also touches cluster2 (no earlier partner) it is NOT
/// fully squashable, so B's connector becomes Red and C's cluster0 square
/// becomes White (not DarkGray).
#[test]
fn test_fragmap_squashable_scope_commit() {
    let mut harness = TuiTestHarness::short();

    let mut app = AppState::new();
    app.commits = vec![
        common::create_test_commit("aaaa11112222", "Touch config"),
        common::create_test_commit("bbbb33334444", "Unrelated change"),
        common::create_test_commit("cccc55556666", "Touch config and unique"),
    ];
    app.selection_index = 0;
    app.squashable_scope = SquashableScope::Commit;

    app.fragmap = Some(create_fragmap(
        vec!["aaaa11112222", "bbbb33334444", "cccc55556666"],
        vec![
            simple_cluster("config.rs", 10, 20, &["aaaa11112222", "cccc55556666"]),
            simple_cluster("other.rs", 1, 5, &["bbbb33334444"]),
            simple_cluster("unique.rs", 1, 5, &["cccc55556666"]),
        ],
        vec![
            vec![TouchKind::Added, TouchKind::None, TouchKind::None],
            vec![TouchKind::None, TouchKind::Modified, TouchKind::None],
            vec![TouchKind::Modified, TouchKind::None, TouchKind::Modified],
        ],
    ));

    insta::assert_debug_snapshot!(
        harness.render(|frame| views::commit_list::render(&mut app, frame))
    );
}
