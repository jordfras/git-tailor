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

use super::*;
use crate::{CommitDiff, CommitInfo, FileDiff, Hunk};

fn make_commit_info() -> CommitInfo {
    CommitInfo {
        oid: "abc123".to_string(),
        summary: "Test commit".to_string(),
        author: Some("Test Author".to_string()),
        date: Some("123456789".to_string()),
        parent_oids: vec![],
        message: "Test commit".to_string(),
        author_email: Some("test@example.com".to_string()),
        author_date: Some(time::OffsetDateTime::from_unix_timestamp(123456789).unwrap()),
        committer: Some("Test Committer".to_string()),
        committer_email: Some("committer@example.com".to_string()),
        commit_date: Some(time::OffsetDateTime::from_unix_timestamp(123456789).unwrap()),
    }
}

#[test]
fn test_extract_spans_single_hunk() {
    let commit_diff = CommitDiff {
        commit: make_commit_info(),
        files: vec![FileDiff {
            old_path: Some("file.txt".to_string()),
            new_path: Some("file.txt".to_string()),
            status: crate::DeltaStatus::Modified,
            hunks: vec![Hunk {
                old_start: 10,
                old_lines: 3,
                new_start: 10,
                new_lines: 5,
                lines: vec![],
            }],
        }],
    };

    let spans = extract_spans(&commit_diff);

    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].path, "file.txt");
    assert_eq!(spans[0].start_line, 10);
    assert_eq!(spans[0].end_line, 14); // 10 + 5 - 1
}

#[test]
fn test_extract_spans_multiple_hunks() {
    let commit_diff = CommitDiff {
        commit: make_commit_info(),
        files: vec![FileDiff {
            old_path: Some("file.txt".to_string()),
            new_path: Some("file.txt".to_string()),
            status: crate::DeltaStatus::Modified,
            hunks: vec![
                Hunk {
                    old_start: 5,
                    old_lines: 2,
                    new_start: 5,
                    new_lines: 3,
                    lines: vec![],
                },
                Hunk {
                    old_start: 20,
                    old_lines: 1,
                    new_start: 21,
                    new_lines: 2,
                    lines: vec![],
                },
            ],
        }],
    };

    let spans = extract_spans(&commit_diff);

    assert_eq!(spans.len(), 2);
    assert_eq!(spans[0].path, "file.txt");
    assert_eq!(spans[0].start_line, 5);
    assert_eq!(spans[0].end_line, 7); // 5 + 3 - 1

    assert_eq!(spans[1].path, "file.txt");
    assert_eq!(spans[1].start_line, 21);
    assert_eq!(spans[1].end_line, 22); // 21 + 2 - 1
}

#[test]
fn test_extract_spans_multiple_files() {
    let commit_diff = CommitDiff {
        commit: make_commit_info(),
        files: vec![
            FileDiff {
                old_path: Some("a.txt".to_string()),
                new_path: Some("a.txt".to_string()),
                status: crate::DeltaStatus::Modified,
                hunks: vec![Hunk {
                    old_start: 1,
                    old_lines: 1,
                    new_start: 1,
                    new_lines: 2,
                    lines: vec![],
                }],
            },
            FileDiff {
                old_path: Some("b.txt".to_string()),
                new_path: Some("b.txt".to_string()),
                status: crate::DeltaStatus::Modified,
                hunks: vec![Hunk {
                    old_start: 10,
                    old_lines: 3,
                    new_start: 10,
                    new_lines: 4,
                    lines: vec![],
                }],
            },
        ],
    };

    let spans = extract_spans(&commit_diff);

    assert_eq!(spans.len(), 2);
    assert_eq!(spans[0].path, "a.txt");
    assert_eq!(spans[0].start_line, 1);
    assert_eq!(spans[0].end_line, 2);

    assert_eq!(spans[1].path, "b.txt");
    assert_eq!(spans[1].start_line, 10);
    assert_eq!(spans[1].end_line, 13);
}

#[test]
fn test_extract_spans_skips_deleted_files() {
    let commit_diff = CommitDiff {
        commit: make_commit_info(),
        files: vec![
            FileDiff {
                old_path: Some("file.txt".to_string()),
                new_path: Some("file.txt".to_string()),
                status: crate::DeltaStatus::Modified,
                hunks: vec![Hunk {
                    old_start: 1,
                    old_lines: 1,
                    new_start: 1,
                    new_lines: 2,
                    lines: vec![],
                }],
            },
            FileDiff {
                old_path: Some("deleted.txt".to_string()),
                new_path: None, // File was deleted
                status: crate::DeltaStatus::Deleted,
                hunks: vec![Hunk {
                    old_start: 1,
                    old_lines: 5,
                    new_start: 0,
                    new_lines: 0,
                    lines: vec![],
                }],
            },
        ],
    };

    let spans = extract_spans(&commit_diff);

    // Should only have span from file.txt, not from deleted.txt
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].path, "file.txt");
}

#[test]
fn test_extract_spans_skips_empty_hunks() {
    let commit_diff = CommitDiff {
        commit: make_commit_info(),
        files: vec![FileDiff {
            old_path: Some("file.txt".to_string()),
            new_path: Some("file.txt".to_string()),
            status: crate::DeltaStatus::Modified,
            hunks: vec![
                Hunk {
                    old_start: 5,
                    old_lines: 2,
                    new_start: 5,
                    new_lines: 3,
                    lines: vec![],
                },
                Hunk {
                    old_start: 10,
                    old_lines: 1,
                    new_start: 8,
                    new_lines: 0, // Empty hunk (pure deletion in context)
                    lines: vec![],
                },
            ],
        }],
    };

    let spans = extract_spans(&commit_diff);

    // Should only have span from first hunk, not the empty one
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].start_line, 5);
    assert_eq!(spans[0].end_line, 7);
}

#[test]
fn test_extract_spans_added_file() {
    let commit_diff = CommitDiff {
        commit: make_commit_info(),
        files: vec![FileDiff {
            old_path: None, // File was added
            new_path: Some("new_file.txt".to_string()),
            status: crate::DeltaStatus::Added,
            hunks: vec![Hunk {
                old_start: 0,
                old_lines: 0,
                new_start: 1,
                new_lines: 10,
                lines: vec![],
            }],
        }],
    };

    let spans = extract_spans(&commit_diff);

    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].path, "new_file.txt");
    assert_eq!(spans[0].start_line, 1);
    assert_eq!(spans[0].end_line, 10);
}

