// TUI snapshot tests with TestBackend

use git_scissors::{
    app::AppState,
    fragmap::{FileSpan, FragMap, SpanCluster, TouchKind},
    views, CommitInfo,
};
use ratatui::{backend::TestBackend, Terminal};

fn create_test_commit(oid: &str, summary: &str) -> CommitInfo {
    CommitInfo {
        oid: oid.to_string(),
        summary: summary.to_string(),
        author: "Test Author <test@example.com>".to_string(),
        date: "2024-01-15 10:30:00".to_string(),
        parent_oids: vec!["parent123".to_string()],
        message: summary.to_string(),
        author_email: "test@example.com".to_string(),
        author_date: time::OffsetDateTime::from_unix_timestamp(1705318200).unwrap(),
        committer: "Test Committer".to_string(),
        committer_email: "committer@example.com".to_string(),
        commit_date: time::OffsetDateTime::from_unix_timestamp(1705318200).unwrap(),
    }
}

#[test]
fn test_commit_list_empty() {
    let backend = TestBackend::new(80, 10);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    let mut app = AppState::new();

    terminal
        .draw(|frame| {
            views::commit_list::render(&mut app, frame);
        })
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    insta::assert_debug_snapshot!(buffer);
}

#[test]
fn test_commit_list_with_commits() {
    let backend = TestBackend::new(80, 15);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    let mut app = AppState::new();
    app.commits = vec![
        create_test_commit("abc123def456", "Initial commit"),
        create_test_commit("def456ghi789", "Add feature X"),
        create_test_commit("ghi789jkl012", "Fix bug in parser"),
    ];
    app.selection_index = 0;

    terminal
        .draw(|frame| {
            views::commit_list::render(&mut app, frame);
        })
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    insta::assert_debug_snapshot!(buffer);
}

#[test]
fn test_commit_list_with_selection() {
    let backend = TestBackend::new(80, 15);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    let mut app = AppState::new();
    app.commits = vec![
        create_test_commit("abc123def456", "Initial commit"),
        create_test_commit("def456ghi789", "Add feature X"),
        create_test_commit("ghi789jkl012", "Fix bug in parser"),
    ];
    app.selection_index = 1;

    terminal
        .draw(|frame| {
            views::commit_list::render(&mut app, frame);
        })
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    insta::assert_debug_snapshot!(buffer);
}

#[test]
fn test_commit_list_long_summary() {
    let backend = TestBackend::new(80, 12);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    let mut app = AppState::new();
    app.commits = vec![
        create_test_commit(
            "abc123def456",
            "This is a very long commit summary that exceeds normal length",
        ),
        create_test_commit("def456ghi789", "Short"),
    ];
    app.selection_index = 0;

    terminal
        .draw(|frame| {
            views::commit_list::render(&mut app, frame);
        })
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    insta::assert_debug_snapshot!(buffer);
}

#[test]
fn test_commit_list_scrolled_to_top() {
    // Terminal height 8: borders(2) + header(1) = 3 overhead, so 5 rows visible.
    // 10 commits total → scrollbar should appear.
    let backend = TestBackend::new(60, 8);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    let mut app = AppState::new();
    app.commits = (0..10)
        .map(|i| create_test_commit(&format!("{:012x}", i), &format!("Commit number {}", i)))
        .collect();
    app.selection_index = 0;

    terminal
        .draw(|frame| {
            views::commit_list::render(&mut app, frame);
        })
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    insta::assert_debug_snapshot!(buffer);
}

#[test]
fn test_commit_list_scrolled_to_bottom() {
    // Same setup as above, but selection at last commit.
    let backend = TestBackend::new(60, 8);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    let mut app = AppState::new();
    app.commits = (0..10)
        .map(|i| create_test_commit(&format!("{:012x}", i), &format!("Commit number {}", i)))
        .collect();
    app.selection_index = 9;

    terminal
        .draw(|frame| {
            views::commit_list::render(&mut app, frame);
        })
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    insta::assert_debug_snapshot!(buffer);
}

#[test]
fn test_commit_list_reversed_with_commits() {
    let backend = TestBackend::new(80, 15);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    let mut app = AppState::new();
    app.commits = vec![
        create_test_commit("abc123def456", "Initial commit"),
        create_test_commit("def456ghi789", "Add feature X"),
        create_test_commit("ghi789jkl012", "Fix bug in parser"),
    ];
    app.selection_index = 2; // HEAD
    app.reverse = true;

    terminal
        .draw(|frame| {
            views::commit_list::render(&mut app, frame);
        })
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    insta::assert_debug_snapshot!(buffer);
}

