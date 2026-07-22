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

use anyhow::{Result, anyhow};
use git_tailor::{
    CommitDiff, CommitInfo, Oid,
    app::SquashMode,
    repo::{ConflictState, GitRepo, RebaseOutcome, SquashContext},
};

/// Builder for a configurable [`StubRepo`].
///
/// Set only the methods your test needs; all others will panic with
/// `unimplemented!()` for unconfigured paths.
///
/// # Example
///
/// ```ignore
/// let repo = StubRepoBuilder::new().with_commit_diff(diff).build();
/// ```
pub struct StubRepoBuilder {
    commit_diff: Option<CommitDiff>,
    staged_diff: Option<CommitDiff>,
    unstaged_diff: Option<CommitDiff>,
}

impl StubRepoBuilder {
    pub fn new() -> Self {
        Self {
            commit_diff: None,
            staged_diff: None,
            unstaged_diff: None,
        }
    }

    /// Configure the diff returned by [`GitRepo::commit_diff`] for any OID.
    pub fn with_commit_diff(mut self, diff: CommitDiff) -> Self {
        self.commit_diff = Some(diff);
        self
    }

    /// Configure the diff returned by [`GitRepo::staged_diff`].
    pub fn with_staged_diff(mut self, diff: CommitDiff) -> Self {
        self.staged_diff = Some(diff);
        self
    }

    /// Configure the diff returned by [`GitRepo::unstaged_diff`].
    pub fn with_unstaged_diff(mut self, diff: CommitDiff) -> Self {
        self.unstaged_diff = Some(diff);
        self
    }

    pub fn build(self) -> StubRepo {
        StubRepo {
            commit_diff: self.commit_diff,
            staged_diff: self.staged_diff,
            unstaged_diff: self.unstaged_diff,
        }
    }
}

impl Default for StubRepoBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Configurable `GitRepo` stub produced by [`StubRepoBuilder`].
///
/// Methods set via the builder return their configured values; all other
/// methods panic with `unimplemented!()`.
#[derive(Default)]
pub struct StubRepo {
    commit_diff: Option<CommitDiff>,
    staged_diff: Option<CommitDiff>,
    unstaged_diff: Option<CommitDiff>,
}