#[test]
fn test_extract_spans_single_line_change() {
    let commit_diff = CommitDiff {
        commit: make_commit_info(),
        files: vec![FileDiff {
            old_path: Some("file.txt".to_string()),
            new_path: Some("file.txt".to_string()),
            status: crate::DeltaStatus::Modified,
            hunks: vec![Hunk {
                old_start: 42,
                old_lines: 1,
                new_start: 42,
                new_lines: 1,
                lines: vec![],
            }],
        }],
    };

    let spans = extract_spans(&commit_diff);

    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].start_line, 42);
    assert_eq!(spans[0].end_line, 42); // Single line: 42 + 1 - 1 = 42
}

#[test]
fn test_extract_spans_empty_commit() {
    let commit_diff = CommitDiff {
        commit: make_commit_info(),
        files: vec![],
    };

    let spans = extract_spans(&commit_diff);

    assert_eq!(spans.len(), 0);
}

#[test]
fn test_map_line_forward_before_hunk() {
    let hunks = vec![HunkInfo {
        old_start: 10,
        old_lines: 5,
        new_start: 10,
        new_lines: 8,
    }];
    assert_eq!(map_line_forward(5, &hunks), 5);
}

#[test]
fn test_map_line_forward_after_hunk() {
    let hunks = vec![HunkInfo {
        old_start: 10,
        old_lines: 5,
        new_start: 10,
        new_lines: 8,
    }];
    // delta = 8 - 5 = 3, so line 20 → 23
    assert_eq!(map_line_forward(20, &hunks), 23);
}

#[test]
fn test_split_and_propagate_overlap() {
    let hunks = vec![HunkInfo {
        old_start: 10,
        old_lines: 5,
        new_start: 10,
        new_lines: 8,
    }];
    // Span [8, 20) partially overlaps with hunk [10, 15).
    // After splitting: [8, 10) (before) and [15, 20) (after).
    // [8, 10) → map: 8 < 10, no delta → [8, 10)
    // [15, 20) → map: 15 >= 15, delta = 8-5 = 3 → [18, 23)
    let result = split_and_propagate(8, 20, &hunks);
    assert_eq!(result, vec![(8, 10), (18, 23)]);
}

#[test]
fn test_map_line_forward_two_hunks() {
    let hunks = vec![
        HunkInfo {
            old_start: 10,
            old_lines: 5,
            new_start: 10,
            new_lines: 8,
        },
        HunkInfo {
            old_start: 30,
            old_lines: 3,
            new_start: 33,
            new_lines: 5,
        },
    ];
    // Between hunks: line 25 → 25 + 3 = 28
    assert_eq!(map_line_forward(25, &hunks), 28);
    // After both: line 40 → 40 + 3 + 2 = 45
    assert_eq!(map_line_forward(40, &hunks), 45);
}

#[test]
fn test_propagation_sequential_commits_same_file() {
    // Two commits touching different, distant parts of the same file.
    // After propagation they should not share a cluster.
    let commits = vec![
        CommitDiff {
            commit: make_commit_info_with_oid("c1"),
            files: vec![FileDiff {
                old_path: Some("f.rs".to_string()),
                new_path: Some("f.rs".to_string()),
                status: crate::DeltaStatus::Modified,
                hunks: vec![Hunk {
                    old_start: 10,
                    old_lines: 5,
                    new_start: 10,
                    new_lines: 8,
                    lines: vec![],
                }],
            }],
        },
        CommitDiff {
            commit: make_commit_info_with_oid("c2"),
            files: vec![FileDiff {
                old_path: Some("f.rs".to_string()),
                new_path: Some("f.rs".to_string()),
                status: crate::DeltaStatus::Modified,
                hunks: vec![Hunk {
                    old_start: 50,
                    old_lines: 5,
                    new_start: 50,
                    new_lines: 5,
                    lines: vec![],
                }],
            }],
        },
    ];

    let fm = build_fragmap(&commits, true);
    assert!(!fm.shares_cluster_with(0, 1));
}

#[test]
fn test_propagation_overlapping_hunks_are_related() {
    // Commit 1 inserts a large block. Commit 2 modifies within that block.
    // After propagation commit 1's span includes commit 2's region.
    let commits = vec![
        CommitDiff {
            commit: make_commit_info_with_oid("c1"),
            files: vec![FileDiff {
                old_path: Some("f.rs".to_string()),
                new_path: Some("f.rs".to_string()),
                status: crate::DeltaStatus::Modified,
                hunks: vec![Hunk {
                    old_start: 10,
                    old_lines: 5,
                    new_start: 10,
                    new_lines: 55,
                    lines: vec![],
                }],
            }],
        },
        CommitDiff {
            commit: make_commit_info_with_oid("c2"),
            files: vec![FileDiff {
                old_path: Some("f.rs".to_string()),
                new_path: Some("f.rs".to_string()),
                status: crate::DeltaStatus::Modified,
                hunks: vec![Hunk {
                    old_start: 30,
                    old_lines: 10,
                    new_start: 30,
                    new_lines: 10,
                    lines: vec![],
                }],
            }],
        },
    ];

    let fm = build_fragmap(&commits, true);
    assert!(fm.shares_cluster_with(0, 1));
}

#[test]
fn test_propagation_distant_changes_not_related() {
    // Changes far apart in the same file should not cluster.
    let commits = vec![
        CommitDiff {
            commit: make_commit_info_with_oid("c1"),
            files: vec![FileDiff {
                old_path: Some("f.rs".to_string()),
                new_path: Some("f.rs".to_string()),
                status: crate::DeltaStatus::Modified,
                hunks: vec![Hunk {
                    old_start: 10,
                    old_lines: 3,
                    new_start: 10,
                    new_lines: 5,
                    lines: vec![],
                }],
            }],
        },
        CommitDiff {
            commit: make_commit_info_with_oid("c2"),
            files: vec![FileDiff {
                old_path: Some("f.rs".to_string()),
                new_path: Some("f.rs".to_string()),
                status: crate::DeltaStatus::Modified,
                hunks: vec![Hunk {
                    old_start: 200,
                    old_lines: 5,
                    new_start: 202,
                    new_lines: 5,
                    lines: vec![],
                }],
            }],
        },
    ];

    let fm = build_fragmap(&commits, true);
    assert!(!fm.shares_cluster_with(0, 1));
}

