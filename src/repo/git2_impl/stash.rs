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

//! Auto-stash: stash dirty working-tree state around operations that need a
//! clean tree, then restore the exact staged/unstaged split afterwards.
//!
//! The stash and the pre-operation tip are recorded in the journal so the
//! restore can happen when the operation truly completes — even across a
//! conflict pause or a crash/restart — and so a conflicting reapply can be
//! aborted all the way back to the pre-operation state.

use anyhow::{Context, Result};
use git2::{Signature, StashApplyOptions, StashFlags};

use super::super::{AutostashContinue, AutostashRestore};
use super::Git2Repo;
use super::conflict;
use super::journal;
use super::journal::AutostashRecord;
use super::reads;
use crate::Oid;

impl Git2Repo {
    /// Stash dirty working-tree state (staged + unstaged + untracked) when
    /// auto-stash is enabled and the tree is dirty, recording the stash and the
    /// current branch tip in the journal.
    ///
    /// Idempotent: if a stash is already recorded for the in-flight operation
    /// (e.g. a multi-step squash), this is a no-op so the dirty state is stashed
    /// exactly once.
    pub(super) fn save_autostash(&mut self) -> Result<()> {
        if !self.autostash {
            return Ok(());
        }
        if journal::autostash(self)?.is_some() {
            return Ok(());
        }
        if !self.is_worktree_dirty()? {
            return Ok(());
        }

        // Without this, `stash_save2` would serialize the stale blob for a
        // same-size edit and lose it.
        self.refresh_index_stat_cache()?;

        // Captured before the operation advances the ref, so an aborted reapply
        // can rewind here — where the stash, whose base this tip is, re-applies
        // cleanly.
        let pre_op_tip = reads::head_oid(self)?;

        let sig = self
            .inner
            .signature()
            .or_else(|_| Signature::now("git-tailor", "git-tailor@localhost"))?;
        let oid = self.inner.stash_save2(
            &sig,
            Some("git-tailor: autostash"),
            Some(StashFlags::INCLUDE_UNTRACKED),
        )?;

        // The stash reset the working tree and index; refresh the cached index
        // so subsequent reads on this handle see the clean state.
        self.inner.index()?.read(true)?;
        journal::set_autostash(
            self,
            Some(AutostashRecord {
                stash: Oid::from(oid),
                pre_op_tip,
                applied_with_conflict: false,
            }),
        )
    }

    /// Reapply and drop the recorded auto-stash, restoring the staged/unstaged
    /// split. Returns [`AutostashRestore::Done`] when nothing is recorded or it
    /// reapplies cleanly.
    ///
    /// `libgit2`'s `stash_apply` does not report a content conflict as an error
    /// — it returns `Ok` after writing conflict markers into the working tree and
    /// leaving unmerged entries in the index — so the conflict is detected
    /// explicitly via the index. On conflict the stash is **kept** and the
    /// journal record is flagged `applied_with_conflict`, so the user's changes
    /// are never lost and startup recovery does not reapply it a second time.
    pub(super) fn restore_autostash(&mut self) -> Result<AutostashRestore> {
        let Some(record) = journal::autostash(self)? else {
            return Ok(AutostashRestore::Done);
        };

        // Already reapplied with conflicts in an earlier run (or earlier this
        // session): the markers are in the tree, so just report the conflict.
        if record.applied_with_conflict {
            return Ok(AutostashRestore::Conflict {
                files: self.autostash_conflicting_files()?,
            });
        }

        let git_oid = git2::Oid::from(&record.stash);
        let index = self.stash_index_of(git_oid)?.ok_or_else(|| {
            anyhow::anyhow!(
                "auto-stash {} not found in the stash list",
                record.stash.short()
            )
        })?;

        let mut opts = StashApplyOptions::new();
        opts.reinstantiate_index();
        self.inner.stash_apply(index, Some(&mut opts)).context(
            "could not reapply auto-stashed changes — they may conflict with the \
             result; your changes remain in `git stash list`",
        )?;

        let files = self.autostash_conflicting_files()?;
        if !files.is_empty() {
            // Keep the stash and flag the record so we don't reapply it again.
            journal::set_autostash(
                self,
                Some(AutostashRecord {
                    applied_with_conflict: true,
                    ..record
                }),
            )?;
            return Ok(AutostashRestore::Conflict { files });
        }

        self.inner.stash_drop(index)?;
        self.inner.index()?.read(true)?;
        journal::set_autostash(self, None)?;
        Ok(AutostashRestore::Done)
    }

    /// Stage the files the user has resolved; if none remain conflicted, drop the
    /// stash and clear the journal record.
    pub(super) fn continue_autostash(&mut self) -> Result<AutostashContinue> {
        let Some(record) = journal::autostash(self)? else {
            return Ok(AutostashContinue::Resolved);
        };

        let resolved = self.autostash_conflicting_files()?;
        conflict::auto_stage_resolved_conflicts(self, &resolved)?;

        let files = self.autostash_conflicting_files()?;
        if !files.is_empty() {
            return Ok(AutostashContinue::StillUnresolved { files });
        }

        if let Some(index) = self.stash_index_of(git2::Oid::from(&record.stash))? {
            self.inner.stash_drop(index)?;
        }
        self.inner.index()?.read(true)?;
        journal::set_autostash(self, None)?;
        Ok(AutostashContinue::Resolved)
    }

    /// Rewind branch + working tree to the pre-operation tip (undoing the
    /// operation), re-apply the stash there (always conflict-free, since that
    /// tip is the stash's base), drop it, and forget the reverted operation.
    pub(super) fn abort_autostash(&mut self) -> Result<()> {
        let Some(record) = journal::autostash(self)? else {
            return Ok(());
        };
        let discarded_tip = reads::head_oid(self)?;

        // Hard-reset to the pre-operation tip, discarding the conflicted reapply
        // (and the operation's commits) in one step. Scope the commit so its
        // borrow of `self.inner` is released before the stash mutations below.
        let pre = git2::Oid::from(&record.pre_op_tip);
        {
            let pre_commit = self.inner.find_commit(pre)?;
            self.inner
                .reset(pre_commit.as_object(), git2::ResetType::Hard, None)?;
        }

        // Re-apply the stash onto its own base — no conflict — and drop it.
        let git_oid = git2::Oid::from(&record.stash);
        if let Some(index) = self.stash_index_of(git_oid)? {
            let mut opts = StashApplyOptions::new();
            opts.reinstantiate_index();
            self.inner.stash_apply(index, Some(&mut opts)).context(
                "failed to restore auto-stashed changes while aborting; they \
                 remain in `git stash list`",
            )?;
            if let Some(index) = self.stash_index_of(git_oid)? {
                self.inner.stash_drop(index)?;
            }
        }
        self.inner.index()?.read(true)?;

        journal::set_autostash(self, None)?;
        journal::drop_reverted_undo_record(self, &record.pre_op_tip, &discarded_tip)
    }

    /// Conflicting paths (index stage > 0) from a fresh read of the index.
    fn autostash_conflicting_files(&self) -> Result<Vec<String>> {
        let mut index = self.inner.index()?;
        index.read(true)?;
        Ok(conflict::collect_conflict_files_from_index(&index))
    }

    /// Find the current index of the stash entry with the given OID, if present.
    fn stash_index_of(&mut self, oid: git2::Oid) -> Result<Option<usize>> {
        let mut found = None;
        self.inner.stash_foreach(|index, _message, stash_oid| {
            if *stash_oid == oid {
                found = Some(index);
                false // stop iterating
            } else {
                true
            }
        })?;
        Ok(found)
    }
}
