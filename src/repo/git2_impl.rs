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

use anyhow::{Context, Result};
use std::collections::HashSet;

use crate::{CommitDiff, CommitInfo, Oid, app::SquashMode};

use super::{RepoRead, RepoWrite};

/// Convert a libgit2 OID into our domain `Oid` type.
impl From<git2::Oid> for Oid {
    fn from(oid: git2::Oid) -> Self {
        Oid::new(oid.to_string())
    }
}

/// Convert our domain `Oid` back into a libgit2 OID.
impl From<&Oid> for git2::Oid {
    fn from(oid: &Oid) -> Self {
        git2::Oid::from_str(oid.long()).expect("Oid always holds a valid git OID hex string")
    }
}

mod autofixup_op;
mod cherry_pick;
mod commit_staged_op;
mod conflict;
mod drop_op;
mod edit_op;
mod hunks;
mod journal;
mod lift_op;
mod move_op;
mod reads;
mod reword_op;
mod split_op;
mod squash_op;
mod stage_op;
mod stash;

/// Concrete git repository backed by `libgit2` via the `git2` crate.
///
/// Construct with [`Git2Repo::open`]; then use through the [`GitRepo`] trait.
pub struct Git2Repo {
    inner: git2::Repository,
    /// When true, operations that need a clean working tree auto-stash dirty
    /// state instead of refusing (see [`RepoWrite::autostash_save`]).
    autostash: bool,
}

impl Git2Repo {
    /// Try to open a git repository by iteratively trying the given path and
    /// its parents until a repository root is found.
    pub fn open(mut path: std::path::PathBuf) -> Result<Self> {
        loop {
            let result = git2::Repository::open(&path);
            if let Ok(repo) = result {
                return Ok(Git2Repo {
                    inner: repo,
                    autostash: false,
                });
            }
            if !path.pop() {
                anyhow::bail!("Could not find git repository root");
            }
        }
    }

    /// Enable or disable auto-stash for this session (from the `--autostash`
    /// flag / `GT_AUTOSTASH`).
    pub fn set_autostash(&mut self, enabled: bool) {
        self.autostash = enabled;
    }

    /// List local/remote-tracking branch and tag names, for shell completion of
    /// the `base` argument.
    pub fn list_ref_names(&self) -> Result<Vec<String>> {
        reads::list_ref_names(self)
    }

    /// Path to the repository's git directory (the `.git` dir for a normal repo).
    fn git_dir(&self) -> &std::path::Path {
        self.inner.path()
    }

    /// Persist or clear the crash-safety journal based on a rebase operation's
    /// outcome: record the conflict state on `Conflict` (so an interrupted
    /// resolution can be recovered), clear it on `Complete` and push an undo
    /// entry from `tip_before` to the resulting tip. Errors are passed through
    /// untouched.
    fn journaled(
        &self,
        label: &str,
        tip_before: &Oid,
        outcome: Result<super::RebaseOutcome>,
    ) -> Result<super::RebaseOutcome> {
        if let Ok(out) = &outcome {
            match out {
                super::RebaseOutcome::Conflict(state) => {
                    journal::set_in_progress(self, &super::InProgress::Conflict(state.clone()))?
                }
                super::RebaseOutcome::Complete => {
                    journal::clear_in_progress(self)?;
                    // The snapshot belongs to this operation only if the
                    // operation started from the temporary commit it made.
                    // Anything else is a record stranded by an earlier run, and
                    // restoring its working tree over this result — and calling
                    // that this operation's undo entry — would be wrong twice
                    // over.
                    match journal::worktree_source(self)? {
                        Some(snapshot) if tip_before == &snapshot.temp_oid => {
                            // The rewrite landed, but the other row may have
                            // nowhere to land: that is a conflict of its own,
                            // and the operation is not complete until it is
                            // resolved.
                            if let Some(state) = self.finish_worktree_source(label, &snapshot)? {
                                return Ok(super::RebaseOutcome::Conflict(Box::new(state)));
                            }
                        }
                        _ => self.record_undo_if_changed(label, tip_before)?,
                    }
                }
            }
        }
        outcome
    }