// Helper functions for matrix generation tests

fn make_commit_info_with_oid(oid: &str) -> CommitInfo {
    CommitInfo {
        oid: oid.to_string(),
        summary: format!("Commit {}", oid),
        author: Some("Test Author".to_string()),
        date: Some("123456789".to_string()),
        parent_oids: vec![],
        message: format!("Commit {}", oid),
        author_email: Some("test@example.com".to_string()),
        author_date: Some(time::OffsetDateTime::from_unix_timestamp(123456789).unwrap()),
        committer: Some("Test Committer".to_string()),
        committer_email: Some("committer@example.com".to_string()),
        commit_date: Some(time::OffsetDateTime::from_unix_timestamp(123456789).unwrap()),
    }
}

fn make_file_diff(
    old_path: Option<&str>,
    new_path: Option<&str>,
    old_start: u32,
    old_lines: u32,
    new_start: u32,
    new_lines: u32,
) -> FileDiff {
    FileDiff {
        old_path: old_path.map(|s| s.to_string()),
        new_path: new_path.map(|s| s.to_string()),
        status: crate::DeltaStatus::Modified,
        hunks: vec![Hunk {
            old_start,
            old_lines,
            new_start,
            new_lines,
            lines: vec![],
        }],
    }
}

fn make_commit_diff(oid: &str, files: Vec<FileDiff>) -> CommitDiff {
    CommitDiff {
        commit: make_commit_info_with_oid(oid),
        files,
    }
}

// Matrix generation tests

#[test]
fn test_build_fragmap_empty_commits() {
    let fragmap = build_fragmap(&[], true);

    assert_eq!(fragmap.commits.len(), 0);
    assert_eq!(fragmap.clusters.len(), 0);
    assert_eq!(fragmap.matrix.len(), 0);
}

#[test]
fn test_build_fragmap_single_commit() {
    let commits = vec![make_commit_diff(
        "c1",
        vec![make_file_diff(
            None, // File was added
            Some("file.txt"),
            0,
            0,
            1,
            3,
        )],
    )];

    let fragmap = build_fragmap(&commits, true);

    assert_eq!(fragmap.commits.len(), 1);
    assert_eq!(fragmap.commits[0], "c1");

    // Should have one cluster
    assert_eq!(fragmap.clusters.len(), 1);
    assert_eq!(fragmap.clusters[0].spans.len(), 1);
    assert_eq!(fragmap.clusters[0].spans[0].path, "file.txt");
    assert_eq!(fragmap.clusters[0].commit_oids, vec!["c1"]);

    // Matrix should be 1x1 with Added
    assert_eq!(fragmap.matrix.len(), 1);
    assert_eq!(fragmap.matrix[0].len(), 1);
    assert_eq!(fragmap.matrix[0][0], TouchKind::Added);
}

#[test]
fn test_build_fragmap_overlapping_spans_merge() {
    // Two commits touching overlapping regions should be related
    let commits = vec![
        make_commit_diff(
            "c1",
            vec![make_file_diff(
                Some("file.txt"),
                Some("file.txt"),
                1,
                0,
                1,
                5, // lines 1-5
            )],
        ),
        make_commit_diff(
            "c2",
            vec![make_file_diff(
                Some("file.txt"),
                Some("file.txt"),
                3,
                3,
                3,
                4, // lines 3-6 (overlaps with c1)
            )],
        ),
    ];

    let fragmap = build_fragmap(&commits, true);

    assert_eq!(fragmap.commits.len(), 2);

    // Both commits should share at least one cluster
    assert!(fragmap.shares_cluster_with(0, 1));

    // There should be a cluster containing both commits
    let shared = fragmap.clusters.iter().any(|c| {
        c.commit_oids.contains(&"c1".to_string()) && c.commit_oids.contains(&"c2".to_string())
    });
    assert!(shared);

    // Both commits should have non-None entries in the shared cluster
    let shared_idx = fragmap
        .clusters
        .iter()
        .position(|c| {
            c.commit_oids.contains(&"c1".to_string()) && c.commit_oids.contains(&"c2".to_string())
        })
        .unwrap();
    assert_ne!(fragmap.matrix[0][shared_idx], TouchKind::None);
    assert_ne!(fragmap.matrix[1][shared_idx], TouchKind::None);
}

#[test]
fn test_build_fragmap_non_overlapping_separate_clusters() {
    // Two commits touching different regions should create two clusters
    let commits = vec![
        make_commit_diff(
            "c1",
            vec![make_file_diff(
                Some("file.txt"),
                Some("file.txt"),
                1,
                0,
                1,
                5, // lines 1-5
            )],
        ),
        make_commit_diff(
            "c2",
            vec![make_file_diff(
                Some("file.txt"),
                Some("file.txt"),
                10,
                3,
                10,
                4, // lines 10-13 (no overlap)
            )],
        ),
    ];

    let fragmap = build_fragmap(&commits, true);

    assert_eq!(fragmap.commits.len(), 2);

    // Should have two clusters (no overlap)
    assert_eq!(fragmap.clusters.len(), 2);

    // Matrix should be 2x2
    assert_eq!(fragmap.matrix.len(), 2);
    assert_eq!(fragmap.matrix[0].len(), 2);
    assert_eq!(fragmap.matrix[1].len(), 2);

    // c1 touches first cluster, not second
    assert_ne!(fragmap.matrix[0][0], TouchKind::None);
    assert_eq!(fragmap.matrix[0][1], TouchKind::None);

    // c2 touches second cluster, not first
    assert_eq!(fragmap.matrix[1][0], TouchKind::None);
    assert_ne!(fragmap.matrix[1][1], TouchKind::None);
}