impl GitRepo for StubRepo {
    fn head_oid(&self) -> Result<Oid> {
        unimplemented!()
    }
    fn find_reference_point(&self, _commit_ish: &str) -> Result<Oid> {
        unimplemented!()
    }
    fn list_commits(&self, _from: &Oid, _to: &Oid) -> Result<Vec<CommitInfo>> {
        unimplemented!()
    }
    fn commit_diff(&self, _oid: &Oid, _context_lines: u32) -> Result<CommitDiff> {
        match &self.commit_diff {
            Some(diff) => Ok(diff.clone()),
            None => Err(anyhow!("commit_diff not configured on StubRepo")),
        }
    }
    fn commit_diff_for_fragmap(&self, _oid: &Oid) -> Result<CommitDiff> {
        unimplemented!()
    }
    fn staged_diff(&self, _context_lines: u32) -> Result<Option<CommitDiff>> {
        Ok(self.staged_diff.clone())
    }
    fn staged_diff_for_fragmap(&self) -> Result<Option<CommitDiff>> {
        Ok(self.staged_diff.clone())
    }
    fn unstaged_diff(&self, _context_lines: u32) -> Result<Option<CommitDiff>> {
        Ok(self.unstaged_diff.clone())
    }
    fn unstaged_diff_for_fragmap(&self) -> Result<Option<CommitDiff>> {
        Ok(self.unstaged_diff.clone())
    }
    fn split_commit_per_file(&self, _commit_oid: &Oid, _head_oid: &Oid) -> Result<()> {
        unimplemented!()
    }
    fn split_commit_per_hunk(&self, _commit_oid: &Oid, _head_oid: &Oid) -> Result<()> {
        unimplemented!()
    }
    fn split_commit_per_hunk_group(
        &self,
        _commit_oid: &Oid,
        _head_oid: &Oid,
        _reference_oid: &Oid,
    ) -> Result<()> {
        unimplemented!()
    }
    fn split_commit_out_files(
        &self,
        _commit_oid: &Oid,
        _file_paths: &[String],
        _head_oid: &Oid,
    ) -> Result<()> {
        unimplemented!()
    }
    fn split_commit_out_hunks(
        &self,
        _commit_oid: &Oid,
        _hunks: &[(usize, usize)],
        _head_oid: &Oid,
        _context_lines: u32,
    ) -> Result<()> {
        unimplemented!()
    }
    fn count_split_per_file(&self, _commit_oid: &Oid) -> Result<usize> {
        unimplemented!()
    }
    fn count_split_per_hunk(&self, _commit_oid: &Oid) -> Result<usize> {
        unimplemented!()
    }
    fn count_split_per_hunk_group(
        &self,
        _commit_oid: &Oid,
        _head_oid: &Oid,
        _reference_oid: &Oid,
    ) -> Result<usize> {
        unimplemented!()
    }
    fn reword_commit(&self, _commit_oid: &Oid, _new_message: &str, _head_oid: &Oid) -> Result<()> {
        unimplemented!()
    }
    fn get_config_string(&self, _key: &str) -> Result<Option<String>> {
        unimplemented!()
    }
    fn drop_commit(&self, _commit_oid: &Oid, _head_oid: &Oid) -> Result<RebaseOutcome> {
        unimplemented!()
    }
    fn move_commit(
        &self,
        _commit_oid: &Oid,
        _insert_after_oid: Option<&Oid>,
        _head_oid: &Oid,
    ) -> Result<RebaseOutcome> {
        unimplemented!()
    }
    fn rebase_continue(&self, _state: &ConflictState) -> Result<RebaseOutcome> {
        unimplemented!()
    }
    fn rebase_abort(&self, _state: &ConflictState) -> Result<()> {
        unimplemented!()
    }
    fn read_journal(&self) -> Result<git_tailor::repo::JournalStatus> {
        unimplemented!()
    }
    fn clear_journal(&self) -> Result<()> {
        unimplemented!()
    }
    fn prune_stale_journal(&self) -> Result<()> {
        unimplemented!()
    }
    fn clean_journal(&self) -> Result<git_tailor::repo::JournalCleanSummary> {
        unimplemented!()
    }
    fn undo(&self) -> Result<git_tailor::repo::UndoOutcome> {
        unimplemented!()
    }
    fn redo(&self) -> Result<git_tailor::repo::UndoOutcome> {
        unimplemented!()
    }
    fn pending_undo_skips_autostash(&self) -> Result<bool> {
        unimplemented!()
    }
    fn pending_redo_skips_autostash(&self) -> Result<bool> {
        unimplemented!()
    }
    fn stage_all(&self) -> Result<git_tailor::repo::StageOutcome> {
        unimplemented!()
    }
    fn unstage_all(&self) -> Result<git_tailor::repo::StageOutcome> {
        unimplemented!()
    }
    fn commit_staged(&self, _message: &str) -> Result<git_tailor::repo::CommitOutcome> {
        unimplemented!()
    }
    fn autostash_save(&mut self) -> Result<()> {
        unimplemented!()
    }
    fn autostash_restore(&mut self) -> Result<git_tailor::repo::AutostashRestore> {
        unimplemented!()
    }
    fn autostash_conflict_continue(&mut self) -> Result<git_tailor::repo::AutostashContinue> {
        unimplemented!()
    }
    fn autostash_conflict_abort(&mut self) -> Result<()> {
        unimplemented!()
    }
    fn workdir(&self) -> Option<std::path::PathBuf> {
        unimplemented!()
    }
    fn read_index_stage(&self, _path: &str, _stage: i32) -> Result<Option<Vec<u8>>> {
        unimplemented!()
    }
    fn read_conflicting_files(&self) -> Vec<String> {
        unimplemented!()
    }
    fn squash_commits(
        &self,
        _source_oid: &Oid,
        _target_oid: &Oid,
        _message: &str,
        _head_oid: &Oid,
    ) -> Result<RebaseOutcome> {
        unimplemented!()
    }
    fn squash_try_combine(
        &self,
        _source_oid: &Oid,
        _target_oid: &Oid,
        _combined_message: &str,
        _squash_mode: SquashMode,
        _head_oid: &Oid,
    ) -> Result<Option<ConflictState>> {
        unimplemented!()
    }
    fn squash_finalize(
        &self,
        _ctx: &SquashContext,
        _message: &str,
        _original_branch_oid: &Oid,
        _autofixup_context: Option<&git_tailor::repo::AutofixupContext>,
    ) -> Result<RebaseOutcome> {
        unimplemented!()
    }
    fn autofixup(
        &self,
        _head_oid: &Oid,
        _reference_oid: &Oid,
        _message_overrides: &std::collections::HashMap<String, String>,
    ) -> Result<RebaseOutcome> {
        unimplemented!()
    }
    fn stage_file(&self, _path: &str) -> Result<()> {
        unimplemented!()
    }
    fn auto_stage_resolved_conflicts(&self, _files: &[String]) -> Result<()> {
        unimplemented!()
    }
    fn default_branch(&self) -> Result<Option<String>> {
        Ok(None)
    }
    fn root_commit_oid(&self) -> Result<Oid> {
        unimplemented!()
    }
    fn commit_walker<'a>(
        &'a self,
        _from_oid: &Oid,
        _to_oid: &Oid,
    ) -> Result<Box<dyn Iterator<Item = Result<CommitInfo>> + 'a>> {
        unimplemented!()
    }
}
