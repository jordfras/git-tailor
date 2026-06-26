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
use serde::{Deserialize, Serialize};

use crate::{CommitDiff, CommitInfo, Oid, app::SquashMode};

/// Result of reading the crash-safety journal at startup.
///
/// Crash detection keys off an in-progress record inside the journal, not the
/// file's existence — so `None` covers both "no journal file" and "a journal
/// exists but holds no interrupted operation".
#[derive(Debug)]
pub enum JournalStatus {
    /// No interrupted operation to recover.
    None,
    /// An operation was interrupted; the persisted state can resume or abort it.
    Recovered(Box<ConflictState>),
    /// The journal was written by a newer git-tailor (`version` exceeds what
    /// this build understands). The file is left untouched.
    NewerVersion(u32),
    /// The journal could not be read or parsed; the message describes why.
    Corrupt(String),
}

/// Result of an undo or redo request.
#[derive(Debug)]
pub enum UndoOutcome {
    /// The corresponding stack was empty — nothing to undo/redo.
    Empty,
    /// The branch no longer matched the recorded tip (history was changed
    /// outside git-tailor), so the undo/redo stack was discarded.
    Stale,
    /// An operation was undone/redone; `label` names it (e.g. "Drop").
    Done { label: String },
}

/// Result of a rebase operation that may encounter merge conflicts.
#[derive(Debug)]
pub enum RebaseOutcome {
    /// The rebase completed without conflicts.
    Complete,
    /// A cherry-pick step produced a merge conflict. The conflicted state has
    /// been written to the working tree and index so the user can resolve it.
    Conflict(Box<ConflictState>),
}

/// Enough state to resume or abort a conflicted rebase.
///
/// When a cherry-pick produces conflicts during a rebase, the partially
/// merged index is written to the working tree. The user resolves the
/// conflicts, then calls `rebase_continue` (which reads the resolved
/// index and creates the commit) or `rebase_abort` (which restores
/// the branch to `original_branch_oid`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ConflictState {
    /// Human-readable label for the operation that triggered this conflict
    /// (e.g. "Drop", "Squash"). Used in dialog titles and messages.
    pub operation_label: String,
    /// The branch tip OID before the operation started, used to restore on
    /// abort.
    pub original_branch_oid: Oid,
    /// The new tip OID built so far (all commits cherry-picked before the
    /// conflicting one).
    pub new_tip_oid: Oid,
    /// The OID of the commit whose cherry-pick conflicted.
    pub conflicting_commit_oid: Oid,
    /// OIDs of commits that still need to be cherry-picked after the
    /// conflicting commit is resolved, in order (oldest first).
    pub remaining_oids: Vec<Oid>,
    /// Paths of files that have conflict markers in the index (stage > 0).
    /// Collected at the point of conflict so the dialog can list them.
    pub conflicting_files: Vec<String>,
    /// True when `rebase_continue` was called but the index still had
    /// unresolved entries. The dialog uses this to show a warning to the user.
    pub still_unresolved: bool,
    /// When this conflict was triggered by a move operation, this holds the OID
    /// of the commit being moved. The conflict view uses it to tell the user
    /// whether the moved commit itself conflicted or a successor did.
    pub moved_commit_oid: Option<Oid>,
    /// When present, the conflict arose during the initial squash tree
    /// creation (source vs target overlap). After the user resolves the
    /// conflict the TUI should open the editor and then call
    /// `squash_finalize` instead of `rebase_continue`.
    pub squash_context: Option<SquashContext>,
    /// True when the conflicting commit should become an orphan root (no
    /// parents) after resolution. Used when dropping the root commit.
    pub is_orphan_root: bool,
}

/// Extra state carried through a squash-time conflict so that the squash
/// can be finalized after the user resolves the conflicting tree.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SquashContext {
    /// OID of the target commit's parent (the base for the squash commit).
    /// `None` when the target is the repository root (squash commit is an orphan).
    pub base_oid: Option<Oid>,
    /// OID of the source commit (removed after squash).
    pub source_oid: Oid,
    /// OID of the target commit (author/committer are taken from here).
    pub target_oid: Oid,
    /// The message to use for the squash commit. For squash this is the
    /// combined (target + source) message shown in the editor; for fixup
    /// this is just the target message, used as-is without opening an editor.
    pub combined_message: String,
    /// OIDs of descendants to rebase after the squash commit is created.
    pub descendant_oids: Vec<Oid>,
    /// Whether this is a squash (editor shown) or fixup (target message kept as-is).
    pub squash_mode: SquashMode,
}

/// Enough state to drive the resolution dialog for a conflicting auto-stash
/// reapply. The stash OID and the pre-operation tip needed to continue or abort
/// live in the journal; this only carries what the dialog displays.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StashConflictState {
    /// Label of the operation whose auto-stash reapply conflicted (e.g. "Drop"),
    /// used in the dialog title.
    pub operation_label: String,
    /// Paths with conflict markers left by the reapply.
    pub conflicting_files: Vec<String>,
    /// True when the user pressed continue but conflicts still remain.
    pub still_unresolved: bool,
}