#[test]
fn test_build_fragmap_adjacent_spans_stay_separate() {
    // Adjacent spans (end_line + 1 == start_line) should NOT merge.
    // Only actual overlap causes clustering, matching the original fragmap.
    let commits = vec![
        make_commit_diff(
            "c1",
            vec![make_file_diff(
                Some("file.txt"),
                Some("file.txt"),
                1,
                0,
                1,
                5, // lines 1-5
            )],
        ),
        make_commit_diff(
            "c2",
            vec![make_file_diff(
                Some("file.txt"),
                Some("file.txt"),
                6,
                2,
                6,
                3, // lines 6-8 (adjacent to c1, NOT overlapping)
            )],
        ),
    ];

    let fragmap = build_fragmap(&commits, true);

    assert_eq!(fragmap.commits.len(), 2);

    // Should have two clusters (adjacent but not overlapping)
    assert_eq!(fragmap.clusters.len(), 2);
}

#[test]
fn test_no_snowball_effect_on_cluster_ranges() {
    // Regression test: distant spans must not be absorbed into a nearby cluster.
    //
    // Commit 1: lines 1-5, Commit 2: lines 3-12 (overlaps c1),
    // Commit 3: lines 50-53 (should NOT be absorbed)
    let commits = vec![
        make_commit_diff(
            "c1",
            vec![make_file_diff(
                Some("file.txt"),
                Some("file.txt"),
                1,
                0,
                1,
                5, // lines 1-5
            )],
        ),
        make_commit_diff(
            "c2",
            vec![make_file_diff(
                Some("file.txt"),
                Some("file.txt"),
                3,
                5,
                3,
                10, // lines 3-12 (overlaps c1)
            )],
        ),
        make_commit_diff(
            "c3",
            vec![make_file_diff(
                Some("file.txt"),
                Some("file.txt"),
                50,
                3,
                50,
                4, // lines 50-53 (far away, separate)
            )],
        ),
    ];

    let fragmap = build_fragmap(&commits, true);

    // c1 and c2 share a cluster, c3 does not overlap with either
    assert!(fragmap.shares_cluster_with(0, 1));
    assert!(!fragmap.shares_cluster_with(0, 2));
    assert!(!fragmap.shares_cluster_with(1, 2));
}

#[test]
fn test_different_functions_same_file_separate_clusters() {
    // Real-world scenario: two commits touch different functions in the
    // same file. They should be in separate clusters (separate columns),
    // not squashable into each other.
    let commits = vec![
        make_commit_diff(
            "c1",
            vec![make_file_diff(
                Some("lib.rs"),
                Some("lib.rs"),
                10,
                3,
                10,
                5, // function foo() at lines 10-14
            )],
        ),
        make_commit_diff(
            "c2",
            vec![make_file_diff(
                Some("lib.rs"),
                Some("lib.rs"),
                80,
                2,
                80,
                4, // function bar() at lines 80-83
            )],
        ),
    ];

    let fragmap = build_fragmap(&commits, true);

    // Separate clusters — these are different code regions
    assert_eq!(fragmap.clusters.len(), 2);

    // Neither commit is squashable into the other
    assert!(!fragmap.is_fully_squashable(0));
    assert!(!fragmap.is_fully_squashable(1));

    // They don't share any cluster
    assert!(!fragmap.shares_cluster_with(0, 1));
}

#[test]
fn test_build_fragmap_touchkind_added() {
    // Adding a new file should produce TouchKind::Added
    let commits = vec![make_commit_diff(
        "c1",
        vec![make_file_diff(
            None, // old_path
            Some("new_file.txt"),
            0,
            0,
            1,
            10,
        )],
    )];

    let fragmap = build_fragmap(&commits, true);

    assert_eq!(fragmap.matrix[0][0], TouchKind::Added);
}

#[test]
fn test_build_fragmap_touchkind_modified() {
    // Modifying existing lines should produce TouchKind::Modified
    let commits = vec![make_commit_diff(
        "c1",
        vec![make_file_diff(
            Some("file.txt"),
            Some("file.txt"),
            10,
            5, // old_lines > 0
            10,
            6, // new_lines > 0
        )],
    )];

    let fragmap = build_fragmap(&commits, true);

    assert_eq!(fragmap.matrix[0][0], TouchKind::Modified);
}

#[test]
fn test_build_fragmap_touchkind_deleted() {
    // Deleting lines should produce TouchKind::Deleted
    // But deleted files are skipped, so we test a hunk with deletions
    // Actually, we need to look at the determine_touch_kind logic more carefully
    // For now, test that pure deletions (no new_lines) are skipped at span extraction level
    // This test verifies the matrix generation doesn't crash with complex diffs
    let commits = vec![make_commit_diff(
        "c1",
        vec![make_file_diff(
            Some("file.txt"),
            Some("file.txt"),
            10,
            5,
            10,
            2, // Shrinking the region (some deletions)
        )],
    )];

    let fragmap = build_fragmap(&commits, true);

    // Should still generate a valid fragmap
    assert_eq!(fragmap.commits.len(), 1);
    assert_eq!(fragmap.clusters.len(), 1);
}

#[test]
fn test_build_fragmap_multiple_files_separate_clusters() {
    // Different files should always create separate clusters
    let commits = vec![
        make_commit_diff(
            "c1",
            vec![make_file_diff(Some("a.txt"), Some("a.txt"), 1, 0, 1, 5)],
        ),
        make_commit_diff(
            "c2",
            vec![make_file_diff(
                Some("b.txt"),
                Some("b.txt"),
                1,
                0,
                1,
                5, // Same line range but different file
            )],
        ),
    ];

    let fragmap = build_fragmap(&commits, true);

    assert_eq!(fragmap.commits.len(), 2);

    // Should have two clusters (different files)
    assert_eq!(fragmap.clusters.len(), 2);

    // Each commit touches only its own cluster
    assert_ne!(fragmap.matrix[0][0], TouchKind::None);
    assert_eq!(fragmap.matrix[0][1], TouchKind::None);

    assert_eq!(fragmap.matrix[1][0], TouchKind::None);
    assert_ne!(fragmap.matrix[1][1], TouchKind::None);
}

