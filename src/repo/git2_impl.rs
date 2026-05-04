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

use crate::{CommitDiff, CommitInfo, Oid};

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
mod conflict;
mod drop_op;
mod hunks;
mod move_op;
mod reads;
mod reword_op;
mod split_op;
mod squash_op;

/// Concrete git repository backed by `libgit2` via the `git2` crate.
///
/// Construct with [`Git2Repo::open`]; then use through the [`GitRepo`] trait.
pub struct Git2Repo {
    inner: git2::Repository,
}

impl Git2Repo {
    /// Try to open a git repository by iteratively trying the given path and
    /// its parents until a repository root is found.
    pub fn open(mut path: std::path::PathBuf) -> Result<Self> {
        loop {
            let result = git2::Repository::open(&path);
            if let Ok(repo) = result {
                return Ok(Git2Repo { inner: repo });
            }
            if !path.pop() {
                anyhow::bail!("Could not find git repository root");
            }
        }
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

    fn staged_diff(&self) -> Option<CommitDiff> {
        reads::staged_diff(self)
    }

    fn unstaged_diff(&self) -> Option<CommitDiff> {
        reads::unstaged_diff(self)
    }

    fn split_commit_per_file(&self, commit_oid: &Oid, head_oid: &Oid) -> Result<()> {
        split_op::split_commit_per_file(self, commit_oid, head_oid)
    }

    fn split_commit_per_hunk(&self, commit_oid: &Oid, head_oid: &Oid) -> Result<()> {
        split_op::split_commit_per_hunk(self, commit_oid, head_oid)
    }

    fn split_commit_per_hunk_group(
        &self,
        commit_oid: &Oid,
        head_oid: &Oid,
        reference_oid: &Oid,
    ) -> Result<()> {
        split_op::split_commit_per_hunk_group(self, commit_oid, head_oid, reference_oid)
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
        reword_op::reword_commit(self, commit_oid, new_message, head_oid)
    }

    fn get_config_string(&self, key: &str) -> Option<String> {
        reads::get_config_string(self, key)
    }

    fn drop_commit(&self, commit_oid: &Oid, head_oid: &Oid) -> Result<super::RebaseOutcome> {
        drop_op::drop_commit(self, commit_oid, head_oid)
    }

    fn rebase_continue(&self, state: &super::ConflictState) -> Result<super::RebaseOutcome> {
        conflict::rebase_continue(self, state)
    }

    fn rebase_abort(&self, state: &super::ConflictState) -> Result<()> {
        conflict::rebase_abort(self, state)
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
        move_op::move_commit(self, commit_oid, insert_after_oid, head_oid)
    }

    fn squash_commits(
        &self,
        source_oid: &Oid,
        target_oid: &Oid,
        message: &str,
        head_oid: &Oid,
    ) -> Result<super::RebaseOutcome> {
        squash_op::squash_commits(self, source_oid, target_oid, message, head_oid)
    }

    fn stage_file(&self, path: &str) -> Result<()> {
        self.stage_file(path)
    }

    fn auto_stage_resolved_conflicts(&self, files: &[String]) -> Result<()> {
        conflict::auto_stage_resolved_conflicts(self, files)
    }

    fn default_branch(&self) -> Option<String> {
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
        is_fixup: bool,
        head_oid: &Oid,
    ) -> Result<Option<super::ConflictState>> {
        squash_op::squash_try_combine(
            self,
            source_oid,
            target_oid,
            combined_message,
            is_fixup,
            head_oid,
        )
    }

    fn squash_finalize(
        &self,
        ctx: &super::SquashContext,
        message: &str,
        original_branch_oid: &Oid,
    ) -> Result<super::RebaseOutcome> {
        squash_op::squash_finalize(self, ctx, message, original_branch_oid)
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
        let mut opts = git2::DiffOptions::new();
        opts.context_lines(0);
        opts.interhunk_lines(0);

        let head_tree = self.inner.head().ok().and_then(|h| h.peel_to_tree().ok());

        // Returns true only when the delta is a real file change, not a gitlink.
        let is_real = |delta: git2::DiffDelta| {
            delta.old_file().mode() != git2::FileMode::Commit
                && delta.new_file().mode() != git2::FileMode::Commit
        };

        let has_staged = self
            .inner
            .diff_tree_to_index(head_tree.as_ref(), None, Some(&mut opts))
            .map(|d| d.deltas().any(is_real))
            .unwrap_or(false);

        let has_unstaged = self
            .inner
            .diff_index_to_workdir(None, Some(&mut opts))
            .map(|d| d.deltas().any(is_real))
            .unwrap_or(false);

        if has_staged || has_unstaged {
            anyhow::bail!(
                "You have staged or unstaged changes. \
                 Stash or commit them before running this operation."
            );
        }
        Ok(())
    }

    /// Refuse if any staged or unstaged change touches a file in `commit_paths`.
    fn check_dirty_overlap(&self, commit_paths: &HashSet<String>) -> Result<()> {
        let mut overlapping: Vec<String> = Vec::new();
        for synthetic_diff in [self.staged_diff(), self.unstaged_diff()]
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

    /// Reset the working tree and index to match HEAD.
    fn checkout_head(&self) -> Result<()> {
        let repo = &self.inner;

        // Explicitly reset the index to HEAD's tree before forcing a workdir
        // checkout. cherry_pick_chain uses in-memory index operations
        // (apply_to_tree, merge_trees) whose returned indexes are distinct from
        // the repo's singleton index, but libgit2's checkout_head re-reads the
        // on-disk index to compare against HEAD. If the on-disk index is stale
        // (not yet written after the chain completed), checkout can silently
        // leave the index empty — all files appear as staged deletions with the
        // actual files untracked. Writing HEAD's tree into the index first
        // guarantees a clean baseline regardless of prior index state.
        let head_oid = repo.head()?.peel_to_commit()?;
        let mut index = repo.index()?;
        index.read_tree(&head_oid.tree()?)?;
        index.write()?;

        let mut checkout = git2::build::CheckoutBuilder::new();
        checkout.force();
        repo.checkout_head(Some(&mut checkout))?;
        Ok(())
    }
}
