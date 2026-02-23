// Fragmap: chunk clustering for visualizing commit relationships

use crate::CommitDiff;

/// A span of line numbers within a specific file.
///
/// Represents a contiguous range of lines that were touched by a commit.
/// This is extracted from a hunk's position in the NEW (post-commit) version
/// of the file.
///
/// FileSpans are the building blocks of the fragmap visualization: they get
/// clustered across commits to show which commits touch overlapping regions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSpan {
    /// The file path (from the new version of the file).
    pub path: String,
    /// First line number (1-indexed) in the range.
    pub start_line: u32,
    /// Last line number (1-indexed) in the range, inclusive.
    pub end_line: u32,
}

/// Extract all FileSpans from a commit's diff.
///
/// Converts each hunk in the commit into a FileSpan representing the line
/// range in the NEW (post-commit) version of the file. Deleted files and
/// empty hunks are skipped.
///
/// The resulting spans are used for clustering in the fragmap matrix to
/// identify which commits touch overlapping code regions.
pub fn extract_spans(commit_diff: &CommitDiff) -> Vec<FileSpan> {
    let mut spans = Vec::new();

    for file in &commit_diff.files {
        // Skip deleted files (no new path means the file was removed)
        let path = match &file.new_path {
            Some(p) => p.clone(),
            None => continue,
        };

        for hunk in &file.hunks {
            // Skip hunks with no new lines (pure deletions in context)
            if hunk.new_lines == 0 {
                continue;
            }

            // Create span from hunk's position in the new file
            spans.push(FileSpan {
                path: path.clone(),
                start_line: hunk.new_start,
                end_line: hunk.new_start + hunk.new_lines - 1,
            });
        }
    }

    spans
}

/// The kind of change a commit makes to a code region.
///
/// Used in the fragmap matrix to show how each commit touches each cluster.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchKind {
    /// The commit added new lines in this region (file was added or lines inserted).
    Added,
    /// The commit modified existing lines in this region.
    Modified,
    /// The commit deleted lines in this region.
    Deleted,
    /// The commit did not touch this region.
    None,
}

/// A cluster of overlapping or adjacent FileSpans across multiple commits.
///
/// Represents a code region that multiple commits touch. Spans are merged
/// when they overlap or are adjacent (within same file).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpanCluster {
    /// The merged spans in this cluster (typically one span per file touched).
    pub spans: Vec<FileSpan>,
    /// OIDs of commits that touch this cluster.
    pub commit_oids: Vec<String>,
}

/// The complete fragmap: commits, span clusters, and the matrix showing
/// which commits touch which clusters.
///
/// The matrix is commits × clusters, where matrix[commit_idx][cluster_idx]
/// indicates how that commit touches that cluster.
#[derive(Debug, Clone)]
pub struct FragMap {
    /// The commits in order (oldest to newest).
    pub commits: Vec<String>,
    /// The span clusters (code regions touched by commits).
    pub clusters: Vec<SpanCluster>,
    /// Matrix[commit_idx][cluster_idx] = TouchKind
    pub matrix: Vec<Vec<TouchKind>>,
}

/// Build a fragmap from a collection of commits and their diffs.
///
/// Extracts spans from each commit, clusters overlapping/adjacent spans,
/// and builds the commits × clusters matrix showing which commits touch
/// which code regions and how (Added/Modified/Deleted).
pub fn build_fragmap(commit_diffs: &[CommitDiff]) -> FragMap {
    // Extract spans for each commit
    let commit_spans: Vec<(String, Vec<FileSpan>)> = commit_diffs
        .iter()
        .map(|diff| (diff.commit.oid.clone(), extract_spans(diff)))
        .collect();

    // Build clusters by merging overlapping/adjacent spans
    let clusters = cluster_spans(&commit_spans);

    // Build the matrix
    let commits: Vec<String> = commit_diffs.iter().map(|d| d.commit.oid.clone()).collect();
    let matrix = build_matrix(&commits, &clusters, commit_diffs);

    FragMap {
        commits,
        clusters,
        matrix,
    }
}