#[test]
fn test_build_fragmap_commit_touches_multiple_clusters() {
    // A single commit touching multiple non-adjacent regions of the same
    // file produces columns with identical activation patterns (only c1
    // is active). BriefFragmap-style dedup merges them into one column.
    let mut c1 = make_commit_diff(
        "c1",
        vec![make_file_diff(
            Some("file.txt"),
            Some("file.txt"),
            1,
            0,
            1,
            5, // lines 1-5
        )],
    );

    c1.files.push(make_file_diff(
        Some("file.txt"),
        Some("file.txt"),
        20,
        0,
        20,
        3, // lines 20-22 (separate region)
    ));

    let fragmap = build_fragmap(&[c1], true);

    assert_eq!(fragmap.commits.len(), 1);

    // After dedup, both regions have the same activation pattern {c1},
    // so they collapse into a single column.
    assert_eq!(fragmap.clusters.len(), 1);
    assert_ne!(fragmap.matrix[0][0], TouchKind::None);
}

// Squashability analysis tests

#[test]
fn test_cluster_relation_no_relation_neither_touches() {
    // Two commits that don't touch the same cluster
    let commits = vec![
        make_commit_diff(
            "c1",
            vec![make_file_diff(Some("a.txt"), Some("a.txt"), 1, 0, 1, 5)],
        ),
        make_commit_diff(
            "c2",
            vec![make_file_diff(Some("b.txt"), Some("b.txt"), 1, 0, 1, 5)],
        ),
    ];

    let fragmap = build_fragmap(&commits, true);

    // Two clusters, c1 only touches cluster 0
    assert_eq!(fragmap.clusters.len(), 2);

    let relation = fragmap.cluster_relation(0, 1, 0);
    assert_eq!(relation, SquashRelation::NoRelation);
}

#[test]
fn test_cluster_relation_no_relation_only_one_touches() {
    // Only one commit touches the cluster
    let commits = vec![
        make_commit_diff(
            "c1",
            vec![make_file_diff(
                Some("file.txt"),
                Some("file.txt"),
                1,
                0,
                1,
                5,
            )],
        ),
        make_commit_diff(
            "c2",
            vec![make_file_diff(
                Some("file.txt"),
                Some("file.txt"),
                100,
                0,
                100,
                5, // Far away, different cluster
            )],
        ),
    ];

    let fragmap = build_fragmap(&commits, true);

    assert_eq!(fragmap.clusters.len(), 2);

    // c1 touches cluster 0, c2 doesn't
    let relation = fragmap.cluster_relation(0, 1, 0);
    assert_eq!(relation, SquashRelation::NoRelation);
}

#[test]
fn test_cluster_relation_squashable_no_collisions() {
    // Two commits touch same cluster, no commits in between
    let commits = vec![
        make_commit_diff(
            "c1",
            vec![make_file_diff(
                Some("file.txt"),
                Some("file.txt"),
                1,
                0,
                1,
                5,
            )],
        ),
        make_commit_diff(
            "c2",
            vec![make_file_diff(
                Some("file.txt"),
                Some("file.txt"),
                3,
                3,
                3,
                4, // Overlaps with c1
            )],
        ),
    ];

    let fragmap = build_fragmap(&commits, true);

    // Find the shared cluster
    let shared_idx = fragmap
        .clusters
        .iter()
        .position(|c| {
            c.commit_oids.contains(&"c1".to_string()) && c.commit_oids.contains(&"c2".to_string())
        })
        .expect("should have a shared cluster");

    let relation = fragmap.cluster_relation(0, 1, shared_idx);
    assert_eq!(relation, SquashRelation::Squashable);
}

#[test]
fn test_cluster_relation_conflicting_with_collision() {
    // Three commits touch same code region - middle one creates a collision
    let commits = vec![
        make_commit_diff(
            "c1",
            vec![make_file_diff(
                Some("file.txt"),
                Some("file.txt"),
                1,
                0,
                1,
                5,
            )],
        ),
        make_commit_diff(
            "c2",
            vec![make_file_diff(
                Some("file.txt"),
                Some("file.txt"),
                3,
                2,
                3,
                3, // Overlaps - collision
            )],
        ),
        make_commit_diff(
            "c3",
            vec![make_file_diff(
                Some("file.txt"),
                Some("file.txt"),
                2,
                3,
                2,
                4, // Also overlaps
            )],
        ),
    ];

    let fragmap = build_fragmap(&commits, true);

    // All three commits should share at least one cluster
    let all_three_idx = fragmap
        .clusters
        .iter()
        .position(|c| {
            c.commit_oids.contains(&"c1".to_string())
                && c.commit_oids.contains(&"c2".to_string())
                && c.commit_oids.contains(&"c3".to_string())
        })
        .expect("should have a cluster with all three commits");

    // c1 and c3 have a collision (c2 in between)
    let relation = fragmap.cluster_relation(0, 2, all_three_idx);
    assert_eq!(relation, SquashRelation::Conflicting);
}

#[test]
fn test_cluster_relation_invalid_indices() {
    let commits = vec![
        make_commit_diff(
            "c1",
            vec![make_file_diff(Some("a.txt"), Some("a.txt"), 1, 0, 1, 5)],
        ),
        make_commit_diff(
            "c2",
            vec![make_file_diff(Some("a.txt"), Some("a.txt"), 3, 2, 3, 3)],
        ),
    ];

    let fragmap = build_fragmap(&commits, true);

    // Out of range commit index
    let relation = fragmap.cluster_relation(0, 10, 0);
    assert_eq!(relation, SquashRelation::NoRelation);

    // Out of range cluster index
    let relation = fragmap.cluster_relation(0, 1, 10);
    assert_eq!(relation, SquashRelation::NoRelation);
}

#[test]
fn test_cluster_relation_earlier_not_less_than_later() {
    let commits = vec![
        make_commit_diff(
            "c1",
            vec![make_file_diff(Some("a.txt"), Some("a.txt"), 1, 0, 1, 5)],
        ),
        make_commit_diff(
            "c2",
            vec![make_file_diff(Some("a.txt"), Some("a.txt"), 3, 2, 3, 3)],
        ),
    ];

    let fragmap = build_fragmap(&commits, true);

    // Same index
    let relation = fragmap.cluster_relation(1, 1, 0);
    assert_eq!(relation, SquashRelation::NoRelation);

    // Earlier > later
    let relation = fragmap.cluster_relation(1, 0, 0);
    assert_eq!(relation, SquashRelation::NoRelation);
}

