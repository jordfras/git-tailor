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

pub mod git2_impl;

pub use git2_impl::Git2Repo;

use anyhow::Result;

use crate::{CommitDiff, CommitInfo};

/// Abstraction over git repository operations.
///
/// Isolates the `git2` crate to the `repo::git2_impl` module. Callers work
/// through this trait so that the real `Git2Repo` implementation can be
/// swapped with a mock or fake in tests.
pub trait GitRepo {
    /// Returns the OID that HEAD currently points at.
    ///
    /// Fails if HEAD is detached or does not resolve to a direct commit
    /// reference.
    fn head_oid(&self) -> Result<String>;

    /// Find the merge-base (reference point) between HEAD and a given commit-ish.
    ///
    /// The commit-ish can be:
    /// - A branch name (e.g., "main", "feature")
    /// - A tag name (e.g., "v1.0")
    /// - A commit hash (short or long)
    ///
    /// Returns the OID of the common ancestor as a string.
    fn find_reference_point(&self, commit_ish: &str) -> Result<String>;

    /// List commits from one commit back to another (inclusive).
    ///
    /// Walks the commit graph from `from_oid` back to `to_oid`, collecting
    /// commit metadata. Returns commits in oldest-to-newest order.
    ///
    /// Both `from_oid` and `to_oid` can be any commit-ish (branch, tag, hash).
    /// The range includes both endpoints.
    fn list_commits(&self, from_oid: &str, to_oid: &str) -> Result<Vec<CommitInfo>>;

    /// Extract the full diff for a single commit compared to its first parent.
    ///
    /// For the root commit (no parents), diffs against an empty tree so all
    /// files show as additions. Returns a `CommitDiff` containing the commit
    /// metadata and every file/hunk/line changed.
    fn commit_diff(&self, oid: &str) -> Result<CommitDiff>;

    /// Extract commit diff with zero context lines, suitable for fragmap analysis.
    ///
    /// The fragmap algorithm needs each logical change as its own hunk. With
    /// the default 3-line context, git merges adjacent hunks together which
    /// produces fewer but larger hunks — breaking the SPG's fine-grained
    /// span tracking.
    fn commit_diff_for_fragmap(&self, oid: &str) -> Result<CommitDiff>;

    /// Return a synthetic `CommitDiff` for changes staged in the index (index vs HEAD).
    ///
    /// Returns `None` when the index is clean (no staged changes).
    fn staged_diff(&self) -> Option<CommitDiff>;

    /// Return a synthetic `CommitDiff` for unstaged working-tree changes (workdir vs index).
    ///
    /// Returns `None` when the working tree is clean relative to the index.
    fn unstaged_diff(&self) -> Option<CommitDiff>;
}