    /// Complete a squash whose source was a working-tree row: put the other
    /// row's changes back where they came from and record the operation as one
    /// undoable step.
    ///
    /// The undo has to restore the index alongside the branch — the row's
    /// changes were staged or unstaged before and are committed after — so this
    /// records a mixed reset rather than the plain ref move every other
    /// operation uses.
    ///
    /// Returns the conflict to resolve when the other row's changes cannot be
    /// carried onto what the user resolved the fold to. The rewrite stands
    /// either way; only the working tree is still in the air, and the record
    /// stays in the journal until it settles.
    fn finish_worktree_source(
        &self,
        label: &str,
        snapshot: &super::LiftedRow,
    ) -> Result<Option<super::ConflictState>> {
        let tip_after = reads::head_oid(self)?;
        let index_tree_after = match lift_op::finish(self, snapshot, &tip_after)? {
            lift_op::Settled::Done(index_tree) => index_tree,
            lift_op::Settled::Clash(merged) => {
                let state = super::ConflictState {
                    operation_label: label.to_string(),
                    // The lift is what an abort rewinds to, which unwinds the
                    // whole fold — the rewrite included.
                    original_branch_oid: snapshot.temp_oid.clone(),
                    new_tip_oid: tip_after.clone(),
                    conflicting_commit_oid: tip_after,
                    conflicting_files: lift_op::clashing_paths(&merged),
                    still_unresolved: false,
                    resume: super::Resume::CarryRow(snapshot.clone()),
                    autofixup_context: None,
                };
                // Write-ahead: the markers are about to go on disk, and a crash
                // between the two would leave them there unexplained.
                journal::set_in_progress(
                    self,
                    &super::InProgress::Conflict(Box::new(state.clone())),
                )?;
                lift_op::write_clash(self, &merged)?;
                return Ok(Some(state));
            }
        };
        journal::record_mixed_undo(
            self,
            label,
            journal::MixedUndo {
                tip_before: &snapshot.tip_before,
                tip_after: &tip_after,
                index_tree_before: &snapshot.index_tree_before,
                index_tree_after: &index_tree_after,
            },
        )?;
        journal::set_worktree_source(self, None)?;
        Ok(None)
    }

    /// Wrap a `Result<()>` operation (reword, split): on success, record an
    /// undo entry from `tip_before` to the resulting tip.
    fn record_unit_undo(&self, label: &str, tip_before: &Oid, result: Result<()>) -> Result<()> {
        result?;
        self.record_undo_if_changed(label, tip_before)
    }

    /// Push an undo entry from `tip_before` to the current HEAD, unless the
    /// branch did not actually move.
    fn record_undo_if_changed(&self, label: &str, tip_before: &Oid) -> Result<()> {
        if let Ok(after) = reads::head_oid(self)
            && &after != tip_before
        {
            journal::record_undo(self, label, tip_before, &after)?;
        }
        Ok(())
    }

    /// Run an index-only operation (stage/unstage all), recording an undo entry
    /// from the before-tree to the after-tree. Reports `NoOp` when the index tree
    /// is unchanged, so nothing is journalled.
    fn journaled_index_op(
        &self,
        label: &str,
        op: impl FnOnce(&Self) -> Result<()>,
    ) -> Result<super::StageOutcome> {
        let head = reads::head_oid(self)?;
        let before = journal::current_index_tree(self)?;
        op(self)?;
        let after = journal::current_index_tree(self)?;
        if before == after {
            return Ok(super::StageOutcome::NoOp);
        }
        journal::record_index_undo(self, label, &head, &before, &after)?;
        Ok(super::StageOutcome::Changed)
    }

