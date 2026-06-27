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

//! Create a new commit from the currently staged changes (the index), advancing
//! the branch ref. The journalling wrapper that makes it undoable (as a soft
//! ref move) lives in [`super::Git2Repo::commit_staged`].

use anyhow::{Context, Result};

use super::Git2Repo;
use crate::Oid;

/// Commit whatever is staged in the index with `message`, using the current HEAD
/// as the sole parent. Returns the new commit OID, or `None` when nothing is
/// staged (the index tree equals HEAD's tree).
pub(super) fn commit_staged(repo: &Git2Repo, message: &str) -> Result<Option<Oid>> {
    let parent = repo
        .inner
        .head()
        .context("failed to resolve HEAD")?
        .peel_to_commit()
        .context("failed to read HEAD commit")?;
    let head_tree = parent.tree().context("failed to read HEAD tree")?;

    let mut index = repo.inner.index().context("failed to open index")?;
    index.read(true).context("failed to refresh index")?;
    if index.has_conflicts() {
        anyhow::bail!("cannot commit while the index has unresolved conflicts");
    }

    let tree_oid = index.write_tree().context("failed to write index tree")?;
    if tree_oid == head_tree.id() {
        return Ok(None);
    }
    let tree = repo
        .inner
        .find_tree(tree_oid)
        .context("failed to find staged tree")?;

    let sig = repo
        .inner
        .signature()
        .context("failed to build commit signature (set user.name / user.email)")?;
    let new_oid = repo
        .inner
        .commit(None, &sig, &sig, message, &tree, &[&parent])
        .context("failed to create commit")?;
    repo.advance_branch_ref(new_oid, "git-tailor: commit staged changes")?;
    Ok(Some(new_oid.into()))
}
