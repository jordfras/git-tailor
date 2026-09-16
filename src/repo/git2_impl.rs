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
use std::path::{Path, PathBuf};

use crate::domain::bytes_to_path;
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

    /// Path to the repository's git directory (the `.git` dir for a normal repo,
    /// `.git/worktrees/<name>` for a linked working tree).
    pub fn git_dir(&self) -> &std::path::Path {
        self.inner.path()
    }

    /// Persist or clear the crash-safety journal based on a rebase operation's
    /// outcome: record the conflict state on `Conflict` (so an interrupted
    /// resolution can be recovered), clear it on `Complete` and push an undo
    /// entry from `tip_before` to the resulting tip. Errors are passed through
    /// untouched.
    fn journaled(
        &mut self,
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
        &mut self,
        label: &str,
        snapshot: &super::LiftedRow,
    ) -> Result<Option<super::ConflictState>> {
        let tip_after = reads::head_oid(self)?;
        let index_tree_after = match lift_op::finish(self, snapshot)? {
            lift_op::Settled::Done(index_tree) => index_tree,
            lift_op::Settled::Clash(files) => {
                let state = super::ConflictState {
                    operation_label: label.to_string(),
                    // The lift is what an abort rewinds to, which unwinds the
                    // whole fold — the rewrite included.
                    original_branch_oid: snapshot.temp_oid.clone(),
                    new_tip_oid: tip_after.clone(),
                    conflicting_commit_oid: tip_after,
                    conflicting_files: files,
                    still_unresolved: false,
                    resume: super::Resume::CarryRow(snapshot.clone()),
                    autofixup_context: None,
                    branch_refname: self.current_branch_refname().unwrap_or_default(),
                };
                // The reapply already wrote the markers, so this records why
                // they are there rather than getting ahead of them.
                journal::set_in_progress(
                    self,
                    &super::InProgress::Conflict(Box::new(state.clone())),
                )?;
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
    fn record_unit_undo(
        &mut self,
        label: &str,
        tip_before: &Oid,
        result: Result<()>,
    ) -> Result<()> {
        result?;
        self.record_undo_if_changed(label, tip_before)
    }

    /// Push an undo entry from `tip_before` to the current HEAD, unless the
    /// branch did not actually move.
    fn record_undo_if_changed(&mut self, label: &str, tip_before: &Oid) -> Result<()> {
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
        &mut self,
        label: &str,
        op: impl FnOnce(&mut Self) -> Result<()>,
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

    pub(super) fn stage_file(&mut self, path: &Path) -> Result<()> {
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
                .add_path(path)
                .with_context(|| format!("failed to stage '{}'", path.display()))?;
        } else {
            // File was deleted — remove all index entries for this path
            // (stages 0, 1, 2, 3) so the deletion is staged and no phantom
            // conflict entries remain.
            index
                .remove_path(path)
                .with_context(|| format!("failed to remove '{}' from index", path.display()))?;
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

    fn commit_message_bytes(&self, commit_oid: &Oid) -> Result<Vec<u8>> {
        Ok(self
            .inner
            .find_commit(git2::Oid::from(commit_oid))
            .context("failed to read the commit")?
            .message_bytes()
            .to_vec())
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

    fn read_index_stage(&self, path: &Path, stage: i32) -> Result<Option<Vec<u8>>> {
        reads::read_index_stage(self, path, stage)
    }

    fn read_conflicting_files(&self) -> Vec<PathBuf> {
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

    fn pending_undo_skips_autostash(&self) -> Result<bool> {
        journal::pending_undo_skips_autostash(self)
    }

    fn pending_redo_skips_autostash(&self) -> Result<bool> {
        journal::pending_redo_skips_autostash(self)
    }
}

// Every entry point below checks `refuse_if_branch_moved(head_oid)` as its
// first statement, before calling into the op module that does the rewrite.
// Keeping the check here rather than inside each op module means a new
// operation is written right next to the ones it is modeled on, where the
// check is the first line any of them would be copied from.
impl RepoWrite for Git2Repo {
    fn split_commit_per_file(&mut self, commit_oid: &Oid, head_oid: &Oid) -> Result<()> {
        self.refuse_if_branch_moved(head_oid)?;
        let outcome = split_op::split_commit_per_file(self, commit_oid, head_oid);
        self.record_unit_undo("Split", head_oid, outcome)
    }

    fn split_commit_per_hunk(&mut self, commit_oid: &Oid, head_oid: &Oid) -> Result<()> {
        self.refuse_if_branch_moved(head_oid)?;
        let outcome = split_op::split_commit_per_hunk(self, commit_oid, head_oid);
        self.record_unit_undo("Split", head_oid, outcome)
    }

    fn split_commit_per_hunk_group(
        &mut self,
        commit_oid: &Oid,
        head_oid: &Oid,
        reference_oid: &Oid,
    ) -> Result<()> {
        self.refuse_if_branch_moved(head_oid)?;
        let outcome =
            split_op::split_commit_per_hunk_group(self, commit_oid, head_oid, reference_oid);
        self.record_unit_undo("Split", head_oid, outcome)
    }

    fn split_commit_out_files(
        &mut self,
        commit_oid: &Oid,
        file_paths: &[String],
        head_oid: &Oid,
    ) -> Result<()> {
        self.refuse_if_branch_moved(head_oid)?;
        let outcome = split_op::split_commit_out_files(self, commit_oid, file_paths, head_oid);
        self.record_unit_undo("Split", head_oid, outcome)
    }

    fn split_commit_out_hunks(
        &mut self,
        commit_oid: &Oid,
        hunks: &[(usize, usize)],
        head_oid: &Oid,
        context_lines: u32,
    ) -> Result<()> {
        self.refuse_if_branch_moved(head_oid)?;
        let outcome =
            split_op::split_commit_out_hunks(self, commit_oid, hunks, head_oid, context_lines);
        self.record_unit_undo("Split", head_oid, outcome)
    }

    fn reword_commit(
        &mut self,
        commit_oid: &Oid,
        new_message: &[u8],
        head_oid: &Oid,
    ) -> Result<()> {
        self.refuse_if_branch_moved(head_oid)?;
        let outcome = reword_op::reword_commit(self, commit_oid, new_message, head_oid);
        self.record_unit_undo("Reword", head_oid, outcome)
    }

    fn drop_commit(&mut self, commit_oid: &Oid, head_oid: &Oid) -> Result<super::RebaseOutcome> {
        self.refuse_if_branch_moved(head_oid)?;
        let outcome = drop_op::drop_commit(self, commit_oid, head_oid);
        self.journaled("Drop", head_oid, outcome)
    }

    fn begin_edit(&mut self, commit_oid: &Oid, head_oid: &Oid) -> Result<()> {
        self.refuse_if_branch_moved(head_oid)?;
        edit_op::begin_edit(self, commit_oid, head_oid)
    }

    fn finish_edit(&mut self, commit_oid: &Oid) -> Result<super::EditOutcome> {
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

    fn abort_edit(&mut self) -> Result<()> {
        edit_op::abort_edit(self)
    }

    fn rebase_continue(&mut self, state: &super::ConflictState) -> Result<super::RebaseOutcome> {
        // A paused conflict left a particular branch on a particular tip.
        // Either having changed means resuming would rewrite something nobody
        // asked it to.
        self.refuse_if_conflict_branch_moved(state)?;
        // A carry conflict is not a rebase step: the history it belongs to is
        // already written, and what is left settles the working tree and records
        // the fold's undo entry itself, so it does not go through `journaled`.
        if let super::Resume::CarryRow(lifted) = &state.resume {
            return lift_op::continue_carry(self, lifted, state);
        }
        if state.autofixup_context.is_some() {
            let outcome = autofixup_op::continue_autofixup(self, state);
            return self.journaled("Autofixup", &state.original_branch_oid, outcome);
        }
        let outcome = conflict::rebase_continue(self, state);
        self.journaled(&state.operation_label, &state.original_branch_oid, outcome)
    }

    fn rebase_abort(&mut self, state: &super::ConflictState) -> Result<()> {
        // Same as resuming: an abort writes the rewind to a branch, and it must
        // be the branch the conflict is on, still where it was left.
        self.refuse_if_conflict_branch_moved(state)?;
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

    fn read_journal(&mut self) -> Result<super::JournalStatus> {
        Ok(journal::read(self))
    }

    fn clear_journal(&mut self) -> Result<()> {
        journal::discard_in_flight(self)
    }

    fn prune_stale_journal(&mut self) -> Result<()> {
        journal::prune_stale(self)
    }

    fn clean_journal(&mut self) -> Result<super::JournalCleanSummary> {
        journal::clean(self)
    }

    fn undo(&mut self) -> Result<super::UndoOutcome> {
        journal::apply_undo(self)
    }

    fn redo(&mut self) -> Result<super::UndoOutcome> {
        journal::apply_redo(self)
    }

    fn stage_all(&mut self) -> Result<super::StageOutcome> {
        self.journaled_index_op("Stage all", stage_op::stage_all)
    }

    fn unstage_all(&mut self) -> Result<super::StageOutcome> {
        self.journaled_index_op("Unstage all", stage_op::unstage_all)
    }

    fn commit_staged(&mut self, message: &[u8]) -> Result<super::CommitOutcome> {
        let before = reads::head_oid(self)?;
        match commit_staged_op::commit_staged(self, message)? {
            None => Ok(super::CommitOutcome::NothingStaged),
            Some(after) => {
                journal::record_commit_undo(self, "Commit", &before, &after)?;
                Ok(super::CommitOutcome::Committed)
            }
        }
    }

    fn lift_worktree_row(
        &mut self,
        source: super::WorktreeSource,
    ) -> Result<Option<super::LiftedRow>> {
        lift_op::lift(self, source)
    }

    fn restore_lifted_row(&mut self, lifted: &super::LiftedRow) -> Result<()> {
        lift_op::restore(self, lifted)
    }

    fn recorded_lifted_row(&mut self) -> Result<Option<super::LiftedRow>> {
        journal::worktree_source(self)
    }

    fn rescue_lifted_row(&mut self, lifted: &super::LiftedRow) -> Result<Option<String>> {
        lift_op::rescue(self, lifted)
    }

    fn autostash_save(&mut self) -> Result<()> {
        self.save_autostash()
    }

    fn autostash_restore(&mut self) -> Result<crate::repo::AutostashRestore> {
        // Only a leftover *auto-stash*. A fold's leftover sits in the same slot
        // and is not the same thing: the fold has not finished, and `finish` is
        // what knows where that work belongs. Putting it back here would also
        // hand the stash dialog a `pre_op_tip` that is the fold's temporary
        // commit, and its abort hard-resets to whatever that names.
        if journal::autostash(self)?.is_some_and(|r| r.fold_temp_oid.is_some()) {
            return Ok(super::AutostashRestore::Done);
        }
        self.restore_autostash()
    }

    fn autostash_conflict_continue(&mut self) -> Result<crate::repo::AutostashContinue> {
        self.continue_autostash()
    }

    fn autostash_conflict_abort(&mut self) -> Result<()> {
        self.abort_autostash()
    }

    fn move_commit(
        &mut self,
        commit_oid: &Oid,
        insert_after_oid: Option<&Oid>,
        head_oid: &Oid,
    ) -> Result<super::RebaseOutcome> {
        self.refuse_if_branch_moved(head_oid)?;
        let outcome = move_op::move_commit(self, commit_oid, insert_after_oid, head_oid);
        self.journaled("Move", head_oid, outcome)
    }

    fn squash_commits(
        &mut self,
        source_oid: &Oid,
        target_oid: &Oid,
        message: &[u8],
        head_oid: &Oid,
    ) -> Result<super::RebaseOutcome> {
        self.refuse_if_branch_moved(head_oid)?;
        let outcome = squash_op::squash_commits(self, source_oid, target_oid, message, head_oid);
        self.journaled("Squash", head_oid, outcome)
    }

    fn stage_file(&mut self, path: &Path) -> Result<()> {
        // Qualified: with a `&mut self` receiver the trait method now matches
        // method resolution before the inherent one, so `self.stage_file(..)`
        // would call straight back into here.
        Git2Repo::stage_file(self, path)
    }

    fn auto_stage_resolved_conflicts(&mut self, files: &[PathBuf]) -> Result<()> {
        conflict::auto_stage_resolved_conflicts(self, files)
    }

    fn squash_try_combine(
        &mut self,
        source_oid: &Oid,
        target_oid: &Oid,
        combined_message: &[u8],
        squash_mode: SquashMode,
        head_oid: &Oid,
    ) -> Result<Option<super::ConflictState>> {
        self.refuse_if_branch_moved(head_oid)?;
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
        &mut self,
        ctx: &super::SquashContext,
        message: &[u8],
        original_branch_oid: &Oid,
        autofixup_context: Option<&super::AutofixupContext>,
    ) -> Result<super::RebaseOutcome> {
        // Not handed a ConflictState like rebase_continue/rebase_abort are, so
        // the branch and tip this squash-tree conflict paused on come from the
        // journal's own record of it instead — set by squash_try_combine (or
        // squash_commits, on a descendant conflict) at the moment it paused.
        if let Some(super::InProgress::Conflict(state)) = journal::in_progress(self)? {
            self.refuse_if_conflict_branch_moved(&state)?;
        }
        if let Some(autofixup_ctx) = autofixup_context {
            let outcome = autofixup_op::continue_autofixup_after_squash_finalize(
                self,
                ctx,
                message,
                original_branch_oid,
                autofixup_ctx,
            );
            return self.journaled("Autofixup", original_branch_oid, outcome);
        }
        // The mode's own word, not "Squash" for both: the dialog that sent the
        // user here was built from `ctx.squash_mode`, and a working-tree fold
        // can raise a second dialog from this very call.
        let outcome = squash_op::squash_finalize(self, ctx, message, original_branch_oid);
        self.journaled(ctx.squash_mode.label(), original_branch_oid, outcome)
    }

    fn autofixup(
        &mut self,
        head_oid: &Oid,
        reference_oid: &Oid,
        message_overrides: &std::collections::HashMap<String, String>,
    ) -> Result<super::RebaseOutcome> {
        self.refuse_if_branch_moved(head_oid)?;
        let outcome = autofixup_op::autofixup(self, head_oid, reference_oid, message_overrides);
        self.journaled("Autofixup", head_oid, outcome)
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

/// Delete `path` under `workdir` if it exists and is not a directory.
/// Reports whether it actually removed a file, so a caller that only wants
/// to clean up now-empty parent directories knows whether anything changed.
///
/// Anything but a directory: a submodule's checkout is not this operation's
/// to delete, and neither is anything else that grew into one. Asking
/// `symlink_metadata` rather than `is_file` keeps a symlink in scope — git
/// tracked it as a file, and following it would judge the entry by whatever
/// it points at.
pub(super) fn remove_written_path(workdir: &Path, path: &Path) -> Result<bool> {
    let full = workdir.join(path);
    if full.symlink_metadata().is_ok_and(|meta| !meta.is_dir()) {
        std::fs::remove_file(&full)
            .with_context(|| format!("failed to remove leftover file {}", full.display()))?;
        return Ok(true);
    }
    Ok(false)
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
    fn check_no_dirty_state(&mut self) -> Result<()> {
        // No exemption for a fold in flight: the lift sets the other row's
        // changes aside in the stash, so the working tree it leaves behind is
        // genuinely clean and there is nothing here to excuse.
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
    fn advance_branch_ref(&mut self, new_tip: git2::Oid, log_msg: &str) -> Result<()> {
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
    pub(super) fn reset_worktree(&mut self, reset: WorktreeReset) -> Result<()> {
        // Every route to a working-tree checkout that goes through here is
        // checked, whether or not its caller remembered to. Callers that can
        // still back out cheaply check earlier as well, before moving the ref —
        // by then this one is a formality that passes.
        self.refuse_untracked_collisions(reset.from_tree, reset.worktree_tree)?;
        self.remove_dropped_files(reset.from_tree, reset.worktree_tree)?;

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
    fn remove_dropped_files(&mut self, from: git2::Oid, to: git2::Oid) -> Result<()> {
        let Some(workdir) = self.inner.workdir() else {
            return Ok(());
        };
        let from = self
            .inner
            .find_tree(from)
            .context("failed to find the current tree")?;
        let to = self
            .inner
            .find_tree(to)
            .context("failed to find the target working tree")?;
        let diff = self
            .inner
            .diff_tree_to_tree(Some(&from), Some(&to), None)
            .context("failed to diff for dropped files")?;
        for delta in diff.deltas() {
            if delta.status() == git2::Delta::Deleted
                && let Some(path) = delta.old_file().path()
            {
                remove_written_path(workdir, path)?;
            }
        }
        Ok(())
    }

    /// Move the branch to `new_tip` and bring the working tree with it.
    ///
    /// The only way to do both together, so a new caller cannot forget
    /// [`Self::refuse_untracked_collisions`]. That check runs before the ref
    /// moves, so a refusal leaves the branch, the index and the working tree
    /// exactly as they were rather than half-rewritten with the checkout still
    /// owed.
    ///
    /// `prev_tip` is the tip the working tree currently reflects;
    /// [`Self::checkout_head`] needs it to delete the files the new tip drops.
    pub(super) fn advance_and_checkout(
        &mut self,
        new_tip: git2::Oid,
        prev_tip: &Oid,
        log_msg: &str,
    ) -> Result<()> {
        let from_tree = self.commit_tree_id(git2::Oid::from(prev_tip))?;
        let to_tree = self.commit_tree_id(new_tip)?;
        self.refuse_untracked_collisions(from_tree, to_tree)?;

        self.advance_branch_ref(new_tip, log_msg)?;
        self.checkout_head(prev_tip)
    }

    /// Full name of the branch HEAD is on. Errors when HEAD is detached.
    pub(super) fn current_branch_refname(&self) -> Result<String> {
        Ok(self
            .inner
            .head()?
            .resolve()
            .context("HEAD is not on a branch")?
            .name()
            .context("branch ref has no name")?
            .to_string())
    }

    /// Refuse when HEAD is no longer on the branch a paused operation belongs to.
    ///
    /// Resuming or aborting writes the result to a branch, and it has to be the
    /// one the conflict is on. The tip check alone cannot see this: a different
    /// branch sitting on the same commit passes it and is then rewritten to a
    /// history it never had.
    ///
    /// An empty `expected` means the state predates this being recorded, so
    /// there is nothing to compare and the check stands aside.
    pub(super) fn refuse_if_branch_switched(&self, expected: &str) -> Result<()> {
        let actual = self.current_branch_refname().unwrap_or_default();
        Self::check_branch_refname(&actual, expected)
    }

    /// The comparison [`Self::refuse_if_branch_switched`] makes, split out so
    /// [`Self::refuse_if_conflict_branch_moved`] can reuse it against a branch
    /// name it already resolved, instead of resolving HEAD a second time.
    fn check_branch_refname(actual: &str, expected: &str) -> Result<()> {
        if expected.is_empty() || actual == expected {
            return Ok(());
        }
        anyhow::bail!(
            "This operation belongs to {expected}, but HEAD is on {} now. \
             Switch back before continuing or aborting it.",
            if actual.is_empty() {
                "a detached HEAD"
            } else {
                actual
            }
        )
    }

    /// Refuse when the branch no longer holds what the caller was told it did.
    ///
    /// Every operation is chosen against a commit list read at some earlier
    /// moment, and is handed the tip that list was built from. The session lock
    /// keeps a second git-tailor out; it does nothing about `git commit` in
    /// another terminal, an IDE's git integration, or a script. Without this the
    /// rewrite is computed from a view that is gone and then force-written over
    /// the real one, and a commit made elsewhere simply disappears.
    ///
    /// Comparing the tip also catches HEAD having moved to a *different branch*:
    /// what git-tailor would write to is no longer what it started on. A branch
    /// that happens to sit on the same commit is the one case this lets through,
    /// and writing the same history to it is what the user asked for anyway.
    ///
    /// A gap remains between this check and the write, which a compare-and-swap
    /// on the ref would close. It is not worth the plumbing: the window here is
    /// microseconds, where the one this closes is however long the commit list
    /// has been on screen.
    pub(super) fn refuse_if_branch_moved(&self, expected: &Oid) -> Result<()> {
        let actual = reads::head_oid(self)?;
        Self::check_branch_tip(&actual, expected)
    }

    /// The comparison [`Self::refuse_if_branch_moved`] makes, split out so
    /// [`Self::refuse_if_conflict_branch_moved`] can reuse it against a tip it
    /// already resolved, instead of resolving HEAD a second time.
    fn check_branch_tip(actual: &Oid, expected: &Oid) -> Result<()> {
        if actual == expected {
            return Ok(());
        }
        anyhow::bail!(
            "The branch moved since this was loaded — it is at {} now, not {}. \
             Something else wrote to the repository, or HEAD was switched to \
             another branch. Reload and try again.",
            actual.short(),
            expected.short()
        )
    }

    /// Refuse to resume or abort a paused conflict when the branch it belongs
    /// to is no longer where it was left — switched away from, or moved on by
    /// something else. The two checks always travel together: a tip match on
    /// the wrong branch is coincidence, not permission.
    pub(super) fn refuse_if_conflict_branch_moved(
        &self,
        state: &super::ConflictState,
    ) -> Result<()> {
        // One `head()` resolution feeds both checks, rather than each of
        // refuse_if_branch_switched/refuse_if_branch_moved resolving it again.
        let head = self.inner.head();
        let actual_refname: String = match &head {
            Ok(h) => match h.resolve() {
                Ok(resolved) => resolved.name().unwrap_or_default().to_string(),
                Err(_) => String::new(),
            },
            Err(_) => String::new(),
        };
        Self::check_branch_refname(&actual_refname, &state.branch_refname)?;

        let actual_oid = head
            .context("Failed to get HEAD")?
            .target()
            .context("HEAD is not a direct reference")?;
        Self::check_branch_tip(&Oid::from(actual_oid), &state.new_tip_oid)
    }

    /// Refuse to treat `commit` as a root when it is only one by accident of a
    /// shallow fetch.
    ///
    /// `git clone --depth` grafts the history: the oldest fetched commit reports
    /// no parents while upstream it has plenty. Every "is this the root?" test
    /// in the rewrite engine asks `parent_count() == 0`, so in a shallow clone
    /// they all get the wrong answer and build a genuinely parentless commit —
    /// severing the branch from everything behind the graft. Undoable locally;
    /// pushed, it truncates the history everyone shares.
    ///
    /// Only the boundary is refused. Commits above it are ordinary, and a
    /// shallow clone is a normal way to work on a large repository.
    pub(super) fn refuse_shallow_root(&self, commit: git2::Oid) -> Result<()> {
        if !self.inner.is_shallow() {
            return Ok(());
        }
        anyhow::bail!(
            "Cannot rewrite {} as a root commit: this is a shallow clone, so it \
             only looks like the root — upstream it has history behind it that \
             was never fetched. Rewriting it here would cut the branch off from \
             that history. Run `git fetch --unshallow` first.",
            Oid::from(commit).short()
        )
    }

    /// Create a commit whose message is written **byte for byte**.
    ///
    /// git2 cannot: `Repository::commit` and `commit_create_buffer` both take
    /// `&str`, and `Commit::message` hands back an error rather than bytes when
    /// a message is not UTF-8. Reaching for `unwrap_or("")` at the call site
    /// turns "I cannot read this" into "it says nothing", and a rewrite then
    /// writes that back as the truth.
    ///
    /// So: let libgit2 build the object with an empty message — it knows how to
    /// format signatures, order parents and canonicalise the rest — then put the
    /// real bytes where the empty message was. The header block ends at the
    /// first blank line, which is also where `encoding` belongs if the original
    /// carried one. Nothing else is hand-serialized.
    ///
    /// Extra headers of the original, a `gpgsig` above all, are deliberately not
    /// carried over: the content is changing, so a signature over the old
    /// content would be a lie. `git rebase` drops them the same way.
    pub(super) fn commit_preserving_message(
        &self,
        author: &git2::Signature<'_>,
        committer: &git2::Signature<'_>,
        message: &[u8],
        encoding: Option<&str>,
        tree: &git2::Tree<'_>,
        parents: &[&git2::Commit<'_>],
    ) -> Result<git2::Oid> {
        let buffer = self
            .inner
            .commit_create_buffer(author, committer, "", tree, parents)
            .context("failed to build the commit object")?;

        // The header block runs up to the first blank line.
        let split = buffer
            .windows(2)
            .position(|pair| pair == b"\n\n")
            .map(|i| i + 1)
            .ok_or_else(|| anyhow::anyhow!("commit object has no header terminator"))?;

        let mut raw = Vec::with_capacity(buffer.len() + message.len() + 32);
        raw.extend_from_slice(&buffer[..split]);
        if let Some(encoding) = encoding {
            raw.extend_from_slice(format!("encoding {encoding}\n").as_bytes());
        }
        raw.extend_from_slice(b"\n");
        raw.extend_from_slice(message);

        self.inner
            .odb()
            .context("failed to open the object database")?
            .write(git2::ObjectType::Commit, &raw)
            .context("failed to write the commit object")
    }

    /// Tree of `commit`.
    fn commit_tree_id(&self, commit: git2::Oid) -> Result<git2::Oid> {
        Ok(self
            .inner
            .find_commit(commit)
            .with_context(|| format!("failed to read commit {commit}"))?
            .tree_id())
    }

    /// Refuse the operation when checking out `to_tree` would overwrite an
    /// untracked file.
    ///
    /// A force checkout writes the target tree over whatever is on disk, so a
    /// path the operation *reintroduces* — dropping the commit that deleted it,
    /// say — lands on top of an untracked file of the same name and takes its
    /// contents with it. Those contents were never in git, so nothing can get
    /// them back: not undo, not the reflog, not `git stash list`.
    ///
    /// [`Self::check_no_dirty_state`] cannot catch this. It asks
    /// [`Self::is_worktree_dirty`], which diffs HEAD against the index and the
    /// index against the working tree — neither of which sees an untracked file.
    /// A working tree holding nothing else reads as perfectly clean.
    ///
    /// Called *before* the branch ref moves, so a refusal leaves the repository
    /// exactly as it was rather than half-rewritten.
    ///
    /// One known gap, on a case-insensitive filesystem: the index is consulted
    /// case-sensitively and the disk is not, so an untracked `NOTES.txt` where
    /// `notes.txt` returns reads as a collision on macOS and Windows and not on
    /// Linux. The refusal is the conservative half of that difference, so the
    /// gap costs a puzzling message rather than a file.
    pub(super) fn refuse_untracked_collisions(
        &self,
        from_tree: git2::Oid,
        to_tree: git2::Oid,
    ) -> Result<()> {
        Self::refuse(self.untracked_collisions(from_tree, to_tree)?)
    }

    /// Turn a collision list into the refusal the user sees.
    fn refuse(files: Vec<String>) -> Result<()> {
        if !files.is_empty() {
            anyhow::bail!(
                "This would overwrite untracked files: {}. \
                 Move, delete, or commit them first.",
                files.join(", ")
            );
        }
        Ok(())
    }

    /// Untracked working-tree files that checking out `to_tree` would overwrite.
    ///
    /// Only paths the target *adds* can collide — anything already tracked is
    /// the dirty guard's business, not this one's. A file whose contents already
    /// match the incoming blob is left out: overwriting it changes nothing.
    fn untracked_collisions(
        &self,
        from_tree: git2::Oid,
        to_tree: git2::Oid,
    ) -> Result<Vec<String>> {
        let from = self
            .inner
            .find_tree(from_tree)
            .context("failed to find the current tree")?;
        let to = self
            .inner
            .find_tree(to_tree)
            .context("failed to find the target tree")?;
        let diff = self
            .inner
            .diff_tree_to_tree(Some(&from), Some(&to), None)
            .context("failed to diff for untracked collisions")?;

        // Only what the target *adds* can land on an untracked file; anything
        // already tracked is the dirty guard's business.
        let incoming: Vec<(PathBuf, git2::Oid)> = diff
            .deltas()
            .filter(|delta| delta.status() == git2::Delta::Added)
            .filter_map(|delta| {
                delta
                    .new_file()
                    .path()
                    .map(|path| (path.to_path_buf(), delta.new_file().id()))
            })
            .collect();
        self.collisions_among(incoming)
    }

    /// Untracked working-tree content that checking `incoming` out would
    /// overwrite.
    ///
    /// The index form of [`Self::untracked_collisions`], for the conflict
    /// writes: a half-finished merge is an index, not a tree, so there is no
    /// target tree to diff against. Every path the index names is a candidate;
    /// the ones already in the current index drop out immediately, which leaves
    /// the handful the operation is reintroducing.
    pub(super) fn refuse_index_collisions(&self, incoming: &git2::Index) -> Result<()> {
        // An index entry's path is raw bytes; decoding with `String::from_utf8`
        // and dropping the failures would quietly exempt those paths from this
        // guard, which is the one place a missed path costs a file.
        let candidates: Vec<(PathBuf, git2::Oid)> = incoming
            .iter()
            .map(|entry| (bytes_to_path(&entry.path), entry.id))
            .collect();
        Self::refuse(self.collisions_among(candidates)?)
    }

    /// Refuse if checking `tree` out over the working tree would overwrite
    /// untracked work.
    ///
    /// The abort form. There is no "before" tree to diff against: an abort puts
    /// a whole tree back over a working tree holding a half-finished merge,
    /// which is not a tree at all. So every path the target names is a
    /// candidate, and the current index sorts them out.
    pub(super) fn refuse_tree_collisions(&self, tree: git2::Oid) -> Result<()> {
        let tree = self
            .inner
            .find_tree(tree)
            .context("failed to find the target tree")?;
        let mut incoming = git2::Index::new().context("failed to build a scratch index")?;
        incoming
            .read_tree(&tree)
            .context("failed to read the target tree")?;
        self.refuse_index_collisions(&incoming)
    }

    /// The shared body: of the paths a checkout is about to write, which ones
    /// have untracked work of the user's sitting at them.
    fn collisions_among(&self, incoming: Vec<(PathBuf, git2::Oid)>) -> Result<Vec<String>> {
        let Some(workdir) = self.inner.workdir() else {
            return Ok(Vec::new());
        };
        let mut index = self.inner.index().context("failed to open index")?;
        // Reloaded if it changed on disk: the user resolves conflicts and stages
        // with their own tools between our calls, and judging what is untracked
        // from a stale cache is how a tracked file gets mistaken for theirs — or
        // theirs for a tracked file.
        index.read(false).context("failed to refresh index")?;
        let mut collisions = Vec::new();
        for (path, blob) in incoming {
            let path = path.as_path();
            // Every stage, not just 0: mid-conflict the index holds the path at
            // stages 1-3 and nothing at 0, and a file git is in the middle of
            // merging is emphatically not untracked.
            if (0..=3).any(|stage| index.get_path(path, stage).is_some()) {
                continue;
            }
            let full = workdir.join(path);
            match full.symlink_metadata() {
                // A directory where a file returns: the checkout replaces the
                // whole thing. An empty one is nothing to lose; anything inside
                // is the user's. A real submodule never reaches this — its
                // gitlink is in the index, and the check above took it.
                Ok(meta) if meta.is_dir() => {
                    if std::fs::read_dir(&full).is_ok_and(|mut d| d.next().is_some()) {
                        collisions.push(path.display().to_string());
                    }
                }
                // A file or a symlink. A symlink counts deliberately: git
                // tracked the path as a file, and following the link would
                // judge it by whatever it points at.
                //
                // Identical content is not a collision — the checkout is a
                // no-op. Hashed without filters, so a checkout-filtered file
                // can read as different and be reported. Erring that way is the
                // safe one: the user is asked about a file, not silently
                // relieved of it.
                Ok(_) => {
                    if !git2::Oid::hash_file(git2::ObjectType::Blob, &full)
                        .is_ok_and(|oid| oid == blob)
                    {
                        collisions.push(path.display().to_string());
                    }
                }
                // Nothing there — the ordinary case, unless a *parent* of the
                // path is occupied by a file, which is why nothing can be there.
                // The checkout has to remove that file to make the directory.
                Err(_) => {
                    if let Some(blocked) = Self::blocking_ancestor(&index, workdir, path) {
                        collisions.push(blocked);
                    }
                }
            }
        }
        collisions.sort();
        collisions.dedup();
        Ok(collisions)
    }

    /// The untracked file standing where `path` needs a directory, if any.
    ///
    /// Reported instead of `path` itself: that is the file the user has to move
    /// out of the way, and the path git wants means nothing to them.
    fn blocking_ancestor(
        index: &git2::Index,
        workdir: &std::path::Path,
        path: &std::path::Path,
    ) -> Option<String> {
        let mut ancestor = path.parent()?;
        while !ancestor.as_os_str().is_empty() {
            if index.get_path(ancestor, 0).is_none()
                && workdir
                    .join(ancestor)
                    .symlink_metadata()
                    .is_ok_and(|meta| !meta.is_dir())
            {
                return Some(ancestor.display().to_string());
            }
            ancestor = ancestor.parent()?;
        }
        None
    }

    /// Re-examine the working tree so the index's cached stats describe what is
    /// actually on disk.
    ///
    /// libgit2 decides which tracked files are dirty from each index entry's
    /// cached stat — size and mtime — so a same-size edit whose mtime collides
    /// with that cache (an edit made within the filesystem's mtime tick of the
    /// last index write) reads as unchanged. Anything that then serializes the
    /// working tree, whether into a stash or into a tree object, silently uses
    /// the stale blob and the edit is lost.
    pub(super) fn refresh_index_stat_cache(&mut self) -> Result<()> {
        let mut opts = git2::StatusOptions::new();
        opts.include_untracked(true).update_index(true);
        self.inner
            .statuses(Some(&mut opts))
            .context("failed to refresh the index")?;
        Ok(())
    }

    /// Point the on-disk index at `tree`, clearing any conflict stages.
    pub(super) fn set_index_tree(&mut self, tree: git2::Oid) -> Result<()> {
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
    fn checkout_head(&mut self, prev_tip: &Oid) -> Result<()> {
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
