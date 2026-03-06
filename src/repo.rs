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

/// Result of a rebase operation that may encounter merge conflicts.
#[derive(Debug)]
pub enum RebaseOutcome {
    /// The rebase completed without conflicts.
    Complete,
    /// A cherry-pick step produced a merge conflict. The conflicted state has
    /// been written to the working tree and index so the user can resolve it.
    Conflict(ConflictState),
}

/// Enough state to resume or abort a conflicted rebase.
///
/// When a cherry-pick produces conflicts during a rebase, the partially
/// merged index is written to the working tree. The user resolves the
/// conflicts, then calls `rebase_continue` (which reads the resolved
/// index and creates the commit) or `rebase_abort` (which restores
/// the branch to `original_branch_oid`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictState {
    /// Human-readable label for the operation that triggered this conflict
    /// (e.g. "Drop", "Squash"). Used in dialog titles and messages.
    pub operation_label: String,
    /// The branch tip OID before the operation started, used to restore on
    /// abort.
    pub original_branch_oid: String,
    /// The new tip OID built so far (all commits cherry-picked before the
    /// conflicting one).
    pub new_tip_oid: String,
    /// The OID of the commit whose cherry-pick conflicted.
    pub conflicting_commit_oid: String,
    /// OIDs of commits that still need to be cherry-picked after the
    /// conflicting commit is resolved, in order (oldest first).
    pub remaining_oids: Vec<String>,
    /// Paths of files that have conflict markers in the index (stage > 0).
    /// Collected at the point of conflict so the dialog can list them.
    pub conflicting_files: Vec<String>,
    /// True when `rebase_continue` was called but the index still had
    /// unresolved entries. The dialog uses this to show a warning to the user.
    pub still_unresolved: bool,
}

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

    /// Split a commit into one commit per changed file.
    ///
    /// Creates N new commits (one per file touched by `commit_oid`), each applying
    /// only that file's changes. Rebases all commits between `commit_oid` (exclusive)
    /// and `head_oid` (inclusive) onto the resulting commits, then fast-forwards the
    /// branch ref to the new tip.
    ///
    /// Fails if:
    /// - the commit has fewer than 2 changed files (nothing to split)
    /// - staged or unstaged changes share file paths with the commit being split
    /// - a rebase conflict occurs while rebuilding descendants
    fn split_commit_per_file(&self, commit_oid: &str, head_oid: &str) -> Result<()>;

    /// Split a commit into one commit per hunk.
    ///
    /// Creates N new commits (one per hunk across all files), in file-then-hunk-index
    /// order. Each intermediate tree is built by cumulatively applying the first k hunks
    /// of the full diff (with 0 context lines) onto the original parent tree.
    ///
    /// Fails if:
    /// - the commit has fewer than 2 hunks (nothing to split)
    /// - staged or unstaged changes share file paths with the commit being split
    /// - a rebase conflict occurs while rebuilding descendants
    fn split_commit_per_hunk(&self, commit_oid: &str, head_oid: &str) -> Result<()>;

    /// Split a commit into one commit per hunk group.
    ///
    /// Hunks are grouped using the same SPG-based fragmap algorithm shown in the
    /// hunk group matrix: two hunks from the commit end up in the same group when
    /// they share the same set of interacting commits on the branch (i.e. their
    /// fragmap columns deduplicate to the same column). This yields fewer, more
    /// cohesive commits than per-hunk splitting, and the groups match exactly what
    /// the user sees in the TUI fragmap after deduplication.
    ///
    /// Fails if:
    /// - the commit cannot be mapped to at least 2 fragmap groups (nothing to split)
    /// - staged or unstaged changes share file paths with the commit being split
    /// - a rebase conflict occurs while rebuilding descendants
    fn split_commit_per_hunk_group(
        &self,
        commit_oid: &str,
        head_oid: &str,
        reference_oid: &str,
    ) -> Result<()>;

    /// Count how many commits `split_commit_per_file` would produce for this commit.
    fn count_split_per_file(&self, commit_oid: &str) -> Result<usize>;

    /// Count how many commits `split_commit_per_hunk` would produce for this commit.
    fn count_split_per_hunk(&self, commit_oid: &str) -> Result<usize>;

    /// Count how many fragmap groups `split_commit_per_hunk_group` would produce
    /// for this commit, given the full branch context up to `head_oid` from
    /// `reference_oid`.
    fn count_split_per_hunk_group(
        &self,
        commit_oid: &str,
        head_oid: &str,
        reference_oid: &str,
    ) -> Result<usize>;

    /// Reword the message of an existing commit.
    ///
    /// Creates a new commit with the same tree and parents as `commit_oid` but
    /// with `new_message` as the commit message, then cherry-picks all commits
    /// strictly between `commit_oid` and `head_oid` (inclusive) onto the new
    /// commit, and fast-forwards the branch ref to the resulting tip.
    ///
    /// Because only the message changes the diff at every step is identical, so
    /// no conflicts can arise from staged or unstaged working-tree changes.
    fn reword_commit(&self, commit_oid: &str, new_message: &str, head_oid: &str) -> Result<()>;

    /// Read a string value from the repository's git configuration.
    ///
    /// Returns `None` when the key does not exist or is not valid UTF-8.
    fn get_config_string(&self, key: &str) -> Option<String>;

    /// Drop a commit from the branch by cherry-picking its descendants onto
    /// its parent.
    ///
    /// Returns `RebaseOutcome::Complete` when all descendants are
    /// successfully rebased, or `RebaseOutcome::Conflict` when a cherry-pick
    /// step produces merge conflicts. In the conflict case the working tree
    /// and index contain the partially merged state for the user to resolve.
    fn drop_commit(&self, commit_oid: &str, head_oid: &str) -> Result<RebaseOutcome>;

    /// Resume a conflicted rebase after the user has resolved conflicts.
    ///
    /// Reads the current index (which the user resolved), creates a commit
    /// for the conflicting cherry-pick, then continues cherry-picking the
    /// remaining descendants. Returns a new `RebaseOutcome` — the next
    /// cherry-pick may also conflict.
    fn rebase_continue(&self, state: &ConflictState) -> Result<RebaseOutcome>;

    /// Abort a conflicted rebase and restore the branch to its original state.
    ///
    /// Resets the branch ref to `state.original_branch_oid`, cleans up the
    /// working tree and index.
    fn rebase_abort(&self, state: &ConflictState) -> Result<()>;

    /// Return the path of the repository's working directory, if any.
    ///
    /// Bare repositories have no working directory and return `None`.
    fn workdir(&self) -> Option<std::path::PathBuf>;

    /// Read the raw blob content of a specific index stage for a conflicted path.
    ///
    /// Stage 1 = base (common ancestor), 2 = ours, 3 = theirs.
    /// Returns `None` when that stage entry does not exist for the path.
    fn read_index_stage(&self, path: &str, stage: i32) -> Result<Option<Vec<u8>>>;

    /// Return the list of paths that currently have conflict markers in the index
    /// (entries with stage > 0), sorted alphabetically and deduplicated.
    fn read_conflicting_files(&self) -> Vec<String>;

    /// Stage a working-tree file, clearing any conflict entries for that path.
    ///
    /// Equivalent to `git add <path>`. Reads the file from the working directory,
    /// adds it to the index at stage 0 (which removes stages 1/2/3), and writes
    /// the updated index to disk. Must be called after a merge tool resolves a
    /// conflict so that subsequent `index.has_conflicts()` checks return false.
    fn stage_file(&self, path: &str) -> Result<()>;
}