/// Result of reapplying the auto-stash after an operation.
#[derive(Debug)]
pub enum AutostashRestore {
    /// Nothing was stashed, or the stash was reapplied cleanly.
    Done,
    /// The reapply conflicted: markers are in the working tree and the stash is
    /// kept. Carries the conflicting file paths for the resolution dialog.
    Conflict { files: Vec<String> },
}

/// Result of attempting to finish a conflicting auto-stash reapply.
#[derive(Debug)]
pub enum AutostashContinue {
    /// All conflicts resolved; the stash was dropped.
    Resolved,
    /// Conflicts still remain; `files` lists them.
    StillUnresolved { files: Vec<String> },
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
    fn head_oid(&self) -> Result<Oid>;

    /// Find the merge-base (reference point) between HEAD and a given commit-ish.
    ///
    /// The commit-ish can be:
    /// - A branch name (e.g., "main", "feature")
    /// - A tag name (e.g., "v1.0")
    /// - A commit hash (short or long)
    ///
    /// Returns the OID of the common ancestor as a string.
    fn find_reference_point(&self, commit_ish: &str) -> Result<Oid>;

    /// List commits from one commit back to another (inclusive).
    ///
    /// Walks the commit graph from `from_oid` back to `to_oid`, collecting
    /// commit metadata. Returns commits in oldest-to-newest order.
    ///
    /// Both `from_oid` and `to_oid` can be any commit-ish (branch, tag, hash).
    /// The range includes both endpoints.
    fn list_commits(&self, from_oid: &Oid, to_oid: &Oid) -> Result<Vec<CommitInfo>>;

    /// Extract the full diff for a single commit compared to its first parent.
    ///
    /// For the root commit (no parents), diffs against an empty tree so all
    /// files show as additions. Returns a `CommitDiff` containing the commit
    /// metadata and every file/hunk/line changed.
    fn commit_diff(&self, oid: &Oid) -> Result<CommitDiff>;

    /// Extract commit diff with zero context lines, suitable for fragmap analysis.
    ///
    /// The fragmap algorithm needs each logical change as its own hunk. With
    /// the default 3-line context, git merges adjacent hunks together which
    /// produces fewer but larger hunks — breaking the SPG's fine-grained
    /// span tracking.
    fn commit_diff_for_fragmap(&self, oid: &Oid) -> Result<CommitDiff>;

    /// Return a synthetic `CommitDiff` for changes staged in the index (index vs HEAD).
    ///
    /// Returns `Ok(None)` when the index is clean (no staged changes).
    fn staged_diff(&self) -> Result<Option<CommitDiff>>;

    /// Return a synthetic `CommitDiff` for unstaged working-tree changes (workdir vs index).
    ///
    /// Returns `Ok(None)` when the working tree is clean relative to the index.
    fn unstaged_diff(&self) -> Result<Option<CommitDiff>>;

