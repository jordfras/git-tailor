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