    pub(super) fn stage_file(&self, path: &str) -> Result<()> {
        let mut index = self.inner.index().context("failed to read index")?;
        index
            .read(true)
            .context("failed to refresh index from disk")?;

        let workdir = self
            .inner
            .workdir()
            .ok_or_else(|| anyhow::anyhow!("repository has no working directory"))?;

        if workdir.join(path).exists() {
            // File is present — add it to clear conflict stages and create a
            // normal stage-0 entry.
            index
                .add_path(std::path::Path::new(path))
                .with_context(|| format!("failed to stage '{path}'"))?;
        } else {
            // File was deleted — remove all index entries for this path
            // (stages 0, 1, 2, 3) so the deletion is staged and no phantom
            // conflict entries remain.
            index
                .remove_path(std::path::Path::new(path))
                .with_context(|| format!("failed to remove '{path}' from index"))?;
        }

        index
            .write()
            .context("failed to write index after staging")?;
        Ok(())
    }
}

impl RepoRead for Git2Repo {
    fn head_oid(&self) -> Result<Oid> {
        reads::head_oid(self)
    }

    fn find_reference_point(&self, commit_ish: &str) -> Result<Oid> {
        reads::find_reference_point(self, commit_ish)
    }

    fn list_commits(&self, from_oid: &Oid, to_oid: &Oid) -> Result<Vec<CommitInfo>> {
        reads::list_commits(self, from_oid, to_oid)
    }

    fn commit_diff(&self, oid: &Oid, context_lines: u32) -> Result<CommitDiff> {
        reads::commit_diff(self, oid, context_lines)
    }

    fn commit_diff_for_fragmap(&self, oid: &Oid) -> Result<CommitDiff> {
        reads::commit_diff_for_fragmap(self, oid)
    }

    fn staged_diff(&self, context_lines: u32) -> Result<Option<CommitDiff>> {
        reads::staged_diff(self, context_lines)
    }

    fn staged_diff_for_fragmap(&self) -> Result<Option<CommitDiff>> {
        reads::staged_diff_for_fragmap(self)
    }

    fn unstaged_diff(&self, context_lines: u32) -> Result<Option<CommitDiff>> {
        reads::unstaged_diff(self, context_lines)
    }

    fn unstaged_diff_for_fragmap(&self) -> Result<Option<CommitDiff>> {
        reads::unstaged_diff_for_fragmap(self)
    }

    fn get_config_string(&self, key: &str) -> Result<Option<String>> {
        reads::get_config_string(self, key)
    }

    fn workdir(&self) -> Option<std::path::PathBuf> {
        reads::workdir(self)
    }

    fn is_worktree_dirty(&self) -> Result<bool> {
        // Calls the inherent `Git2Repo::is_worktree_dirty` (inherent methods
        // take resolution priority over trait methods), not itself.
        Git2Repo::is_worktree_dirty(self)
    }

    fn read_index_stage(&self, path: &str, stage: i32) -> Result<Option<Vec<u8>>> {
        reads::read_index_stage(self, path, stage)
    }

    fn read_conflicting_files(&self) -> Vec<String> {
        conflict::read_conflicting_files(self)
    }

    fn root_commit_oid(&self) -> Result<Oid> {
        reads::root_commit_oid(self)
    }

    fn default_branch(&self) -> Result<Option<String>> {
        reads::default_branch(self)
    }

    fn commit_walker<'a>(
        &'a self,
        from_oid: &Oid,
        to_oid: &Oid,
    ) -> Result<Box<dyn Iterator<Item = Result<CommitInfo>> + 'a>> {
        reads::commit_walker(self, from_oid, to_oid)
    }
}

impl RepoWrite for Git2Repo {
    fn split_commit_per_file(&self, commit_oid: &Oid, head_oid: &Oid) -> Result<()> {
        self.record_unit_undo(
            "Split",
            head_oid,
            split_op::split_commit_per_file(self, commit_oid, head_oid),
        )
    }

    fn split_commit_per_hunk(&self, commit_oid: &Oid, head_oid: &Oid) -> Result<()> {
        self.record_unit_undo(
            "Split",
            head_oid,
            split_op::split_commit_per_hunk(self, commit_oid, head_oid),
        )
    }