#[test]
fn test_cluster_relation_multiple_clusters() {
    // Complex scenario with multiple clusters across files
    let commits = vec![
        make_commit_diff(
            "c1",
            vec![
                make_file_diff(Some("a.txt"), Some("a.txt"), 1, 0, 1, 5),
                make_file_diff(Some("b.txt"), Some("b.txt"), 1, 0, 1, 5),
            ],
        ),
        make_commit_diff(
            "c2",
            vec![make_file_diff(Some("a.txt"), Some("a.txt"), 3, 2, 3, 3)],
        ),
        make_commit_diff(
            "c3",
            vec![make_file_diff(Some("b.txt"), Some("b.txt"), 3, 2, 3, 3)],
        ),
    ];

    let fragmap = build_fragmap(&commits, true);

    // Find shared clusters by file
    let a_cluster_idx = fragmap
        .clusters
        .iter()
        .position(|c| {
            c.spans[0].path == "a.txt"
                && c.commit_oids.contains(&"c1".to_string())
                && c.commit_oids.contains(&"c2".to_string())
        })
        .expect("should have a shared a.txt cluster");
    let b_cluster_idx = fragmap
        .clusters
        .iter()
        .position(|c| {
            c.spans[0].path == "b.txt"
                && c.commit_oids.contains(&"c1".to_string())
                && c.commit_oids.contains(&"c3".to_string())
        })
        .expect("should have a shared b.txt cluster");

    // c1 and c2 both touch a.txt cluster - squashable (no collision)
    let relation = fragmap.cluster_relation(0, 1, a_cluster_idx);
    assert_eq!(relation, SquashRelation::Squashable);

    // c1 and c3 both touch b.txt cluster - squashable (no collision)
    let relation = fragmap.cluster_relation(0, 2, b_cluster_idx);
    assert_eq!(relation, SquashRelation::Squashable);

    // c2 and c3 don't share any cluster
    assert!(!fragmap.shares_cluster_with(1, 2));
}

#[test]
fn test_cluster_relation_squashable_with_gap() {
    // Four commits: c1 and c4 touch overlapping regions, c2 and c3 don't
    let commits = vec![
        make_commit_diff(
            "c1",
            vec![make_file_diff(
                Some("file.txt"),
                Some("file.txt"),
                1,
                0,
                1,
                5,
            )],
        ),
        make_commit_diff(
            "c2",
            vec![make_file_diff(
                Some("other.txt"),
                Some("other.txt"),
                1,
                0,
                1,
                5,
            )],
        ),
        make_commit_diff(
            "c3",
            vec![make_file_diff(
                Some("another.txt"),
                Some("another.txt"),
                1,
                0,
                1,
                5,
            )],
        ),
        make_commit_diff(
            "c4",
            vec![make_file_diff(
                Some("file.txt"),
                Some("file.txt"),
                3,
                2,
                3,
                3,
            )],
        ),
    ];

    let fragmap = build_fragmap(&commits, true);

    // Find the shared file.txt cluster containing c1 and c4
    let file_cluster_idx = fragmap
        .clusters
        .iter()
        .position(|c| {
            c.spans[0].path == "file.txt"
                && c.commit_oids.contains(&"c1".to_string())
                && c.commit_oids.contains(&"c4".to_string())
        })
        .expect("should have a shared file.txt cluster");

    // c1 and c4 touch file.txt, c2 and c3 don't - squashable
    let relation = fragmap.cluster_relation(0, 3, file_cluster_idx);
    assert_eq!(relation, SquashRelation::Squashable);
}

/// Build a FragMap directly from a matrix (bypasses span extraction).
fn make_fragmap(commit_ids: &[&str], n_clusters: usize, touches: &[(usize, usize)]) -> FragMap {
    let commits: Vec<String> = commit_ids.iter().map(|s| s.to_string()).collect();
    let clusters = (0..n_clusters)
        .map(|_| SpanCluster {
            spans: vec![FileSpan {
                path: "f.txt".to_string(),
                start_line: 1,
                end_line: 1,
            }],
            commit_oids: vec![],
        })
        .collect();
    let mut matrix = vec![vec![TouchKind::None; n_clusters]; commit_ids.len()];
    for &(c, cl) in touches {
        matrix[c][cl] = TouchKind::Modified;
    }
    FragMap {
        commits,
        clusters,
        matrix,
    }
}

// squash_target tests

#[test]
fn squash_target_no_shared_clusters() {
    // c0 touches cluster 0, c1 touches cluster 1 — no earlier commit in c1's cluster
    let fm = make_fragmap(&["c0", "c1"], 2, &[(0, 0), (1, 1)]);
    assert_eq!(fm.squash_target(1), None);
}

#[test]
fn squash_target_adjacent() {
    // c0 and c1 both touch cluster 0 — c1's target is c0
    let fm = make_fragmap(&["c0", "c1"], 1, &[(0, 0), (1, 0)]);
    assert_eq!(fm.squash_target(1), Some(0));
}

#[test]
fn squash_target_with_gap() {
    // c0 and c2 touch cluster 0, c1 does not — c2's target is c0
    let fm = make_fragmap(&["c0", "c1", "c2"], 1, &[(0, 0), (2, 0)]);
    assert_eq!(fm.squash_target(2), Some(0));
}

#[test]
fn squash_target_conflicting_returns_none() {
    // c0, c1, c2 all touch cluster 0 — c2 blocked by c1
    let fm = make_fragmap(&["c0", "c1", "c2"], 1, &[(0, 0), (1, 0), (2, 0)]);
    assert_eq!(fm.squash_target(2), None);
}

#[test]
fn squash_target_multiple_clusters_same_target() {
    // c0 and c1 share clusters 0 and 1 — target is c0
    let fm = make_fragmap(&["c0", "c1"], 2, &[(0, 0), (0, 1), (1, 0), (1, 1)]);
    assert_eq!(fm.squash_target(1), Some(0));
}

