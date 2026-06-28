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

//! Stage or unstage all working-tree changes at once (`git add -A` / reset to
//! HEAD). These only mutate the index; the journalling wrapper that makes them
//! undoable lives in [`super::Git2Repo::journaled_index_op`].

use anyhow::{Context, Result};

use super::Git2Repo;

/// Stage every working-tree change: modifications, untracked additions, and
/// deletions. Equivalent to `git add -A`.
pub(super) fn stage_all(repo: &Git2Repo) -> Result<()> {
    let mut index = repo.inner.index().context("failed to open index")?;
    index.read(true).context("failed to refresh index")?;

    // A conflicted index belongs to an in-progress rebase, not a clean staging
    // operation; `write_tree` (used to snapshot for undo) would fail on it too.
    if index.has_conflicts() {
        anyhow::bail!("cannot stage while the index has unresolved conflicts");
    }

    // `add_all` stages modifications and untracked files but not removals;
    // `update_all` captures deletions of already-tracked files.
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .context("failed to stage working-tree changes")?;
    index
        .update_all(["*"].iter(), None)
        .context("failed to stage deletions")?;
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
