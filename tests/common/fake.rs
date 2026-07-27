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
use git_tailor::{CommitDiff, CommitInfo, Oid, repo::RepoRead};

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

    /// Configure the diff returned by [`RepoRead::commit_diff`] for any OID.
    pub fn with_commit_diff(mut self, diff: CommitDiff) -> Self {
        self.commit_diff = Some(diff);
        self
    }

    /// Configure the diff returned by [`RepoRead::staged_diff`].
    pub fn with_staged_diff(mut self, diff: CommitDiff) -> Self {
        self.staged_diff = Some(diff);
        self
    }

    /// Configure the diff returned by [`RepoRead::unstaged_diff`].
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

/// Configurable [`RepoRead`] stub produced by [`StubRepoBuilder`].
///
/// It implements only the read side of the repository — enough to drive the
/// rendering/view code under test. Diff-returning methods set via the builder
/// return their configured values; all other reads panic with
/// `unimplemented!()`.
#[derive(Default)]
pub struct StubRepo {
    commit_diff: Option<CommitDiff>,
    staged_diff: Option<CommitDiff>,
    unstaged_diff: Option<CommitDiff>,
}

impl RepoRead for StubRepo {
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
    fn get_config_string(&self, _key: &str) -> Result<Option<String>> {
        unimplemented!()
    }
    fn workdir(&self) -> Option<std::path::PathBuf> {
        unimplemented!()
    }
    fn is_worktree_dirty(&self) -> Result<bool> {
        Ok(false)
    }
    fn read_index_stage(&self, _path: &str, _stage: i32) -> Result<Option<Vec<u8>>> {
        unimplemented!()
    }
    fn read_conflicting_files(&self) -> Vec<String> {
        unimplemented!()
    }
    fn root_commit_oid(&self) -> Result<Oid> {
        unimplemented!()
    }
    fn default_branch(&self) -> Result<Option<String>> {
        Ok(None)
    }
    fn commit_walker<'a>(
        &'a self,
        _from_oid: &Oid,
        _to_oid: &Oid,
    ) -> Result<Box<dyn Iterator<Item = Result<CommitInfo>> + 'a>> {
        unimplemented!()
    }
}