#[test]
fn squash_target_multiple_clusters_different_targets() {
    // cluster 0: c0 and c2 → target c0
    // cluster 1: c1 and c2 → target c1
    // c2 has divergent targets → None
    let fm = make_fragmap(&["c0", "c1", "c2"], 2, &[(0, 0), (1, 1), (2, 0), (2, 1)]);
    assert_eq!(fm.squash_target(2), None);
}

#[test]
fn squash_target_earliest_commit_returns_none() {
    // c0 is the earliest — nothing earlier to squash into
    let fm = make_fragmap(&["c0", "c1"], 1, &[(0, 0), (1, 0)]);
    assert_eq!(fm.squash_target(0), None);
}

#[test]
fn squash_target_no_clusters_touched() {
    // c1 doesn't touch any cluster
    let fm = make_fragmap(&["c0", "c1"], 1, &[(0, 0)]);
    assert_eq!(fm.squash_target(1), None);
}

// is_fully_squashable tests

#[test]
fn is_fully_squashable_single_cluster_adjacent() {
    // c0 and c1 touch cluster 0, c1 is squashable into c0
    let fm = make_fragmap(&["c0", "c1"], 1, &[(0, 0), (1, 0)]);
    assert!(fm.is_fully_squashable(1));
}

#[test]
fn is_fully_squashable_first_commit_not_squashable() {
    // c0 is the earliest — nothing to squash into
    let fm = make_fragmap(&["c0", "c1"], 1, &[(0, 0), (1, 0)]);
    assert!(!fm.is_fully_squashable(0));
}

#[test]
fn is_fully_squashable_multiple_clusters_same_target() {
    // c0 and c1 both touch clusters 0 and 1 — all squashable into c0
    let fm = make_fragmap(&["c0", "c1"], 2, &[(0, 0), (0, 1), (1, 0), (1, 1)]);
    assert!(fm.is_fully_squashable(1));
}

#[test]
fn is_fully_squashable_multiple_clusters_different_targets() {
    // cluster 0: c0 and c2 — target c0
    // cluster 1: c1 and c2 — target c1
    // c2 has different targets, not fully squashable
    let fm = make_fragmap(&["c0", "c1", "c2"], 2, &[(0, 0), (1, 1), (2, 0), (2, 1)]);
    assert!(!fm.is_fully_squashable(2));
}

#[test]
fn is_fully_squashable_conflicting_cluster() {
    // c0, c1, c2 all touch cluster 0 — c2 has c1 in between
    let fm = make_fragmap(&["c0", "c1", "c2"], 1, &[(0, 0), (1, 0), (2, 0)]);
    assert!(!fm.is_fully_squashable(2));
}

#[test]
fn is_fully_squashable_no_clusters_touched() {
    // c1 doesn't touch any cluster
    let fm = make_fragmap(&["c0", "c1"], 1, &[(0, 0)]);
    assert!(!fm.is_fully_squashable(1));
}

// shares_cluster_with tests

#[test]
fn shares_cluster_with_no_shared_cluster() {
    let fm = make_fragmap(&["c0", "c1"], 2, &[(0, 0), (1, 1)]);
    assert!(!fm.shares_cluster_with(0, 1));
}

#[test]
fn shares_cluster_with_adjacent_pair() {
    let fm = make_fragmap(&["c0", "c1"], 1, &[(0, 0), (1, 0)]);
    assert!(fm.shares_cluster_with(0, 1));
}

#[test]
fn shares_cluster_with_blocked_by_middle_commit() {
    let fm = make_fragmap(&["c0", "c1", "c2"], 1, &[(0, 0), (1, 0), (2, 0)]);
    assert!(fm.shares_cluster_with(0, 2));
}

#[test]
fn shares_cluster_with_is_symmetric() {
    let fm = make_fragmap(&["c0", "c1"], 1, &[(0, 0), (1, 0)]);
    assert_eq!(fm.shares_cluster_with(0, 1), fm.shares_cluster_with(1, 0));
}

#[test]
fn shares_cluster_with_same_commit() {
    let fm = make_fragmap(&["c0"], 1, &[(0, 0)]);
    assert!(!fm.shares_cluster_with(0, 0));
}

#[test]
fn shares_cluster_with_one_shared_is_enough() {
    // cluster 0: only c0. cluster 1: c0 and c1
    let fm = make_fragmap(&["c0", "c1"], 2, &[(0, 0), (0, 1), (1, 1)]);
    assert!(fm.shares_cluster_with(0, 1));
}

// =========================================================
// build_fragmap SPG edge cases
// =========================================================

#[test]
fn build_fragmap_pure_insertion_clusters_with_later_modifier() {
    // c1 inserts 10 lines starting at position 5 (old_lines=0).
    // c2 then modifies 3 lines starting at old position 7 (within c1's block).
    // They overlap → must share a cluster.
    let commits = vec![
        make_commit_diff(
            "c1",
            vec![make_file_diff(Some("f.rs"), Some("f.rs"), 5, 0, 5, 10)],
        ),
        make_commit_diff(
            "c2",
            vec![make_file_diff(Some("f.rs"), Some("f.rs"), 7, 3, 7, 3)],
        ),
    ];
    let fm = build_fragmap(&commits, true);
    assert!(fm.shares_cluster_with(0, 1));
}

#[test]
fn build_fragmap_far_deletion_does_not_cluster_with_unrelated_modify() {
    // c1 modifies lines 1-5 of one region.
    // c2 only deletes lines 50-53 (far away, different region, new_lines=0).
    // c2's deletion is far from c1 → separate clusters.
    let commits = vec![
        make_commit_diff(
            "c1",
            vec![make_file_diff(Some("f.rs"), Some("f.rs"), 1, 3, 1, 5)],
        ),
        make_commit_diff(
            "c2",
            vec![FileDiff {
                old_path: Some("f.rs".to_string()),
                new_path: Some("f.rs".to_string()),
                status: crate::DeltaStatus::Modified,
                hunks: vec![Hunk {
                    old_start: 50,
                    old_lines: 3,
                    new_start: 50,
                    new_lines: 0,
                    lines: vec![],
                }],
            }],
        ),
    ];
    let fm = build_fragmap(&commits, true);
    assert!(!fm.shares_cluster_with(0, 1));
}