#[test]
fn test_commit_list_reversed_scrolled() {
    // 10 commits, reverse mode with HEAD selected (index 9) → visual index 0 (top).
    let backend = TestBackend::new(60, 8);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    let mut app = AppState::new();
    app.commits = (0..10)
        .map(|i| create_test_commit(&format!("{:012x}", i), &format!("Commit number {}", i)))
        .collect();
    app.selection_index = 9; // HEAD
    app.reverse = true;

    terminal
        .draw(|frame| {
            views::commit_list::render(&mut app, frame);
        })
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    insta::assert_debug_snapshot!(buffer);
}

// --- Fragmap rendering tests ---

/// Build a FragMap with the given commit OIDs, clusters, and matrix.
fn create_fragmap(
    commit_oids: Vec<&str>,
    clusters: Vec<SpanCluster>,
    matrix: Vec<Vec<TouchKind>>,
) -> FragMap {
    FragMap {
        commits: commit_oids.into_iter().map(String::from).collect(),
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
        commit_oids: oids.iter().map(|s| s.to_string()).collect(),
    }
}

/// Two commits touching the same cluster with no commits in between → squashable.
/// Expects gray squares with yellow connector.
#[test]
fn test_fragmap_squashable_pair() {
    let backend = TestBackend::new(80, 10);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    let mut app = AppState::new();
    app.commits = vec![
        create_test_commit("aaaa11112222", "Add config file"),
        create_test_commit("bbbb33334444", "Unrelated change"),
        create_test_commit("cccc55556666", "Fix config typo"),
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

    terminal
        .draw(|frame| views::commit_list::render(&mut app, frame))
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    insta::assert_debug_snapshot!(buffer);
}

/// Two commits touching the same cluster with a conflicting commit in between.
/// Expects white squares with red connector.
#[test]
fn test_fragmap_conflicting_pair() {
    let backend = TestBackend::new(80, 10);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    let mut app = AppState::new();
    app.commits = vec![
        create_test_commit("aaaa11112222", "Add parser"),
        create_test_commit("bbbb33334444", "Refactor parser"),
        create_test_commit("cccc55556666", "Fix parser bug"),
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

    terminal
        .draw(|frame| views::commit_list::render(&mut app, frame))
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    insta::assert_debug_snapshot!(buffer);
}

/// Mixed columns: one squashable cluster and one conflicting cluster.
/// Tests that each column renders independently.
#[test]
fn test_fragmap_mixed_columns() {
    let backend = TestBackend::new(80, 12);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    let mut app = AppState::new();
    app.commits = vec![
        create_test_commit("aaaa11112222", "Add feature A"),
        create_test_commit("bbbb33334444", "Touch both files"),
        create_test_commit("cccc55556666", "Fix feature A"),
        create_test_commit("dddd77778888", "Polish feature A"),
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

    terminal
        .draw(|frame| views::commit_list::render(&mut app, frame))
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    insta::assert_debug_snapshot!(buffer);
}

/// Fragmap rendering in reversed display order.
/// Verifies that commit-to-fragmap index mapping works correctly.
#[test]
fn test_fragmap_reversed() {
    let backend = TestBackend::new(80, 10);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    let mut app = AppState::new();
    app.commits = vec![
        create_test_commit("aaaa11112222", "Add config file"),
        create_test_commit("bbbb33334444", "Unrelated change"),
        create_test_commit("cccc55556666", "Fix config typo"),
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

    terminal
        .draw(|frame| views::commit_list::render(&mut app, frame))
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    insta::assert_debug_snapshot!(buffer);
}

/// Adjacent commits both touching same cluster (no gap between squares).
#[test]
fn test_fragmap_adjacent_squashable() {
    let backend = TestBackend::new(80, 8);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    let mut app = AppState::new();
    app.commits = vec![
        create_test_commit("aaaa11112222", "Add handler"),
        create_test_commit("bbbb33334444", "Fix handler"),
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

    terminal
        .draw(|frame| views::commit_list::render(&mut app, frame))
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    insta::assert_debug_snapshot!(buffer);
}

/// Narrow terminal with many clusters — verifies horizontal scrolling.
/// Width 40: SHA(10) + gap(1) + Title(min 20) + gap(1) = 32, leaving ~8 chars for fragmap.
/// With 12 clusters and scroll_offset=4, should show clusters 4..12.
#[test]
fn test_fragmap_horizontal_scroll() {
    let backend = TestBackend::new(40, 8);
    let mut terminal = Terminal::new(backend.clone()).unwrap();

    let oids: Vec<&str> = vec!["aaa1", "bbb2", "ccc3"];
    let commits: Vec<CommitInfo> = oids
        .iter()
        .map(|oid| create_test_commit(oid, &format!("Commit {}", oid)))
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

    terminal
        .draw(|frame| views::commit_list::render(&mut app, frame))
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    insta::assert_debug_snapshot!(buffer);
}
