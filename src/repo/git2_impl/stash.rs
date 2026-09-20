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
use git2::{Signature, StashApplyOptions};

use super::super::{AutostashContinue, AutostashRestore};
use super::Git2Repo;
use super::conflict;
use super::journal;
use super::journal::AutostashRecord;
use super::reads;
use crate::Oid;

impl Git2Repo {
    /// Stash dirty working-tree state (staged + unstaged) when auto-stash is
    /// enabled and the tree is dirty, recording the stash and the current
    /// branch tip in the journal.
    ///
    /// Tracked changes only. Untracked files used to be swept in as well, to
    /// stop a checkout landing on top of one, but that never worked: the stash
    /// is only taken when [`Self::is_worktree_dirty`] says so, and that ignores
    /// untracked files — so the one case it was meant to cover, a working tree
    /// holding nothing else, never triggered it. When it *did* fire it merely
    /// deferred the clash into the reapply, which merged the stashed copy onto
    /// the reintroduced file and left conflict markers behind without an
    /// unmerged index entry for [`Self::restore_autostash`] to notice.
    ///
    /// That case is now refused up front by
    /// [`Git2Repo::refuse_untracked_collisions`], which needs the file left
    /// where it is to see it. Leaving untracked files alone also matches what
    /// the working-tree fold has always done, so the two agree on what they
    /// touch.
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
        self.set_work_aside("git-tailor: autostash", None)
    }

    /// Put whatever is uncommitted into a stash and record it, whatever asked
    /// for it. A no-op when there is nothing to set aside.
    ///
    /// Separate from [`Self::save_autostash`] so that "did the user ask for a
    /// stash" and "take one" are two questions. Both callers come through here —
    /// auto-stash, and the working-tree fold parking the row it did not take —
    /// so `fold` records which, since the slot holds one and they are put back
    /// at different moments.
    pub(super) fn set_work_aside(&mut self, message: &str, fold: Option<&Oid>) -> Result<()> {
        if !self.is_worktree_dirty()? {
            return Ok(());
        }

        // One slot, two callers. Displacing a record leaves its stash named by
        // nothing: no undo, recovery or abort would find that work again.
        if let Some(existing) = journal::autostash(self)? {
            anyhow::bail!(
                "Work is already set aside in stash {0} and has not been put back. \
                 Recover it with `git stash apply {0}` — `git stash pop` would act \
                 on whatever is at stash@{{0}}, which may be something else.",
                existing.stash
            );
        }

        // Without this, `stash_save2` would serialize the stale blob for a
        // same-size edit and lose it.
        self.refresh_index_stat_cache()?;

        // Captured before the operation advances the ref, so an aborted reapply
        // can rewind here — where the stash, whose base this tip is, re-applies
        // cleanly.
        let pre_op_tip = reads::head_oid(self)?;
        let branch_refname = self.current_branch_refname().unwrap_or_default();

        let sig = self
            .inner
            .signature()
            .or_else(|_| Signature::now("git-tailor", "git-tailor@localhost"))?;
        let oid = self.inner.stash_save2(&sig, Some(message), None)?;

        // The stash reset the working tree and index; refresh the cached index
        // so subsequent reads on this handle see the clean state.
        self.inner.index()?.read(true)?;
        journal::set_autostash(
            self,
            Some(AutostashRecord {
                stash: Oid::from(oid),
                pre_op_tip,
                applied_with_conflict: false,
                branch_refname,
                fold_temp_oid: fold.cloned(),
            }),
        )?;
        Ok(())
    }

    /// Put work set aside back at `base`, the commit it was taken on, and drop
    /// it. The merge cannot conflict — a stash applied onto its own base is a
    /// no-op — but an untracked file sitting on one of its paths can still be in
    /// the way, which is refused by name before anything moves.
    ///
    /// The abort half of [`Self::set_work_aside`], for a caller that will move
    /// the branch itself afterwards — the fold rewinds past `base` to the tip it
    /// started from, which [`Self::abort_autostash`] has no reason to do.
    pub(super) fn abort_work_aside(&mut self, base: &Oid) -> Result<()> {
        // Read before the reset, but the reset happens either way: a row whose
        // counterpart was clean leaves nothing to set aside, and the working
        // tree still has to come back to `base` from wherever the operation
        // checked it out to.
        let record = journal::autostash(self)?;

        // No branch check here. The only caller is the fold's `restore`, which
        // has already checked against the fold's *own* recorded branch — and the
        // name on this record may belong to someone else's `--autostash`, which
        // would refuse an unwind that is perfectly in order.

        let mine = record.filter(|r| r.fold_temp_oid.as_ref() == Some(base));

        let base_oid = git2::Oid::from(base);
        let base_tree = self.commit_tree_id(base_oid)?;

        // Both writes checked before either happens, so a refusal leaves the fold
        // re-abortable rather than half-unwound. The reapply needs its own check:
        // a file the parked row newly staged is in no commit at all, so `base`'s
        // tree cannot account for it.
        self.refuse_tree_collisions(base_tree)?;
        if let Some(record) = &mine {
            let stash_tree = self.commit_tree_id(git2::Oid::from(&record.stash))?;
            self.refuse_untracked_collisions(base_tree, stash_tree)?;
        }

        // Scoped so the commit's borrow ends before the stash mutations below.
        {
            let base_commit = self.inner.find_commit(base_oid)?;
            self.inner
                .reset(base_commit.as_object(), git2::ResetType::Hard, None)?;
        }

        let Some(record) = mine else {
            self.inner.index()?.read(true)?;
            return Ok(());
        };

        let git_oid = git2::Oid::from(&record.stash);
        if let Some(index) = self.stash_index_of(git_oid)? {
            let mut opts = StashApplyOptions::new();
            opts.reinstantiate_index();
            self.inner
                .stash_apply(index, Some(&mut opts))
                .context("failed to put the working-tree changes back")?;
            if let Some(index) = self.stash_index_of(git_oid)? {
                self.inner.stash_drop(index)?;
            }
        }
        self.inner.index()?.read(true)?;
        journal::set_autostash(self, None)
    }

    /// Drop work set aside without putting it back, and forget the record.
    ///
    /// For an operation being abandoned rather than finished or unwound, where
    /// the content has already been preserved somewhere else. Nothing else may
    /// use this: a stash dropped without a copy elsewhere is work destroyed.
    pub(super) fn discard_work_aside(&mut self, fold: &Oid) -> Result<()> {
        let Some(record) =
            journal::autostash(self)?.filter(|r| r.fold_temp_oid.as_ref() == Some(fold))
        else {
            return Ok(());
        };
        if let Some(index) = self.stash_index_of(git2::Oid::from(&record.stash))? {
            self.inner
                .stash_drop(index)
                .context("failed to drop the set-aside changes")?;
        }
        journal::set_autostash(self, None)
    }

    /// Whether the work in the slot was set aside by the fold on `temp_oid`.
    pub(super) fn work_aside_is_fold(&mut self, temp_oid: &Oid) -> Result<bool> {
        Ok(journal::autostash(self)?.is_some_and(|r| r.fold_temp_oid.as_ref() == Some(temp_oid)))
    }

    /// Reapply and drop the recorded auto-stash, restoring the staged/unstaged
    /// split. Returns [`AutostashRestore::Done`] when nothing is recorded or it
    /// reapplies cleanly.
    ///
    /// A content clash is detected through the index rather than the return
    /// value: `stash_apply` writes conflict markers and leaves unmerged entries
    /// behind, reporting either `Ok` or `Conflict` depending on whether the
    /// staged/unstaged split could be reinstantiated. On conflict the stash is
    /// **kept** and the journal record is flagged `applied_with_conflict`, so the
    /// user's changes are never lost and startup recovery does not reapply it a
    /// second time.
    ///
    /// A path an untracked file occupies is refused first. It raises the same
    /// `Conflict` code as a content clash but has nothing to resolve: the
    /// checkout cannot land at all, so the fallback below would retry something
    /// that cannot succeed and report it as a generic failure.
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

        // The stash holds what was tracked when it was taken, which includes
        // paths that exist in no commit. Nothing else puts those back on disk,
        // so an untracked file can be sitting on one by the time the reapply
        // runs. Checked before the apply, where the stash is still intact and
        // the user can clear the path and retry.
        let stash_tree = self.commit_tree_id(git_oid)?;
        let current_tree = self.commit_tree_id(git2::Oid::from(&reads::head_oid(self)?))?;
        self.refuse_untracked_collisions(current_tree, stash_tree)?;

        // `reinstantiate_index` asks libgit2 to restore the staged/unstaged
        // split as well as the contents. It cannot do both when the reapply
        // conflicts: a path cannot be staged and unmerged at once, so libgit2
        // refuses with `Conflict` rather than writing markers. Refusing would
        // strand the user — the rewrite is already done and their work is only
        // in the stash, with no dialog to resolve it — so fall back to a plain
        // apply, which does write markers. The split is what gives way, and it
        // is the lesser loss: the contents are on disk and resolvable.
        //
        // Retrying assumes the refused attempt applied nothing. libgit2 checks
        // for conflicts against the index before it writes, so it gives up
        // before touching the working tree — but it is its assumption to keep,
        // and a partial apply followed by this retry would apply twice.
        let mut opts = StashApplyOptions::new();
        opts.reinstantiate_index();
        if let Err(e) = self.inner.stash_apply(index, Some(&mut opts)) {
            if e.code() != git2::ErrorCode::Conflict {
                return Err(e).context(
                    "could not reapply auto-stashed changes — they may conflict \
                     with the result; your changes remain in `git stash list`",
                );
            }
            self.inner
                .stash_apply(index, Some(&mut StashApplyOptions::new()))
                .context(
                    "could not reapply auto-stashed changes — they may conflict \
                     with the result; your changes remain in `git stash list`",
                )?;
        }

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

        // Re-resolved, not reused from before the apply: `refs/stash` is
        // repository-wide while the session lock is per working tree, so a push
        // from a linked worktree shifts every position.
        if let Some(index) = self.stash_index_of(git_oid)? {
            self.inner.stash_drop(index)?;
        }
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
        // A fold's leftover is not the stash dialog's to abort: its `pre_op_tip`
        // is the fold's temporary commit, so the reset below would rewind the
        // branch onto a synthetic commit.
        if record.fold_temp_oid.is_some() {
            anyhow::bail!(
                "These changes were set aside by a working-tree squash, not by \
                 --autostash. Finish or abort that operation instead."
            );
        }

        // The hard reset below moves whatever branch HEAD resolves to now; that
        // has to still be the branch this stash was taken on.
        self.refuse_if_branch_switched(&record.branch_refname)?;
        let discarded_tip = reads::head_oid(self)?;

        // Scoped: the commit's borrow of `self.inner` must end before the
        // stash mutations below, which need it mutably.
        {
            let pre = git2::Oid::from(&record.pre_op_tip);
            let pre_commit = self.inner.find_commit(pre)?;

            // A hard reset writes the whole tip's tree out, so it reintroduces
            // every path the operation removed — checked before anything moves.
            let pre_tree = pre_commit
                .tree()
                .context("failed to read the pre-operation tree")?
                .id();
            self.refuse_tree_collisions(pre_tree)?;

            // Hard-reset to the pre-operation tip, discarding the conflicted
            // reapply (and the operation's commits) in one step.
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
    fn autostash_conflicting_files(&self) -> Result<Vec<std::path::PathBuf>> {
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