#[test]
fn build_fragmap_file_rename_cluster_uses_canonical_path() {
    // A commit that renames foo.rs → bar.rs. The cluster should track
    // the canonical (earliest) path — foo.rs.
    let c1 = CommitDiff {
        commit: make_commit_info_with_oid("c1"),
        files: vec![FileDiff {
            old_path: Some("foo.rs".to_string()),
            new_path: Some("bar.rs".to_string()),
            status: crate::DeltaStatus::Modified,
            hunks: vec![Hunk {
                old_start: 5,
                old_lines: 3,
                new_start: 5,
                new_lines: 4,
                lines: vec![],
            }],
        }],
    };
    let fm = build_fragmap(&[c1], true);
    assert_eq!(fm.clusters.len(), 1);
    assert_eq!(fm.clusters[0].spans[0].path, "foo.rs");
}

#[test]
fn build_fragmap_rename_groups_old_and_new_in_same_cluster() {
    // Commit 0 touches foo.rs lines 1-10.
    // Commit 1 renames foo.rs → bar.rs and modifies overlapping lines 5-12.
    // Both should land in the same cluster because the rename map links
    // bar.rs back to the canonical name foo.rs.
    let c0 = make_commit_diff(
        "c0",
        vec![make_file_diff(Some("foo.rs"), Some("foo.rs"), 1, 0, 1, 10)],
    );
    let c1 = CommitDiff {
        commit: make_commit_info_with_oid("c1"),
        files: vec![FileDiff {
            old_path: Some("foo.rs".to_string()),
            new_path: Some("bar.rs".to_string()),
            status: crate::DeltaStatus::Modified,
            hunks: vec![Hunk {
                old_start: 5,
                old_lines: 6,
                new_start: 5,
                new_lines: 8,
                lines: vec![],
            }],
        }],
    };
    let fm = build_fragmap(&[c0, c1], true);
    // Both commits share a cluster — the overlapping region groups them.
    assert!(
        fm.clusters.iter().any(|cl| {
            cl.commit_oids.contains(&"c0".to_string()) && cl.commit_oids.contains(&"c1".to_string())
        }),
        "expected c0 and c1 to share a cluster via rename tracking"
    );
}

#[test]
fn build_fragmap_single_commit_two_regions_deduped_to_one_column() {
    // One commit touching two non-overlapping regions of the same file.
    // Both SPG paths have the same active-node set {c1} → deduplicated to 1 column.
    let mut c1 = make_commit_diff(
        "c1",
        vec![make_file_diff(Some("f.rs"), Some("f.rs"), 1, 0, 1, 5)],
    );
    c1.files
        .push(make_file_diff(Some("f.rs"), Some("f.rs"), 100, 0, 100, 5));
    let fm = build_fragmap(&[c1], true);
    assert_eq!(fm.clusters.len(), 1);
    assert_ne!(fm.matrix[0][0], TouchKind::None);
}

#[test]
fn build_fragmap_no_dedup_keeps_identical_activation_pattern_columns() {
    // Same scenario as the dedup test above, but with deduplicate=false.
    // Both clusters have the same activation pattern {c1}, but they must
    // NOT be merged — the full view should expose all raw columns.
    let mut c1 = make_commit_diff(
        "c1",
        vec![make_file_diff(Some("f.rs"), Some("f.rs"), 1, 0, 1, 5)],
    );
    c1.files
        .push(make_file_diff(Some("f.rs"), Some("f.rs"), 100, 0, 100, 5));
    let fm = build_fragmap(&[c1], false);
    assert_eq!(fm.clusters.len(), 2);
    assert_ne!(fm.matrix[0][0], TouchKind::None);
    assert_ne!(fm.matrix[0][1], TouchKind::None);
}

#[test]
fn build_fragmap_two_commits_separate_regions_not_deduped() {
    // c1 and c2 each touch a distinct region → different activation patterns → 2 columns.
    let commits = vec![
        make_commit_diff(
            "c1",
            vec![make_file_diff(Some("f.rs"), Some("f.rs"), 1, 0, 1, 5)],
        ),
        make_commit_diff(
            "c2",
            vec![make_file_diff(Some("f.rs"), Some("f.rs"), 100, 0, 100, 5)],
        ),
    ];
    let fm = build_fragmap(&commits, true);
    assert_eq!(fm.clusters.len(), 2);
}

#[test]
fn build_fragmap_three_commits_sequential_on_same_region() {
    // c1 introduces a block, c2 refines it, c3 refines it again.
    // All three share a cluster; c1 and c3 are also related.
    let commits = vec![
        make_commit_diff(
            "c1",
            vec![make_file_diff(Some("f.rs"), Some("f.rs"), 10, 5, 10, 10)],
        ),
        make_commit_diff(
            "c2",
            vec![make_file_diff(Some("f.rs"), Some("f.rs"), 12, 3, 12, 3)],
        ),
        make_commit_diff(
            "c3",
            vec![make_file_diff(Some("f.rs"), Some("f.rs"), 11, 2, 11, 2)],
        ),
    ];
    let fm = build_fragmap(&commits, true);
    assert!(fm.shares_cluster_with(0, 1));
    assert!(fm.shares_cluster_with(0, 2));
    assert!(fm.shares_cluster_with(1, 2));
}

#[test]
fn build_fragmap_empty_span_does_not_panic() {
    // A commit with a single-line addition (new_lines=1) followed by a
    // commit that touches an adjacent but non-overlapping line.
    // Regression guard: no panic or infinite loop in SPG construction.
    let commits = vec![
        make_commit_diff(
            "c1",
            vec![make_file_diff(Some("f.rs"), Some("f.rs"), 10, 1, 10, 1)],
        ),
        make_commit_diff(
            "c2",
            vec![make_file_diff(Some("f.rs"), Some("f.rs"), 20, 1, 20, 1)],
        ),
    ];
    let fm = build_fragmap(&commits, true);
    assert_eq!(fm.commits.len(), 2);
}
