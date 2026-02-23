// Core library for git-scissors

use anyhow::{Context, Result};

/// Represents commit metadata extracted from git repository.
///
/// This is a pure data structure containing commit information
/// without any git2 object dependencies.
#[derive(Debug, Clone)]
pub struct CommitInfo {
    pub oid: String,
    pub summary: String,
    pub author: String,
    pub date: String,
    pub parent_oids: Vec<String>,
}

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

/// List commits from one commit back to another (inclusive).
///
/// Walks the commit graph from `from_oid` back to `to_oid`, collecting
/// commit metadata. Returns commits in oldest-to-newest order.
///
/// Both `from_oid` and `to_oid` can be any commit-ish (branch, tag, hash).
/// The range includes both endpoints.
pub fn list_commits(from_oid: &str, to_oid: &str) -> Result<Vec<CommitInfo>> {
    let repo = git2::Repository::open(".").context("Failed to open git repository")?;

    let from_object = repo
        .revparse_single(from_oid)
        .context(format!("Failed to resolve '{}'", from_oid))?;
    let from_commit_oid = from_object.id();

    let to_object = repo
        .revparse_single(to_oid)
        .context(format!("Failed to resolve '{}'", to_oid))?;
    let to_commit_oid = to_object.id();

    let mut revwalk = repo.revwalk()?;
    revwalk.push(from_commit_oid)?;

    let mut commits = Vec::new();

    for oid_result in revwalk {
        let oid = oid_result?;
        let commit = repo.find_commit(oid)?;

        let commit_info = CommitInfo {
            oid: oid.to_string(),
            summary: commit.summary().unwrap_or("").to_string(),
            author: commit.author().name().unwrap_or("").to_string(),
            date: commit.time().seconds().to_string(),
            parent_oids: commit.parent_ids().map(|id| id.to_string()).collect(),
        };

        commits.push(commit_info);

        if oid == to_commit_oid {
            break;
        }
    }

    commits.reverse();
    Ok(commits)
}