    fn split_commit_per_hunk_group(
        &self,
        commit_oid: &Oid,
        head_oid: &Oid,
        reference_oid: &Oid,
    ) -> Result<()> {
        self.record_unit_undo(
            "Split",
            head_oid,
            split_op::split_commit_per_hunk_group(self, commit_oid, head_oid, reference_oid),
        )
    }

    fn split_commit_out_files(
        &self,
        commit_oid: &Oid,
        file_paths: &[String],
        head_oid: &Oid,
    ) -> Result<()> {
        self.record_unit_undo(
            "Split",
            head_oid,
            split_op::split_commit_out_files(self, commit_oid, file_paths, head_oid),
        )
    }

    fn split_commit_out_hunks(
        &self,
        commit_oid: &Oid,
        hunks: &[(usize, usize)],
        head_oid: &Oid,
        context_lines: u32,
    ) -> Result<()> {
        self.record_unit_undo(
            "Split",
            head_oid,
            split_op::split_commit_out_hunks(self, commit_oid, hunks, head_oid, context_lines),
        )
    }

    fn count_split_per_file(&self, commit_oid: &Oid) -> Result<usize> {
        split_op::count_split_per_file(self, commit_oid)
    }

    fn count_split_per_hunk(&self, commit_oid: &Oid) -> Result<usize> {
        split_op::count_split_per_hunk(self, commit_oid)
    }

    fn count_split_per_hunk_group(
        &self,
        commit_oid: &Oid,
        head_oid: &Oid,
        reference_oid: &Oid,
    ) -> Result<usize> {
        split_op::count_split_per_hunk_group(self, commit_oid, head_oid, reference_oid)
    }

    fn reword_commit(&self, commit_oid: &Oid, new_message: &str, head_oid: &Oid) -> Result<()> {
        self.record_unit_undo(
            "Reword",
            head_oid,
            reword_op::reword_commit(self, commit_oid, new_message, head_oid),
        )
    }

    fn drop_commit(&self, commit_oid: &Oid, head_oid: &Oid) -> Result<super::RebaseOutcome> {
        self.journaled(
            "Drop",
            head_oid,
            drop_op::drop_commit(self, commit_oid, head_oid),
        )
    }

    fn begin_edit(&self, commit_oid: &Oid, head_oid: &Oid) -> Result<()> {
        edit_op::begin_edit(self, commit_oid, head_oid)
    }

    fn finish_edit(&self, commit_oid: &Oid) -> Result<super::EditOutcome> {
        // Capture the undo base (the original branch tip) before `finish_edit`
        // clears the in-progress record on completion.
        let original = journal::in_progress(self)?.map(|s| s.original_branch_oid().clone());
        let outcome = edit_op::finish_edit(self, commit_oid)?;
        if matches!(outcome, super::EditOutcome::Complete)
            && let Some(original) = original
        {
            self.record_undo_if_changed("Edit", &original)?;
        }
        Ok(outcome)
    }

    fn abort_edit(&self) -> Result<()> {
        edit_op::abort_edit(self)
    }

    fn rebase_continue(&self, state: &super::ConflictState) -> Result<super::RebaseOutcome> {
        // A carry conflict is not a rebase step: the history it belongs to is
        // already written, and what is left settles the working tree and records
        // the fold's undo entry itself, so it does not go through `journaled`.
        if let super::Resume::CarryRow(lifted) = &state.resume {
            return lift_op::continue_carry(self, lifted, state);
        }
        if state.autofixup_context.is_some() {
            return self.journaled(
                "Autofixup",
                &state.original_branch_oid,
                autofixup_op::continue_autofixup(self, state),
            );
        }
        self.journaled(
            &state.operation_label,
            &state.original_branch_oid,
            conflict::rebase_continue(self, state),
        )
    }

