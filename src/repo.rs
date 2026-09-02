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

/// Default number of diff context lines, matching git's own default.
pub const DEFAULT_CONTEXT_LINES: u32 = 3;

/// Result of reading the crash-safety journal at startup.
///
/// Crash detection keys off an in-progress record inside the journal, not the
/// file's existence — so `None` covers both "no journal file" and "a journal
/// exists but holds no interrupted operation".
#[derive(Debug)]
pub enum JournalStatus {
    /// No interrupted operation to recover.
    None,
    /// An operation was interrupted; the persisted record can resume/abort a
    /// paused conflict or restore an in-progress Edit.
    Recovered(Box<InProgress>),
    /// The journal was written by a newer git-tailor (`version` exceeds what
    /// this build understands). The file is left untouched.
    NewerVersion(u32),
    /// An older journal held an operation interrupted mid-flight whose record is
    /// not representable in the current schema, so this version cannot resume it.
    /// The file is left untouched (an older git-tailor can still finish the
    /// operation). `op` names the interrupted operation.
    UpgradeInterrupted { op: String },
    /// The journal could not be read or parsed; the message describes why.
    Corrupt(String),
}

/// Summary of what [`GitRepo::clean_journal`] removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JournalCleanSummary {
    /// Number of `refs/git-tailor/*` refs deleted.
    pub refs_removed: usize,
    /// Whether an on-disk journal file was present and removed.
    pub journal_removed: bool,
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

/// Result of a stage-all / unstage-all operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageOutcome {
    /// The index changed and an undo entry was recorded.
    Changed,
    /// The index was already in the requested state — nothing to do.
    NoOp,
}

/// Result of a commit-staged operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitOutcome {
    /// A commit was created from the staged changes.
    Committed,
    /// Nothing was staged (the index matched HEAD), so no commit was made.
    NothingStaged,
}

/// Which working-tree row a squash or fixup draws its changes from — the
/// synthetic "staged" / "unstaged" entries the commit list shows above the real
/// commits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum WorktreeSource {
    #[default]
    Staged,
    Unstaged,
}

impl WorktreeSource {
    /// How the row is named in status messages, sentence-initial.
    pub fn label(self) -> &'static str {
        match self {
            WorktreeSource::Staged => "Staged changes",
            WorktreeSource::Unstaged => "Unstaged changes",
        }
    }

    /// The row a fold from this one leaves behind, whose changes have to go
    /// back where they came from once the fold lands.
    pub fn other(self) -> Self {
        match self {
            WorktreeSource::Staged => WorktreeSource::Unstaged,
            WorktreeSource::Unstaged => WorktreeSource::Staged,
        }
    }
}

/// A working-tree row lifted into a temporary commit, with the pre-operation
/// state the fold must be able to get back to.
///
/// The three trees pin down the whole starting point: `tip_before` is where the
/// branch sat, `index_tree_before` what was staged, and `worktree_tree` what was
/// on disk (tracked paths only — untracked files belong to neither row and are
/// never touched). Restoring from them is exact and needs no merge, which is why
/// these operations do not go through the auto-stash.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LiftedRow {
    /// The row whose changes were lifted into the temporary commit.
    pub source: WorktreeSource,
    /// The branch tip before the temporary commit was created.
    pub tip_before: Oid,
    /// The index tree before the operation.
    pub index_tree_before: Oid,
    /// The working tree (tracked paths) as a tree object. Unchanged by the
    /// operation — it only moves content between committed, staged and unstaged.
    pub worktree_tree: Oid,
    /// The temporary commit's tree: the working tree with the row's changes
    /// taken out of it. The merge base for putting the other row's changes back
    /// on top of wherever the squash ended up.
    pub source_tree: Oid,
    /// The temporary commit itself, which the fold left the branch on. Its diff
    /// against its parent is exactly the row's diff, so it serves as both
    /// `source_oid` and `head_oid` for the squash built on it. Also identifies
    /// the record as belonging to the operation in hand: a record whose
    /// temporary commit is not where the branch is describes something that has
    /// already moved on, and unwinding it would take the user's later work with
    /// it.
    pub temp_oid: Oid,
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
    /// Paths of files that have conflict markers in the index (stage > 0).
    /// Collected at the point of conflict so the dialog can list them.
    pub conflicting_files: Vec<String>,
    /// True when `rebase_continue` was called but the index still had
    /// unresolved entries. The dialog uses this to show a warning to the user.
    pub still_unresolved: bool,
    /// How the conflict is carried to completion once the user resolves it.
    pub resume: Resume,
    /// Set only for a step of an in-progress autofixup batch, so the batch
    /// continues after this pair's conflict resolves. Orthogonal to `resume`
    /// (a batch step can conflict either in its squash tree or its descendants).
    pub autofixup_context: Option<AutofixupContext>,
}

