// Core library for git-scissors

use anyhow::{Context, Result};

/// Find the merge-base (reference point) between HEAD and a given commit-ish.
///
/// The commit-ish can be:
/// - A branch name (e.g., "main", "feature")
/// - A tag name (e.g., "v1.0")
/// - A commit hash (short or long)
///
/// Returns the OID of the common ancestor as a string.
pub fn find_reference_point(commit_ish: &str) -> Result<String> {
    let repo = git2::Repository::open(".").context("Failed to open git repository")?;

    let target_object = repo
        .revparse_single(commit_ish)
        .context(format!("Failed to resolve '{}'", commit_ish))?;
    let target_oid = target_object.id();

    let head = repo.head().context("Failed to get HEAD")?;
    let head_oid = head.target().context("HEAD is not a direct reference")?;

    let reference_oid = repo
        .merge_base(head_oid, target_oid)
        .context("Failed to find merge base")?;

    Ok(reference_oid.to_string())
}
