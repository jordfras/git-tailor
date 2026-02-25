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