/// How a resolved conflict is carried to completion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Resume {
    /// Re-commit the resolved index at the conflicting commit, then continue
    /// cherry-picking `remaining_oids`. `orphan_root` makes the conflicting
    /// commit a parentless root (dropping or moving the root commit);
    /// `moved_commit_oid` is a display hint naming the commit a Move relocated.
    Chain {
        /// Commits still to cherry-pick after the resolved one, in order
        /// (oldest first).
        remaining_oids: Vec<Oid>,
        orphan_root: bool,
        moved_commit_oid: Option<Oid>,
    },
    /// Finalize a squash (open the editor if needed, then `squash_finalize`)
    /// after the user resolves the squash-tree conflict.
    Squash(SquashContext),
    /// Put the *other* working-tree row back after the user resolves its clash
    /// with the fold's own resolution. The history is already rewritten by the
    /// time this conflict is raised — what is left is where the row's changes
    /// go, so continuing settles the working tree and aborting unwinds the whole
    /// fold through [`LiftedRow`].
    CarryRow(LiftedRow),
}

impl Default for Resume {
    fn default() -> Self {
        Resume::Chain {
            remaining_oids: Vec::new(),
            orphan_root: false,
            moved_commit_oid: None,
        }
    }
}

/// The journal's in-progress record: either a paused conflict awaiting
/// resolution, or an "Edit" shell session in progress (which carries no
/// conflict — just enough to restore the branch on abort/crash).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InProgress {
    Conflict(Box<ConflictState>),
    Edit(EditInProgress),
    /// A working-tree row has been lifted into a temporary commit but the squash
    /// built on it has not reached a conflict or completion yet. Recovery
    /// rewinds it.
    WorktreeSquash(LiftedRow),
}

/// State captured while an "Edit" (interactive shell edit of a commit) is in
/// progress, so a crash can restore the branch even if the user left HEAD
/// detached or on another branch from inside the shell.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct EditInProgress {
    /// Full name of the branch ref being edited (e.g. `refs/heads/main`), so
    /// recovery/abort can restore it by name regardless of where HEAD points.
    pub branch_refname: String,
    /// The branch tip before the edit began, restored on abort.
    pub original_branch_oid: Oid,
    /// The commit being edited.
    pub edited_commit_oid: Oid,
}

impl InProgress {
    /// The pre-operation branch tip this record pins and restores, whichever
    /// kind of record it is.
    pub fn original_branch_oid(&self) -> &Oid {
        match self {
            InProgress::Conflict(c) => &c.original_branch_oid,
            InProgress::Edit(e) => &e.original_branch_oid,
            InProgress::WorktreeSquash(s) => &s.tip_before,
        }
    }
}

/// Result of finishing an in-progress "Edit".
#[derive(Debug)]
pub enum EditOutcome {
    /// The user's authored commit chain was spliced in and descendants replayed.
    Complete,
    /// Replaying the original descendants onto the edited chain conflicted;
    /// resolve via the normal conflict flow (`RebaseConflict`).
    Conflict(Box<ConflictState>),
    /// The user made no change (exited the shell with the commit untouched);
    /// the branch was restored and nothing was rewritten.
    Cancelled,
}

/// Extra state carried through an in-progress autofixup batch's conflict so
/// it can be resumed correctly.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AutofixupContext {
    /// The range's base, needed to re-scan for the next fixup/target pair
    /// once the current conflict resolves.
    pub reference_oid: Oid,
    /// User-edited final messages chosen up front in the confirmation dialog,
    /// keyed by the target commit's *original* summary text (stable across
    /// the batch's cascading rebases, unlike its OID). Applied only once —
    /// to the last pair squashed into a given target — so an intermediate
    /// step in a multi-fixup group never renames the target before the
    /// remaining fixups in that group have had a chance to match it.
    pub message_overrides: std::collections::HashMap<String, String>,
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