    fn rebase_abort(&self, state: &super::ConflictState) -> Result<()> {
        // A squash sourced from a working-tree row has a temporary commit below
        // the conflict, holding changes the generic reset knows nothing about.
        // The snapshot rewinds past both, exactly — but only when the operation
        // being aborted is the one that made it. A record stranded by an earlier
        // run names a commit this abort has never heard of, and rewinding to it
        // would take the aborted operation's history with it.
        if let Some(snapshot) = journal::worktree_source(self)?
            && state.original_branch_oid == snapshot.temp_oid
        {
            return lift_op::restore(self, &snapshot);
        }
        conflict::rebase_abort(self, state)?;
        journal::clear_in_progress(self)
    }

    fn read_journal(&self) -> Result<super::JournalStatus> {
        Ok(journal::read(self))
    }

    fn clear_journal(&self) -> Result<()> {
        journal::discard_in_flight(self)
    }

    fn prune_stale_journal(&self) -> Result<()> {
        journal::prune_stale(self)
    }

    fn clean_journal(&self) -> Result<super::JournalCleanSummary> {
        journal::clean(self)
    }

    fn undo(&self) -> Result<super::UndoOutcome> {
        journal::apply_undo(self)
    }

    fn redo(&self) -> Result<super::UndoOutcome> {
        journal::apply_redo(self)
    }

    fn pending_undo_skips_autostash(&self) -> Result<bool> {
        journal::pending_undo_skips_autostash(self)
    }

    fn pending_redo_skips_autostash(&self) -> Result<bool> {
        journal::pending_redo_skips_autostash(self)
    }

    fn stage_all(&self) -> Result<super::StageOutcome> {
        self.journaled_index_op("Stage all", stage_op::stage_all)
    }

    fn unstage_all(&self) -> Result<super::StageOutcome> {
        self.journaled_index_op("Unstage all", stage_op::unstage_all)
    }

    fn commit_staged(&self, message: &str) -> Result<super::CommitOutcome> {
        let before = reads::head_oid(self)?;
        match commit_staged_op::commit_staged(self, message)? {
            None => Ok(super::CommitOutcome::NothingStaged),
            Some(after) => {
                journal::record_commit_undo(self, "Commit", &before, &after)?;
                Ok(super::CommitOutcome::Committed)
            }
        }
    }

    fn lift_worktree_row(&self, source: super::WorktreeSource) -> Result<Option<super::LiftedRow>> {
        lift_op::lift(self, source)
    }

    fn restore_lifted_row(&self, lifted: &super::LiftedRow) -> Result<()> {
        lift_op::restore(self, lifted)
    }

    fn rescue_lifted_row(&self, lifted: &super::LiftedRow) -> Result<Option<String>> {
        lift_op::rescue(self, lifted)
    }

    fn autostash_save(&mut self) -> Result<()> {
        self.save_autostash()
    }

    fn autostash_restore(&mut self) -> Result<crate::repo::AutostashRestore> {
        self.restore_autostash()
    }

    fn autostash_conflict_continue(&mut self) -> Result<crate::repo::AutostashContinue> {
        self.continue_autostash()
    }

    fn autostash_conflict_abort(&mut self) -> Result<()> {
        self.abort_autostash()
    }

    fn move_commit(
        &self,
        commit_oid: &Oid,
        insert_after_oid: Option<&Oid>,
        head_oid: &Oid,
    ) -> Result<super::RebaseOutcome> {
        self.journaled(
            "Move",
            head_oid,
            move_op::move_commit(self, commit_oid, insert_after_oid, head_oid),
        )
    }

    fn squash_commits(
        &self,
        source_oid: &Oid,
        target_oid: &Oid,
        message: &str,
        head_oid: &Oid,
    ) -> Result<super::RebaseOutcome> {
        self.journaled(
            "Squash",
            head_oid,
            squash_op::squash_commits(self, source_oid, target_oid, message, head_oid),
        )
    }

    fn stage_file(&self, path: &str) -> Result<()> {
        self.stage_file(path)
    }

