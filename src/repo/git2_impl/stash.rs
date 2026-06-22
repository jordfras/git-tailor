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
//! The stash OID is recorded in the journal so the restore can happen when the
//! operation truly completes — even across a conflict pause or a crash/restart.

use anyhow::{Context, Result};
use git2::{Signature, StashApplyOptions, StashFlags};

use super::Git2Repo;
use super::journal;
use crate::Oid;

impl Git2Repo {
    /// Stash dirty working-tree state (staged + unstaged + untracked) when
    /// auto-stash is enabled and the tree is dirty, recording the stash OID in
    /// the journal.
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
        journal::set_autostash(self, Some(Oid::from(oid)))
    }

    /// Reapply and drop the recorded auto-stash, restoring the staged/unstaged
    /// split. No-op when nothing is recorded.
    ///
    /// Errors — leaving the stash in the list — if it cannot be reapplied
    /// cleanly (e.g. the user's changes conflict with the rewritten result).
    pub(super) fn restore_autostash(&mut self) -> Result<()> {
        let Some(oid) = journal::autostash(self)? else {
            return Ok(());
        };
        let git_oid = git2::Oid::from(&oid);
        let index = self.stash_index_of(git_oid)?.ok_or_else(|| {
            anyhow::anyhow!("auto-stash {} not found in the stash list", oid.short())
        })?;

        let mut opts = StashApplyOptions::new();
        opts.reinstantiate_index();
        self.inner.stash_apply(index, Some(&mut opts)).context(
            "could not reapply auto-stashed changes — they may conflict with the \
             result; your changes remain in `git stash list`",
        )?;
        self.inner.stash_drop(index)?;
        self.inner.index()?.read(true)?;
        journal::set_autostash(self, None)
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