/// Read-only git queries: HEAD/refs, commit walking, diffs, config, and index
/// inspection. No method mutates history or the working tree, so consumers that
/// only render or load (the loader, the detail/main views, the editor resolver)
/// depend on this narrow trait rather than the full [`GitRepo`] surface — and
/// their test doubles need only implement these.
pub trait RepoRead {
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
    /// metadata and every file/hunk/line changed. `context_lines` is the number
    /// of unchanged lines shown around each hunk; git merges hunks whose context
    /// regions overlap, so a larger value yields fewer, larger hunks.
    fn commit_diff(&self, oid: &Oid, context_lines: u32) -> Result<CommitDiff>;

    /// Extract commit diff with zero context lines, suitable for fragmap analysis.
    ///
    /// The fragmap algorithm needs each logical change as its own hunk. With
    /// the default 3-line context, git merges adjacent hunks together which
    /// produces fewer but larger hunks — breaking the SPG's fine-grained
    /// span tracking.
    fn commit_diff_for_fragmap(&self, oid: &Oid) -> Result<CommitDiff>;

    /// Return a synthetic `CommitDiff` for changes staged in the index (index vs
    /// HEAD), with `context_lines` of surrounding context for the detail view.
    ///
    /// Returns `Ok(None)` when the index is clean (no staged changes).
    fn staged_diff(&self, context_lines: u32) -> Result<Option<CommitDiff>>;

    /// Like [`staged_diff`](Self::staged_diff) but with zero-context tight spans
    /// for fragmap analysis (see [`commit_diff_for_fragmap`](Self::commit_diff_for_fragmap)).
    fn staged_diff_for_fragmap(&self) -> Result<Option<CommitDiff>>;

    /// Return a synthetic `CommitDiff` for unstaged working-tree changes (workdir
    /// vs index), with `context_lines` of surrounding context for the detail view.
    ///
    /// Returns `Ok(None)` when the working tree is clean relative to the index.
    fn unstaged_diff(&self, context_lines: u32) -> Result<Option<CommitDiff>>;

    /// Like [`unstaged_diff`](Self::unstaged_diff) but with zero-context tight
    /// spans for fragmap analysis.
    fn unstaged_diff_for_fragmap(&self) -> Result<Option<CommitDiff>>;

    /// Read a string value from the repository's git configuration.
    ///
    /// Returns `Ok(None)` when the key does not exist.
    fn get_config_string(&self, key: &str) -> Result<Option<String>>;

    /// Return the path of the repository's working directory, if any.
    ///
    /// Bare repositories have no working directory and return `None`.
    fn workdir(&self) -> Option<std::path::PathBuf>;

    /// Whether the working tree or index differs from HEAD (submodule pointer
    /// updates ignored). Used by the Edit flow to re-prompt the shell when the
    /// user left uncommitted changes, so they are never silently discarded.
    fn is_worktree_dirty(&self) -> Result<bool>;

    /// Read the raw blob content of a specific index stage for a conflicted path.
    ///
    /// Stage 1 = base (common ancestor), 2 = ours, 3 = theirs.
    /// Returns `None` when that stage entry does not exist for the path.
    fn read_index_stage(&self, path: &str, stage: i32) -> Result<Option<Vec<u8>>>;

    /// Return the list of paths that currently have conflict markers in the index
    /// (entries with stage > 0), sorted alphabetically and deduplicated.
    fn read_conflicting_files(&self) -> Vec<String>;

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

    /// Yield commits incrementally from `from_oid` to `to_oid`, newest first.
    ///
    /// Unlike `list_commits`, this streams one `CommitInfo` per `.next()` call
    /// so callers can render progress between iterations. It covers the same OID
    /// range but yields it in the opposite order: `list_commits` collects the
    /// whole walk and reverses it to oldest-first, which a stream cannot do
    /// without buffering everything and giving up the incremental progress this
    /// exists for. Callers wanting oldest-first reverse what they collected, as
    /// `loader::load_with_progress` does.
    fn commit_walker<'a>(
        &'a self,
        from_oid: &Oid,
        to_oid: &Oid,
    ) -> Result<Box<dyn Iterator<Item = Result<CommitInfo>> + 'a>>;
}