    fn auto_stage_resolved_conflicts(&self, files: &[String]) -> Result<()> {
        conflict::auto_stage_resolved_conflicts(self, files)
    }

    fn squash_try_combine(
        &self,
        source_oid: &Oid,
        target_oid: &Oid,
        combined_message: &str,
        squash_mode: SquashMode,
        head_oid: &Oid,
    ) -> Result<Option<super::ConflictState>> {
        let result = squash_op::squash_try_combine(
            self,
            source_oid,
            target_oid,
            combined_message,
            squash_mode,
            head_oid,
        )?;
        // The squash-tree conflict path writes conflicts to the working tree and
        // returns the state directly (bypassing RebaseOutcome), so journal it here.
        if let Some(state) = &result {
            journal::set_in_progress(self, &super::InProgress::Conflict(Box::new(state.clone())))?;
        }
        Ok(result)
    }

    fn squash_finalize(
        &self,
        ctx: &super::SquashContext,
        message: &str,
        original_branch_oid: &Oid,
        autofixup_context: Option<&super::AutofixupContext>,
    ) -> Result<super::RebaseOutcome> {
        if let Some(autofixup_ctx) = autofixup_context {
            return self.journaled(
                "Autofixup",
                original_branch_oid,
                autofixup_op::continue_autofixup_after_squash_finalize(
                    self,
                    ctx,
                    message,
                    original_branch_oid,
                    autofixup_ctx,
                ),
            );
        }
        self.journaled(
            "Squash",
            original_branch_oid,
            squash_op::squash_finalize(self, ctx, message, original_branch_oid),
        )
    }

    fn autofixup(
        &self,
        head_oid: &Oid,
        reference_oid: &Oid,
        message_overrides: &std::collections::HashMap<String, String>,
    ) -> Result<super::RebaseOutcome> {
        self.journaled(
            "Autofixup",
            head_oid,
            autofixup_op::autofixup(self, head_oid, reference_oid, message_overrides),
        )
    }
}

/// The trees a [`Git2Repo::reset_worktree`] moves between.
///
/// Named fields because all three are tree OIDs: transposed positionally, a
/// reset would silently put the index's content on disk, or the other way
/// round, and still typecheck.
pub(super) struct WorktreeReset {
    /// The tree the working tree reflects on entry. Paths it has that
    /// `worktree_tree` does not are deleted from disk.
    pub from_tree: git2::Oid,
    /// The tree the files on disk must end up matching.
    pub worktree_tree: git2::Oid,
    /// The tree the index must end up holding — equal to `worktree_tree` when
    /// the reset leaves nothing staged.
    pub index_tree: git2::Oid,
}

impl Git2Repo {
    /// Refuse if the working tree or index has any staged or unstaged changes,
    /// ignoring submodule pointer updates (consistent with `git rebase`).
    ///
    /// Gitlink entries (mode `0o160000`) are skipped because libgit2's
    /// `checkout_head` does not recurse into submodule directories, so a dirty
    /// submodule reference cannot be silently discarded.
    ///
    /// Called before operations that end with `checkout_head(force)`, which
    /// would silently discard any dirty state.  The user should stash or
    /// commit their changes before running such operations.
    fn check_no_dirty_state(&self) -> Result<()> {
        // A working-tree-sourced squash deliberately leaves the *other* row's
        // changes in place. They are recorded in the snapshot and restored when
        // the operation finishes, so they are not the unexpected dirt this guard
        // is here to catch — as long as the snapshot still describes what is
        // actually there.
        if lift_op::covers_working_tree(self)? {
            return Ok(());
        }
        if self.is_worktree_dirty()? {
            anyhow::bail!(
                "You have staged or unstaged changes. \
                 Stash or commit them before running this operation."
            );
        }
        Ok(())
    }