/// Cluster FileSpans that overlap or are adjacent in the same file.
///
/// Two spans cluster if they're in the same file and either overlap or
/// are adjacent (end_line + 1 == start_line).
fn cluster_spans(commit_spans: &[(String, Vec<FileSpan>)]) -> Vec<SpanCluster> {
    let mut clusters: Vec<SpanCluster> = Vec::new();

    for (commit_oid, spans) in commit_spans {
        for span in spans {
            // Find if this span belongs to an existing cluster
            let mut found = false;
            for cluster in &mut clusters {
                if cluster_contains_or_adjacent(cluster, span) {
                    // Add this span to the cluster
                    merge_span_into_cluster(cluster, span, commit_oid);
                    found = true;
                    break;
                }
            }

            // If no existing cluster, create a new one
            if !found {
                clusters.push(SpanCluster {
                    spans: vec![span.clone()],
                    commit_oids: vec![commit_oid.clone()],
                });
            }
        }
    }

    clusters
}

/// Check if a span overlaps or is adjacent to any span in the cluster.
fn cluster_contains_or_adjacent(cluster: &SpanCluster, span: &FileSpan) -> bool {
    cluster.spans.iter().any(|cluster_span| {
        if cluster_span.path != span.path {
            return false;
        }

        // Check overlap or adjacency
        let overlaps =
            !(span.end_line < cluster_span.start_line || span.start_line > cluster_span.end_line);
        let adjacent = span.end_line + 1 == cluster_span.start_line
            || cluster_span.end_line + 1 == span.start_line;

        overlaps || adjacent
    })
}

/// Merge a span into a cluster, extending the cluster's range and adding the commit.
fn merge_span_into_cluster(cluster: &mut SpanCluster, span: &FileSpan, commit_oid: &str) {
    // Find the span for this file in the cluster, or add it
    if let Some(cluster_span) = cluster.spans.iter_mut().find(|s| s.path == span.path) {
        // Extend the range
        cluster_span.start_line = cluster_span.start_line.min(span.start_line);
        cluster_span.end_line = cluster_span.end_line.max(span.end_line);
    } else {
        // New file in this cluster
        cluster.spans.push(span.clone());
    }

    // Add commit OID if not already present
    if !cluster.commit_oids.contains(&commit_oid.to_string()) {
        cluster.commit_oids.push(commit_oid.to_string());
    }
}

/// Build the commits × clusters matrix with TouchKind values.
///
/// For each (commit, cluster), determine if the commit touches the cluster
/// and if so, classify the touch as Added/Modified/Deleted.
fn build_matrix(
    commits: &[String],
    clusters: &[SpanCluster],
    commit_diffs: &[CommitDiff],
) -> Vec<Vec<TouchKind>> {
    let mut matrix = vec![vec![TouchKind::None; clusters.len()]; commits.len()];

    for (commit_idx, commit_oid) in commits.iter().enumerate() {
        let commit_diff = &commit_diffs[commit_idx];

        for (cluster_idx, cluster) in clusters.iter().enumerate() {
            // Check if this commit touches this cluster
            if cluster.commit_oids.contains(commit_oid) {
                // Determine the touch kind
                matrix[commit_idx][cluster_idx] = determine_touch_kind(commit_diff, cluster);
            }
        }
    }

    matrix
}

/// Determine how a commit touches a cluster (Added/Modified/Deleted).
///
/// Looks at the files in the commit that overlap with the cluster's spans
/// to classify the type of change.
fn determine_touch_kind(commit_diff: &CommitDiff, cluster: &SpanCluster) -> TouchKind {
    for cluster_span in &cluster.spans {
        for file in &commit_diff.files {
            // Check if this file matches the cluster span
            let file_path = file.new_path.as_ref().or(file.old_path.as_ref());
            if file_path.map(|p| p == &cluster_span.path).unwrap_or(false) {
                // Classify based on file paths
                if file.old_path.is_none() && file.new_path.is_some() {
                    return TouchKind::Added;
                } else if file.old_path.is_some() && file.new_path.is_none() {
                    return TouchKind::Deleted;
                } else {
                    return TouchKind::Modified;
                }
            }
        }
    }

    TouchKind::None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CommitDiff, CommitInfo, FileDiff, Hunk};

    fn make_commit_info() -> CommitInfo {
        CommitInfo {
            oid: "abc123".to_string(),
            summary: "Test commit".to_string(),
            author: "Test Author".to_string(),
            date: "123456789".to_string(),
            parent_oids: vec![],
        }
    }

    #[test]
    fn test_extract_spans_single_hunk() {
        let commit_diff = CommitDiff {
            commit: make_commit_info(),
            files: vec![FileDiff {
                old_path: Some("file.txt".to_string()),
                new_path: Some("file.txt".to_string()),
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
}