/// Every history-mutating and journal/undo/staging operation on a repository —
/// the write half of the abstraction (see [`RepoRead`] for the read half).
pub trait RepoWrite {
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
    /// they sit in the same column — their clusters are touched by the same set of
    /// commits, which is what deduplication merges on — *and* their lines concern
    /// the same commits. This yields fewer, more cohesive commits than per-hunk
    /// splitting. Hunks in different columns are never merged, but a hunk that
    /// spans columns is divided only as far as is needed to make the split
    /// possible, so the result can have fewer commits than the matrix has
    /// columns.
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

    /// Peel a set of selected files out of `commit_oid` into a follow-up commit.
    ///
    /// Produces exactly two commits: the first keeps every
    /// file *not* selected under the original (unchanged) message; the second
    /// contains only the selected files' changes, with a summary suffix
    /// naming the file (or a file count when more than one). All commits
    /// between `commit_oid` (exclusive) and `head_oid` (inclusive) are
    /// rebased onto the result and the branch ref is fast-forwarded.
    ///
    /// Fails if:
    /// - `file_paths` is empty
    /// - `file_paths` contains a path not changed by the commit
    /// - `file_paths` covers every file in the commit (nothing would remain)
    /// - staged or unstaged changes share file paths with the commit being split
    /// - a rebase conflict occurs while rebuilding descendants
    fn split_commit_out_files(
        &self,
        commit_oid: &Oid,
        file_paths: &[String],
        head_oid: &Oid,
    ) -> Result<()>;