    /// List the paths of the files changed by `commit_oid` relative to its
    /// first parent (all files for a root commit), in diff order.
    fn list_commit_files(&self, commit_oid: &Oid) -> Result<Vec<String>>;

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
    fn split_commit_per_file(&self, commit_oid: &Oid, head_oid: &Oid) -> Result<()>;

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
    fn split_commit_per_hunk(&self, commit_oid: &Oid, head_oid: &Oid) -> Result<()>;

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
        commit_oid: &Oid,
        head_oid: &Oid,
        reference_oid: &Oid,
    ) -> Result<()>;

    /// Peel a single file's changes out of `commit_oid` into a follow-up commit.
    ///
    /// Produces exactly two commits: the first keeps every other file's changes
    /// under the original (unchanged) message; the second contains only
    /// `file_path`'s changes, with the file name appended to its summary. All
    /// commits between `commit_oid` (exclusive) and `head_oid` (inclusive) are
    /// rebased onto the result and the branch ref is fast-forwarded.
    ///
    /// Fails if:
    /// - the commit has fewer than 2 changed files (nothing to split out)
    /// - `file_path` is not changed by the commit
    /// - staged or unstaged changes share file paths with the commit being split
    /// - a rebase conflict occurs while rebuilding descendants
    fn split_commit_out_file(
        &self,
        commit_oid: &Oid,
        file_path: &str,
        head_oid: &Oid,
    ) -> Result<()>;

    /// Count how many commits `split_commit_per_file` would produce for this commit.
    fn count_split_per_file(&self, commit_oid: &Oid) -> Result<usize>;

    /// Count how many commits `split_commit_per_hunk` would produce for this commit.
    fn count_split_per_hunk(&self, commit_oid: &Oid) -> Result<usize>;

    /// Count how many fragmap groups `split_commit_per_hunk_group` would produce
    /// for this commit, given the full branch context up to `head_oid` from
    /// `reference_oid`.
    fn count_split_per_hunk_group(
        &self,
        commit_oid: &Oid,
        head_oid: &Oid,
        reference_oid: &Oid,
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
    fn reword_commit(&self, commit_oid: &Oid, new_message: &str, head_oid: &Oid) -> Result<()>;

    /// Read a string value from the repository's git configuration.
    ///
    /// Returns `Ok(None)` when the key does not exist.
    fn get_config_string(&self, key: &str) -> Result<Option<String>>;

    /// Drop a commit from the branch by cherry-picking its descendants onto
    /// its parent.
    ///
    /// Returns `RebaseOutcome::Complete` when all descendants are
    /// successfully rebased, or `RebaseOutcome::Conflict` when a cherry-pick
    /// step produces merge conflicts. In the conflict case the working tree
    /// and index contain the partially merged state for the user to resolve.
    fn drop_commit(&self, commit_oid: &Oid, head_oid: &Oid) -> Result<RebaseOutcome>;

    /// Move a commit to a different position on the branch.
    ///
    /// Removes `commit_oid` from its current position and inserts it
    /// immediately after `insert_after_oid`. All affected descendants are
    /// cherry-picked in the new order.
    ///
    /// `insert_after_oid` may be the merge-base (reference point) to move
    /// the commit to the very beginning of the branch.
    ///
    /// Returns `RebaseOutcome::Complete` on success or
    /// `RebaseOutcome::Conflict` when a cherry-pick step conflicts.
    fn move_commit(
        &self,
        commit_oid: &Oid,
        insert_after_oid: Option<&Oid>,
        head_oid: &Oid,
    ) -> Result<RebaseOutcome>;

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

    /// Read the crash-safety journal to detect an operation interrupted by a
    /// previous run (process killed or crashed mid-conflict).
    ///
    /// Returns [`JournalStatus::Recovered`] with the persisted [`ConflictState`]
    /// when an interrupted operation is found, so the caller can resume it
    /// (via the normal conflict flow) or abort it.
    fn read_journal(&self) -> Result<JournalStatus>;

    /// Discard the journal's in-progress record without otherwise touching the
    /// repository. Used when a recovered operation is stale (the branch has
    /// moved since it was journaled), so resuming or aborting would be unsafe.
    fn clear_journal(&self) -> Result<()>;

    /// Drop undo/redo history (and its `refs/git-tailor/*` gc-pins) that no
    /// longer matches the branch, and reconcile the remaining pins. Run at
    /// startup so stale refs don't linger in tools like `gitk`; a still-valid
    /// stack is preserved so undo/redo survives across restarts.
    fn prune_stale_journal(&self) -> Result<()>;

    /// Undo the most recent history-rewriting operation by restoring the branch
    /// to the tip recorded before it ran (and moving the record to the redo
    /// stack). Refuses if the working tree is dirty; reports
    /// [`UndoOutcome::Stale`] and discards the stack if the branch no longer
    /// matches the recorded post-operation tip.
    fn undo(&self) -> Result<UndoOutcome>;

    /// Redo the most recently undone operation, restoring its post-operation
    /// tip. Same dirty-tree and staleness rules as [`undo`](Self::undo).
    fn redo(&self) -> Result<UndoOutcome>;

    /// When auto-stash is enabled and the working tree is dirty, stash the
    /// staged/unstaged/untracked changes (recording them in the journal) so a
    /// following operation sees a clean tree. Idempotent: a second call while a
    /// stash is already pending is a no-op. No-op when auto-stash is disabled or
    /// the tree is clean.
    fn autostash_save(&mut self) -> Result<()>;

    /// Reapply the pending auto-stash, restoring the original staged/unstaged
    /// split, and drop it. Returns [`AutostashRestore::Done`] when nothing was
    /// stashed or it reapplied cleanly. On a conflict the stash is **kept** (with
    /// markers left in the working tree) and [`AutostashRestore::Conflict`] is
    /// returned so the caller can open the resolution dialog — the user's changes
    /// are never silently lost.
    fn autostash_restore(&mut self) -> Result<AutostashRestore>;

    /// Finish a conflicting auto-stash reapply: stage the files the user has
    /// resolved and, if none remain conflicted, drop the stash. Returns whether
    /// the resolution is complete or files still conflict.
    fn autostash_conflict_continue(&mut self) -> Result<AutostashContinue>;

    /// Abort a conflicting auto-stash reapply: rewind the branch and working
    /// tree to the pre-operation tip (undoing the operation), restore the
    /// original dirty changes there (always conflict-free), and drop the stash.
    fn autostash_conflict_abort(&mut self) -> Result<()>;

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

    /// Squash two commits into one.
    ///
    /// Creates a single commit that combines `target_oid` (older) and
    /// `source_oid` (newer) by cherry-picking source's diff onto target's
    /// tree. The result replaces target's position in the history and source
    /// is removed. All descendants between target and `head_oid` (excluding
    /// source) are rebased onto the squash commit.
    ///
    /// Returns `RebaseOutcome::Complete` on success or
    /// `RebaseOutcome::Conflict` when a cherry-pick conflicts.
    ///
    /// When the initial squash tree creation (source vs target) conflicts,
    /// the returned `ConflictState` carries a `squash_context` so the TUI
    /// can let the user resolve, then call `squash_finalize`.
    fn squash_commits(
        &self,
        source_oid: &Oid,
        target_oid: &Oid,
        message: &str,
        head_oid: &Oid,
    ) -> Result<RebaseOutcome>;

    /// Test whether combining source onto target produces a conflict.
    ///
    /// Returns `Ok(None)` when the trees merge cleanly (caller should proceed
    /// to open the editor and then call `squash_commits`).
    ///
    /// Returns `Ok(Some(ConflictState))` when the cherry-pick conflicts. The
    /// conflict is written to the working tree and index. The `ConflictState`
    /// carries a `SquashContext` so the TUI can let the user resolve, then
    /// (for squash) open the editor and call `squash_finalize`, or (for fixup)
    /// call `squash_finalize` directly without opening the editor.
    fn squash_try_combine(
        &self,
        source_oid: &Oid,
        target_oid: &Oid,
        combined_message: &str,
        squash_mode: SquashMode,
        head_oid: &Oid,
    ) -> Result<Option<ConflictState>>;

    /// Finalize a squash after the user resolved a squash-time tree conflict.
    ///
    /// Reads the resolved index, creates the squash commit with `message`,
    /// then cherry-picks the descendants listed in `ctx`. Returns
    /// `RebaseOutcome::Complete` or `Conflict` for a descendant conflict.
    fn squash_finalize(
        &self,
        ctx: &SquashContext,
        message: &str,
        original_branch_oid: &Oid,
    ) -> Result<RebaseOutcome>;

    /// Stage a working-tree file, clearing any conflict entries for that path.
    ///
    /// Equivalent to `git add <path>`. Reads the file from the working directory,
    /// adds it to the index at stage 0 (which removes stages 1/2/3), and writes
    /// the updated index to disk. Must be called after a merge tool resolves a
    /// conflict so that subsequent `index.has_conflicts()` checks return false.
    fn stage_file(&self, path: &str) -> Result<()>;

    /// Auto-stage conflicting files whose working-tree content no longer
    /// contains conflict markers.
    ///
    /// When a user resolves conflicts in an external editor (instead of the
    /// built-in mergetool), the index still carries stage 1/2/3 entries.
    /// This method reads each file from disk and stages it if the standard
    /// `<<<<<<<` marker is absent, so that `index.has_conflicts()` reflects
    /// the actual resolution state.
    fn auto_stage_resolved_conflicts(&self, files: &[String]) -> Result<()>;

    /// Return the OID of the root (parentless) commit reachable from HEAD.
    ///
    /// Walks the ancestry of HEAD until it finds a commit with no parents.
    /// For repositories with a single linear history this is the initial commit.
    fn root_commit_oid(&self) -> Result<Oid>;

    /// Return the name of the repository's default upstream branch.
    ///
    /// Looks up the symbolic target of `refs/remotes/origin/HEAD` (the pointer
    /// that `git remote set-head origin --auto` sets) and strips the
    /// `refs/remotes/` prefix so the returned value can be passed directly to
    /// `find_reference_point`.  For example when `origin/HEAD` points to
    /// `refs/remotes/origin/main` this returns `Some("origin/main")`.
    ///
    /// Returns `Ok(None)` when the remote tracking ref is absent or has no symbolic
    /// target (e.g. the repo has no remote configured, or `origin/HEAD` was
    /// never set).
    fn default_branch(&self) -> Result<Option<String>>;

    /// Yield commits incrementally from `from_oid` to `to_oid` (oldest first).
    ///
    /// Unlike `list_commits`, this streams one `CommitInfo` per `.next()` call
    /// so callers can render progress between iterations. The OID range and
    /// result ordering are identical to `list_commits`.
    fn commit_walker<'a>(
        &'a self,
        from_oid: &Oid,
        to_oid: &Oid,
    ) -> Result<Box<dyn Iterator<Item = Result<CommitInfo>> + 'a>>;
}

impl ConflictState {
    /// Whether this conflict arose from a squash-time tree conflict
    /// (as opposed to a descendant rebase conflict).
    pub fn is_squash_tree_conflict(&self) -> bool {
        self.squash_context.is_some()
    }
}
