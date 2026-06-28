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

use super::GitRepo;

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

mod cherry_pick;
mod commit_staged_op;
mod conflict;
mod drop_op;
mod hunks;
mod journal;
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
    /// state instead of refusing (see [`GitRepo::autostash_save`]).
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
                super::RebaseOutcome::Conflict(state) => journal::set_in_progress(self, state)?,
                super::RebaseOutcome::Complete => {
                    journal::clear_in_progress(self)?;
                    self.record_undo_if_changed(label, tip_before)?;
                }
            }
        }
        outcome
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

impl GitRepo for Git2Repo {
    fn head_oid(&self) -> Result<Oid> {
        reads::head_oid(self)
    }

    fn find_reference_point(&self, commit_ish: &str) -> Result<Oid> {
        reads::find_reference_point(self, commit_ish)
    }

    fn list_commits(&self, from_oid: &Oid, to_oid: &Oid) -> Result<Vec<CommitInfo>> {
        reads::list_commits(self, from_oid, to_oid)
    }

    fn commit_diff(&self, oid: &Oid) -> Result<CommitDiff> {
        reads::commit_diff(self, oid)
    }

    fn commit_diff_for_fragmap(&self, oid: &Oid) -> Result<CommitDiff> {
        reads::commit_diff_for_fragmap(self, oid)
    }

    fn staged_diff(&self) -> Result<Option<CommitDiff>> {
        reads::staged_diff(self)
    }

    fn staged_diff_for_fragmap(&self) -> Result<Option<CommitDiff>> {
        reads::staged_diff_for_fragmap(self)
    }

    fn unstaged_diff(&self) -> Result<Option<CommitDiff>> {
        reads::unstaged_diff(self)
    }

    fn unstaged_diff_for_fragmap(&self) -> Result<Option<CommitDiff>> {
        reads::unstaged_diff_for_fragmap(self)
    }

    fn list_commit_files(&self, commit_oid: &Oid) -> Result<Vec<String>> {
        reads::list_commit_files(self, commit_oid)
    }

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

    fn split_commit_out_file(
        &self,
        commit_oid: &Oid,
        file_path: &str,
        head_oid: &Oid,
    ) -> Result<()> {
        self.record_unit_undo(
            "Split",
            head_oid,
            split_op::split_commit_out_file(self, commit_oid, file_path, head_oid),
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

    fn get_config_string(&self, key: &str) -> Result<Option<String>> {
        reads::get_config_string(self, key)
    }

    fn drop_commit(&self, commit_oid: &Oid, head_oid: &Oid) -> Result<super::RebaseOutcome> {
        self.journaled(
            "Drop",
            head_oid,
            drop_op::drop_commit(self, commit_oid, head_oid),
        )
    }

    fn rebase_continue(&self, state: &super::ConflictState) -> Result<super::RebaseOutcome> {
        self.journaled(
            &state.operation_label,
            &state.original_branch_oid,
            conflict::rebase_continue(self, state),
        )
    }

    fn rebase_abort(&self, state: &super::ConflictState) -> Result<()> {
        conflict::rebase_abort(self, state)?;
        journal::clear_in_progress(self)
    }

    fn read_journal(&self) -> Result<super::JournalStatus> {
        Ok(journal::read(self))
    }

    fn clear_journal(&self) -> Result<()> {
        journal::clear_in_progress(self)
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

    fn workdir(&self) -> Option<std::path::PathBuf> {
        reads::workdir(self)
    }

    fn read_index_stage(&self, path: &str, stage: i32) -> Result<Option<Vec<u8>>> {
        reads::read_index_stage(self, path, stage)
    }

    fn read_conflicting_files(&self) -> Vec<String> {
        conflict::read_conflicting_files(self)
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

    fn default_branch(&self) -> Result<Option<String>> {
        reads::default_branch(self)
    }

    fn root_commit_oid(&self) -> Result<Oid> {
        reads::root_commit_oid(self)
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
            journal::set_in_progress(self, state)?;
        }
        Ok(result)
    }

    fn squash_finalize(
        &self,
        ctx: &super::SquashContext,
        message: &str,
        original_branch_oid: &Oid,
    ) -> Result<super::RebaseOutcome> {
        self.journaled(
            "Squash",
            original_branch_oid,
            squash_op::squash_finalize(self, ctx, message, original_branch_oid),
        )
    }

    fn commit_walker<'a>(
        &'a self,
        from_oid: &Oid,
        to_oid: &Oid,
    ) -> Result<Box<dyn Iterator<Item = Result<CommitInfo>> + 'a>> {
        reads::commit_walker(self, from_oid, to_oid)
    }
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
        for synthetic_diff in [self.staged_diff()?, self.unstaged_diff()?]
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

    /// Reset the working tree and index to match HEAD, removing files that the
    /// just-completed operation dropped.
    ///
    /// `prev_tip` is the branch tip the working tree currently reflects, before
    /// this operation advanced the ref. Files present in `prev_tip` but absent
    /// from the new HEAD must be deleted from the working tree. A force checkout
    /// alone won't do it: we reset the index to HEAD's tree below, which turns
    /// those files into untracked leftovers that checkout leaves untouched. We
    /// delete exactly those files, so the user's own untracked files survive.
    fn checkout_head(&self, prev_tip: &Oid) -> Result<()> {
        let repo = &self.inner;
        let new_tree = repo.head()?.peel_to_commit()?.tree()?;

        let prev_tree = repo.find_commit(git2::Oid::from(prev_tip))?.tree()?;
        let diff = repo.diff_tree_to_tree(Some(&prev_tree), Some(&new_tree), None)?;
        if let Some(workdir) = repo.workdir() {
            for delta in diff.deltas() {
                if delta.status() == git2::Delta::Deleted
                    && let Some(path) = delta.old_file().path()
                {
                    let full = workdir.join(path);
                    if full.exists() {
                        std::fs::remove_file(&full).with_context(|| {
                            format!("failed to remove dropped file {}", full.display())
                        })?;
                    }
                }
            }
        }

        // Explicitly reset the index to HEAD's tree before forcing a workdir
        // checkout. cherry_pick_chain uses in-memory index operations
        // (apply_to_tree, merge_trees) whose returned indexes are distinct from
        // the repo's singleton index, but libgit2's checkout_head re-reads the
        // on-disk index to compare against HEAD. If the on-disk index is stale
        // (not yet written after the chain completed), checkout can silently
        // leave the index empty — all files appear as staged deletions with the
        // actual files untracked. Writing HEAD's tree into the index first
        // guarantees a clean baseline regardless of prior index state.
        let mut index = repo.index()?;
        index.read_tree(&new_tree)?;
        index.write()?;

        let mut checkout = git2::build::CheckoutBuilder::new();
        checkout.force();
        repo.checkout_head(Some(&mut checkout))?;
        Ok(())
    }
}