    /// Peel a set of selected hunks out of `commit_oid` into a follow-up commit.
    ///
    /// `hunks` identifies each selected hunk as `(delta_idx, hunk_idx)` against
    /// the commit's diff at `context_lines` of context (`delta_idx` is the
    /// file's position in diff order, `hunk_idx` its position within that
    /// file) — the same context level a caller displaying the diff must use
    /// when deriving these indices, since more context can merge what would
    /// otherwise be separate hunks into one. Produces exactly two commits: the
    /// first keeps everything *not* selected under the original (unchanged)
    /// message; the second contains only the selected hunks, with a summary
    /// suffix describing them (the file name when they're all in one file, a
    /// hunk/file count otherwise). All commits between `commit_oid`
    /// (exclusive) and `head_oid` (inclusive) are rebased onto the result and
    /// the branch ref is fast-forwarded.
    ///
    /// Fails if:
    /// - `hunks` is empty
    /// - `hunks` contains an out-of-range `(delta_idx, hunk_idx)` pair
    /// - `hunks` covers every hunk in the commit (nothing would remain)
    /// - staged or unstaged changes share file paths with the commit being split
    /// - a rebase conflict occurs while rebuilding descendants
    fn split_commit_out_hunks(
        &self,
        commit_oid: &Oid,
        hunks: &[(usize, usize)],
        head_oid: &Oid,
        context_lines: u32,
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

    /// Begin editing `commit_oid` (interactive-rebase's `edit`): rewind the
    /// current branch to that commit and check it out, so the caller can drop
    /// the user into a shell to rewrite it by hand (amend, or `git reset` +
    /// re-commit to split it into several commits). The branch's descendants
    /// are held via the write-ahead journal until [`Self::finish_edit`].
    ///
    /// Fails (leaving the branch untouched) if the working tree is dirty, or the
    /// commit is a merge or the root commit.
    fn begin_edit(&self, commit_oid: &Oid, head_oid: &Oid) -> Result<()>;

    /// Finish an edit begun with [`Self::begin_edit`]: take the user-authored
    /// commit chain now on the branch, validate it, replay the original
    /// descendants onto it, and advance the branch. `commit_oid` is the commit
    /// that was edited; the original branch tip and branch name are read from
    /// the in-progress journal.
    ///
    /// Returns `Cancelled` when nothing changed, `Conflict` when replaying
    /// descendants conflicts, or `Complete` on success. On an unexpected
    /// repository state (uncommitted leftovers, HEAD moved off the branch, a
    /// merge commit, or commits that don't build on the edited commit's parent)
    /// it restores the branch to its original tip and returns an error.
    fn finish_edit(&self, commit_oid: &Oid) -> Result<EditOutcome>;

    /// Abort (or crash-recover) an in-progress edit: restore the branch to its
    /// original tip and check it out, using the branch name and original tip
    /// recorded in the in-progress journal. A no-op when no edit is in progress.
    fn abort_edit(&self) -> Result<()>;

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

    /// Discard whatever the journal says is in flight — the paused record *and*
    /// any working-tree snapshot — without otherwise touching the repository.
    /// Used when a recovered operation is stale (the branch has moved since it
    /// was journaled), so resuming or aborting would be unsafe. The snapshot has
    /// to go too: left behind, it keeps telling later operations that a dirty
    /// working tree is accounted for.
    fn clear_journal(&self) -> Result<()>;

    /// Drop undo/redo history (and its `refs/git-tailor/*` gc-pins) that no
    /// longer matches the branch, and reconcile the remaining pins. Run at
    /// startup so stale refs don't linger in tools like `gitk`; a still-valid
    /// stack is preserved so undo/redo survives across restarts.
    fn prune_stale_journal(&self) -> Result<()>;

    /// Remove **all** git-tailor recovery state: every ref under
    /// `refs/git-tailor/` and the on-disk journal file. Refs are found by
    /// namespace rather than from the journal, so stray refs are removed even if
    /// the journal is missing or out of sync. A manual escape hatch (the
    /// `--clean-journal` CLI flag); returns a summary of what was removed.
    fn clean_journal(&self) -> Result<JournalCleanSummary>;

    /// Undo the most recent history-rewriting operation by restoring the branch
    /// to the tip recorded before it ran (and moving the record to the redo
    /// stack). Refuses if the working tree is dirty; reports
    /// [`UndoOutcome::Stale`] and discards the stack if the branch no longer
    /// matches the recorded post-operation tip.
    ///
    /// Undoing a working-tree fold moves the branch and the index and leaves the
    /// files alone, which restores everything a fold that ran cleanly touched —
    /// it only ever moved content between committed, staged and unstaged. A fold
    /// that went through a conflict also changed the files, to whatever the user
    /// resolved to, and that stands: what they replaced is gone the moment they
    /// resolve, so there is nothing left to put back.
    fn undo(&self) -> Result<UndoOutcome>;

    /// Redo the most recently undone operation, restoring its post-operation
    /// tip. Same dirty-tree and staleness rules as [`undo`](Self::undo).
    fn redo(&self) -> Result<UndoOutcome>;

    /// Whether the next [`undo`](Self::undo) leaves the working tree untouched (a
    /// stage/unstage-all op or a commit's soft reset). The caller uses this to
    /// skip the auto-stash dance, which would otherwise stash away and reapply
    /// the very state being restored.
    fn pending_undo_skips_autostash(&self) -> Result<bool>;

    /// Whether the next [`redo`](Self::redo) leaves the working tree untouched.
    /// See [`pending_undo_skips_autostash`](Self::pending_undo_skips_autostash).
    fn pending_redo_skips_autostash(&self) -> Result<bool>;

    /// Stage all changes to tracked files (modifications and deletions), like
    /// `git add -u`. Untracked files are left alone, matching what the unstaged
    /// row's diff shows. Recorded as an undoable index-only operation. Returns
    /// [`StageOutcome::NoOp`] when there was nothing to stage.
    fn stage_all(&self) -> Result<StageOutcome>;

    /// Unstage all staged changes by resetting the index to HEAD. Recorded as an
    /// undoable index-only operation. Returns [`StageOutcome::NoOp`] when there
    /// was nothing staged.
    fn unstage_all(&self) -> Result<StageOutcome>;

    /// Create a commit from the currently staged changes with `message`, using
    /// HEAD as the parent and advancing the branch ref. Recorded as an undoable
    /// operation whose undo is a soft reset (the committed changes reappear as
    /// staged). Returns [`CommitOutcome::NothingStaged`] when the index matches
    /// HEAD.
    fn commit_staged(&self, message: &str) -> Result<CommitOutcome>;

    /// Lift the staged or unstaged working-tree changes into a temporary commit
    /// on top of HEAD, so the squash machinery can take them as its source.
    ///
    /// The commit's diff against its parent is exactly the row's diff, and the
    /// *other* row's changes are left in the index and working tree, still on
    /// their own side of the staged/unstaged line. Returns `None` when the row
    /// has nothing to fold in.
    ///
    /// Records the returned row in the journal write-ahead, so a crash before
    /// the squash finishes leaves a recoverable temporary commit rather than a
    /// stray one. Every path out of the operation must end in either
    /// [`restore_lifted_row`](Self::restore_lifted_row) or a completed squash.
    ///
    /// Fails when the unstaged row cannot be separated from the staged one —
    /// edits to the same lines have no meaningful split.
    fn lift_worktree_row(&self, source: WorktreeSource) -> Result<Option<LiftedRow>>;

    /// Unwind [`lift_worktree_row`](Self::lift_worktree_row): put the branch,
    /// the index and the working tree back exactly as `lifted` recorded them,
    /// and clear the journal record. Untracked files are left alone.
    fn restore_lifted_row(&self, lifted: &LiftedRow) -> Result<()>;

    /// Keep the working tree `lifted` recorded reachable under a ref, for a
    /// record that is about to be discarded because the branch has moved past
    /// it.
    ///
    /// Returns the ref's name, or `None` when the recorded tree is what HEAD
    /// already holds and there is nothing to lose. The ref lives under the
    /// git-tailor namespace, so `--clean-journal` sweeps it along with the rest.
    fn rescue_lifted_row(&self, lifted: &LiftedRow) -> Result<Option<String>>;

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
    ///
    /// `autofixup_context` is `Some` when this finalizes a step of an
    /// in-progress autofixup batch: once this step completes, the remaining
    /// fixup/target pairs in the batch are applied before returning, so the
    /// batch keeps going through a squash-time conflict the same way it does
    /// through a descendant one.
    fn squash_finalize(
        &self,
        ctx: &SquashContext,
        message: &str,
        original_branch_oid: &Oid,
        autofixup_context: Option<&AutofixupContext>,
    ) -> Result<RebaseOutcome>;

    /// Bulk-squash every `fixup!`/`squash!`-prefixed commit in
    /// `reference_oid..head_oid` into the earlier commit its summary names
    /// (see [`crate::autofixup::plan_autofixup`]), bottom-up so multiple
    /// fixups aimed at the same target stack correctly. `squash!` pairs
    /// combine messages non-interactively (target message, blank line,
    /// source message — the same default text the manual squash editor
    /// starts from); `fixup!` pairs keep the target's message unchanged.
    /// Commits with no resolvable target are left in place.
    ///
    /// `message_overrides` (keyed by a target's original summary text) lets
    /// the caller pin the final message for a target's whole fixup/squash
    /// group instead of the computed default — see
    /// [`AutofixupContext::message_overrides`].
    ///
    /// The whole batch is one undoable operation: on success a single undo
    /// entry restores `head_oid`. Returns `RebaseOutcome::Conflict` if any
    /// individual squash step conflicts; resuming via
    /// [`rebase_continue`](Self::rebase_continue) continues the remaining
    /// pairs in the same batch.
    fn autofixup(
        &self,
        head_oid: &Oid,
        reference_oid: &Oid,
        message_overrides: &std::collections::HashMap<String, String>,
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
}

/// A full repository handle: everything a caller can do, read and write. This is
/// the bundle bound most consumers use; the read-only ones (loader, views,
/// editor) narrow to [`RepoRead`] instead. Automatically implemented for any type
/// that is both [`RepoRead`] and [`RepoWrite`], so implementors just provide
/// those two — there is nothing to write here.
pub trait GitRepo: RepoRead + RepoWrite {}

impl<T: RepoRead + RepoWrite> GitRepo for T {}

impl ConflictState {
    /// Whether this conflict arose from a squash-time tree conflict
    /// (as opposed to a descendant rebase conflict).
    pub fn is_squash_tree_conflict(&self) -> bool {
        matches!(self.resume, Resume::Squash(_))
    }

    /// Commits still to cherry-pick after the resolved one. Only a chain
    /// continuation carries these; a squash-tree conflict resumes via
    /// `squash_finalize` and reports none.
    pub fn remaining_oids(&self) -> &[Oid] {
        match &self.resume {
            Resume::Chain { remaining_oids, .. } => remaining_oids,
            Resume::Squash(_) | Resume::CarryRow(_) => &[],
        }
    }

    /// Whether this conflict is the *other* working-tree row failing to land on
    /// what the user resolved the fold to, rather than a rewrite conflicting.
    pub fn is_carry_conflict(&self) -> bool {
        matches!(self.resume, Resume::CarryRow(_))
    }
}
