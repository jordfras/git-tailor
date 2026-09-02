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

//! Stage or unstage all changes to tracked files at once (`git add -u` / reset
//! to HEAD). These only mutate the index; the journalling wrapper that makes
//! them undoable lives in [`super::Git2Repo::journaled_index_op`].

use anyhow::{Context, Result};

use super::Git2Repo;

/// Stage every change to a tracked file: modifications and deletions.
/// Equivalent to `git add -u`. Untracked files are deliberately left alone —
/// they are invisible in the unstaged row's diff, so sweeping them in would
/// stage files the user never saw.
pub(super) fn stage_all(repo: &Git2Repo) -> Result<()> {
    let mut index = repo.inner.index().context("failed to open index")?;
    index.read(true).context("failed to refresh index")?;

    // A conflicted index belongs to an in-progress rebase, not a clean staging
    // operation; `write_tree` (used to snapshot for undo) would fail on it too.
    if index.has_conflicts() {
        anyhow::bail!("cannot stage while the index has unresolved conflicts");
    }

    // `update_all` only touches paths already in the index, so it picks up
    // modifications and deletions while skipping untracked files.
    index
        .update_all(["*"].iter(), None)
        .context("failed to stage working-tree changes")?;
    index.write().context("failed to write index")?;
    Ok(())
}

/// Unstage every staged change by resetting the index to HEAD's tree. Also
/// clears any conflict stages.
pub(super) fn unstage_all(repo: &Git2Repo) -> Result<()> {
    let head_tree = repo
        .inner
        .head()
        .context("failed to resolve HEAD")?
        .peel_to_tree()
        .context("failed to read HEAD tree")?;
    let mut index = repo.inner.index().context("failed to open index")?;
    index
        .read_tree(&head_tree)
        .context("failed to reset index to HEAD")?;
    index.write().context("failed to write index")?;
    Ok(())
}