    /// Whether the working tree or index has real (non-gitlink) staged or
    /// unstaged changes — the condition that makes the rebase operations refuse
    /// (and that auto-stash, when enabled, stashes away).
    pub(super) fn is_worktree_dirty(&self) -> Result<bool> {
        let mut opts = git2::DiffOptions::new();
        opts.context_lines(0);
        opts.interhunk_lines(0);

        let head_tree = match self.inner.head() {
            Ok(head) => Some(head.peel_to_tree()?),
            Err(err)
                if matches!(
                    err.code(),
                    git2::ErrorCode::NotFound | git2::ErrorCode::UnbornBranch
                ) =>
            {
                None
            }
            Err(err) => return Err(err.into()),
        };

        // Returns true only when the delta is a real file change, not a gitlink.
        let is_real = |delta: git2::DiffDelta| {
            delta.old_file().mode() != git2::FileMode::Commit
                && delta.new_file().mode() != git2::FileMode::Commit
        };

        let has_staged = self
            .inner
            .diff_tree_to_index(head_tree.as_ref(), None, Some(&mut opts))?
            .deltas()
            .any(is_real);

        let has_unstaged = self
            .inner
            .diff_index_to_workdir(None, Some(&mut opts))?
            .deltas()
            .any(is_real);

        Ok(has_staged || has_unstaged)
    }

    /// Refuse if any staged or unstaged change touches a file in `commit_paths`.
    fn check_dirty_overlap(&self, commit_paths: &HashSet<String>) -> Result<()> {
        let mut overlapping: Vec<String> = Vec::new();
        // Context lines do not affect the file list this check inspects.
        for synthetic_diff in [
            self.staged_diff(crate::repo::DEFAULT_CONTEXT_LINES)?,
            self.unstaged_diff(crate::repo::DEFAULT_CONTEXT_LINES)?,
        ]
        .into_iter()
        .flatten()
        {
            for file in &synthetic_diff.files {
                let path = file
                    .new_path
                    .as_deref()
                    .or(file.old_path.as_deref())
                    .unwrap_or("");
                if commit_paths.contains(path) && !overlapping.contains(&path.to_string()) {
                    overlapping.push(path.to_string());
                }
            }
        }
        if !overlapping.is_empty() {
            overlapping.sort();
            anyhow::bail!(
                "Cannot split: staged/unstaged changes overlap with: {}",
                overlapping.join(", ")
            );
        }
        Ok(())
    }

    /// Fast-forward the branch ref that HEAD currently points to.
    fn advance_branch_ref(&self, new_tip: git2::Oid, log_msg: &str) -> Result<()> {
        let repo = &self.inner;
        let head_ref = repo.head()?;
        let branch_refname = head_ref
            .resolve()
            .context("HEAD is not a symbolic ref")?
            .name()
            .context("Ref has no name")?
            .to_string();
        repo.reference(&branch_refname, new_tip, true, log_msg)?;
        Ok(())
    }

    /// Reset the working tree to `reset.worktree_tree` and the index to
    /// `reset.index_tree`, deleting the paths `reset.from_tree` has that the
    /// target working tree does not.
    ///
    /// A force checkout alone leaves those paths behind: once the index holds
    /// the target tree, anything absent from it counts as untracked and is
    /// skipped. Deleting exactly the paths the target drops is what keeps the
    /// user's *own* untracked files, where `remove_untracked` would take them
    /// too.
    ///
    /// Going through the index rather than `checkout_head` also sidesteps a
    /// stale on-disk index: the cherry-pick chain builds its trees in memory
    /// (`apply_to_tree`, `merge_trees`), so the repository's singleton index may
    /// not describe the result yet, and libgit2 compares against whatever is on
    /// disk — leaving every file as a staged deletion with the real files
    /// untracked.
    pub(super) fn reset_worktree(&self, reset: WorktreeReset) -> Result<()> {
        let from = self
            .inner
            .find_tree(reset.from_tree)
            .context("failed to find the current tree")?;
        let to = self
            .inner
            .find_tree(reset.worktree_tree)
            .context("failed to find the target working tree")?;
        self.remove_dropped_files(&from, &to)?;

        self.set_index_tree(reset.worktree_tree)?;
        let mut checkout = git2::build::CheckoutBuilder::new();
        checkout.force();
        self.inner
            .checkout_index(None, Some(&mut checkout))
            .context("failed to restore the working tree")?;
        if reset.index_tree != reset.worktree_tree {
            self.set_index_tree(reset.index_tree)?;
        }
        Ok(())
    }

