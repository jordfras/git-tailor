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

// Core library for git-tailor

pub mod app;
pub mod event;
pub mod fragmap;
pub mod repo;
pub mod views;

/// Represents commit metadata extracted from git repository.
///
/// This is a pure data structure containing commit information
/// without any git2 object dependencies.
#[derive(Debug, Clone)]
pub struct CommitInfo {
    pub oid: String,
    pub summary: String,
    pub author: String,
    pub date: String,
    pub parent_oids: Vec<String>,
    /// Full commit message including body (all lines).
    pub message: String,
    /// Author email address.
    pub author_email: String,
    /// Author date with timezone.
    pub author_date: time::OffsetDateTime,
    /// Committer name.
    pub committer: String,
    /// Committer email address.
    pub committer_email: String,
    /// Commit date with timezone.
    pub commit_date: time::OffsetDateTime,
}

/// The kind of change a diff line represents.
///
/// When Git compares two versions of a file, each line in the output falls
/// into one of three categories:
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLineKind {
    /// An unchanged line shown for surrounding context. These lines exist
    /// identically in both the old and new versions of the file.
    Context,
    /// A line that was added in the new version. It does not exist in the
    /// old version. Shown with a "+" prefix in traditional diff output.
    Addition,
    /// A line that was removed from the old version. It does not exist in
    /// the new version. Shown with a "-" prefix in traditional diff output.
    Deletion,
}

/// A single line from a diff, along with what kind of change it represents.
#[derive(Debug, Clone)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    /// The text content of the line (without the +/- prefix).
    pub content: String,
}

/// A "hunk" is a standard Git concept: a contiguous group of changed lines
/// within a single file, together with a few surrounding unchanged (context)
/// lines for orientation.
///
/// When a file has changes in multiple separate regions, Git produces one
/// hunk per region rather than one giant diff. For example, if lines 10-12
/// and lines 50-55 were modified, that file's diff would contain two hunks.
///
/// The line numbers refer to positions in the old (before) and new (after)
/// versions of the file:
/// - `old_start` / `old_lines`: where this hunk begins and how many lines
///   it spans in the original file.
/// - `new_start` / `new_lines`: the same for the modified file.
///
/// These correspond to the `@@ -old_start,old_lines +new_start,new_lines @@`
/// header you see in unified diff output.
#[derive(Debug, Clone)]
pub struct Hunk {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    /// The individual lines in this hunk (context, additions, and deletions).
    pub lines: Vec<DiffLine>,
}

/// Git delta status indicating the type of file change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeltaStatus {
    Unmodified,
    Added,
    Deleted,
    Modified,
    Renamed,
    Copied,
    Ignored,
    Untracked,
    Typechange,
    Unreadable,
    Conflicted,
}

/// The diff for a single file between a commit and its parent commit.
///
/// Represents all changes made to one file in a single commit. A file may
/// have been added (old_path is None), deleted (new_path is None), renamed
/// (both paths differ), or modified (both paths are the same).
#[derive(Debug, Clone)]
pub struct FileDiff {
    /// Path in the old (parent) version, or None if the file was newly added.
    pub old_path: Option<String>,
    /// Path in the new (commit) version, or None if the file was deleted.
    pub new_path: Option<String>,
    /// The git delta status indicating the type of change.
    pub status: DeltaStatus,
    /// The list of changed regions in this file. A simple one-line change
    /// produces one hunk; scattered edits produce multiple hunks.
    pub hunks: Vec<Hunk>,
}

/// All diff information for a single commit.
///
/// Combines the commit metadata with the complete list of file changes,
/// giving a full picture of what a commit modified.
#[derive(Debug, Clone)]
pub struct CommitDiff {
    pub commit: CommitInfo,
    /// Every file that was added, modified, renamed, or deleted in this commit.
    pub files: Vec<FileDiff>,
}