    /// Delete working-tree files present in `from` but absent from `to`.
    fn remove_dropped_files(&self, from: &git2::Tree, to: &git2::Tree) -> Result<()> {
        let Some(workdir) = self.inner.workdir() else {
            return Ok(());
        };
        let diff = self
            .inner
            .diff_tree_to_tree(Some(from), Some(to), None)
            .context("failed to diff for dropped files")?;
        for delta in diff.deltas() {
            if delta.status() == git2::Delta::Deleted
                && let Some(path) = delta.old_file().path()
            {
                let full = workdir.join(path);
                // Only ever a file: a submodule's directory is not this
                // operation's to delete, and neither is anything else that grew
                // into one.
                if full.is_file() {
                    std::fs::remove_file(&full).with_context(|| {
                        format!("failed to remove dropped file {}", full.display())
                    })?;
                }
            }
        }
        Ok(())
    }

    /// Re-examine the working tree so the index's cached stats describe what is
    /// actually on disk.
    ///
    /// libgit2 decides which tracked files are dirty from each index entry's
    /// cached stat — size and mtime — so a same-size edit whose mtime collides
    /// with that cache (an edit made within the filesystem's mtime tick of the
    /// last index write) reads as unchanged. Anything that then serialises the
    /// working tree, whether into a stash or into a tree object, silently uses
    /// the stale blob and the edit is lost.
    pub(super) fn refresh_index_stat_cache(&self) -> Result<()> {
        let mut opts = git2::StatusOptions::new();
        opts.include_untracked(true).update_index(true);
        self.inner
            .statuses(Some(&mut opts))
            .context("failed to refresh the index")?;
        Ok(())
    }

    /// Point the on-disk index at `tree`, clearing any conflict stages.
    pub(super) fn set_index_tree(&self, tree: git2::Oid) -> Result<()> {
        let tree = self
            .inner
            .find_tree(tree)
            .context("failed to find index tree")?;
        let mut index = self.inner.index().context("failed to open index")?;
        index.read_tree(&tree).context("failed to set index tree")?;
        index.write().context("failed to write index")?;
        Ok(())
    }

    /// Reset the working tree and index to match HEAD, removing files that the
    /// just-completed operation dropped.
    ///
    /// `prev_tip` is the branch tip the working tree currently reflects, before
    /// this operation advanced the ref.
    fn checkout_head(&self, prev_tip: &Oid) -> Result<()> {
        let new_tree = self.inner.head()?.peel_to_commit()?.tree()?.id();
        let prev_tree = self
            .inner
            .find_commit(git2::Oid::from(prev_tip))?
            .tree()?
            .id();
        self.reset_worktree(WorktreeReset {
            from_tree: prev_tree,
            worktree_tree: new_tree,
            index_tree: new_tree,
        })
    }

    /// The empty (no-entries) git tree — the three-way-merge base for building
    /// an orphan root (a commit with no parent to diff against).
    fn empty_tree(&self) -> Result<git2::Tree<'_>> {
        let oid = self.inner.treebuilder(None)?.write()?;
        Ok(self.inner.find_tree(oid)?)
    }

    /// Walk the entire ancestry of `head` and return the commit OIDs oldest-first
    /// (`[root, …, head]`).
    fn all_commit_oids_oldest_first(&self, head: git2::Oid) -> Result<Vec<git2::Oid>> {
        let mut revwalk = self.inner.revwalk()?;
        revwalk.push(head)?;
        let mut oids: Vec<git2::Oid> = revwalk.collect::<Result<Vec<_>, git2::Error>>()?;
        oids.reverse();
        Ok(oids)
    }
}
