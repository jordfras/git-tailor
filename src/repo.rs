// Repository operations

use anyhow::{Context, Result};

use crate::{CommitDiff, CommitInfo, DiffLine, DiffLineKind, FileDiff, Hunk};

/// Find the merge-base (reference point) between HEAD and a given commit-ish.
///
/// The commit-ish can be:
/// - A branch name (e.g., "main", "feature")
/// - A tag name (e.g., "v1.0")
/// - A commit hash (short or long)
///
/// Returns the OID of the common ancestor as a string.
pub fn find_reference_point(commit_ish: &str) -> Result<String> {
    find_reference_point_in(".", commit_ish)
}

/// Internal: find reference point in a specific repository path.
pub(crate) fn find_reference_point_in(repo_path: &str, commit_ish: &str) -> Result<String> {
    let repo = git2::Repository::open(repo_path).context("Failed to open git repository")?;

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

/// Extract `CommitInfo` metadata from a `git2::Commit`.
fn commit_info_from(commit: &git2::Commit) -> CommitInfo {
    CommitInfo {
        oid: commit.id().to_string(),
        summary: commit.summary().unwrap_or("").to_string(),
        author: commit.author().name().unwrap_or("").to_string(),
        date: commit.time().seconds().to_string(),
        parent_oids: commit.parent_ids().map(|id| id.to_string()).collect(),
    }
}

/// List commits from one commit back to another (inclusive).
///
/// Walks the commit graph from `from_oid` back to `to_oid`, collecting
/// commit metadata. Returns commits in oldest-to-newest order.
///
/// Both `from_oid` and `to_oid` can be any commit-ish (branch, tag, hash).
/// The range includes both endpoints.
pub fn list_commits(from_oid: &str, to_oid: &str) -> Result<Vec<CommitInfo>> {
    list_commits_in(".", from_oid, to_oid)
}

/// Internal: list commits in a specific repository path.
pub fn list_commits_in(repo_path: &str, from_oid: &str, to_oid: &str) -> Result<Vec<CommitInfo>> {
    let repo = git2::Repository::open(repo_path).context("Failed to open git repository")?;

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
        commits.push(commit_info_from(&commit));

        if oid == to_commit_oid {
            break;
        }
    }

    commits.reverse();
    Ok(commits)
}

/// Extract the full diff for a single commit compared to its first parent.
///
/// For the root commit (no parents), diffs against an empty tree so all
/// files show as additions. Returns a `CommitDiff` containing the commit
/// metadata and every file/hunk/line changed.
pub fn commit_diff(oid: &str) -> Result<CommitDiff> {
    commit_diff_in(".", oid)
}

/// Internal: extract commit diff in a specific repository path.
pub fn commit_diff_in(repo_path: &str, oid: &str) -> Result<CommitDiff> {
    let repo = git2::Repository::open(repo_path).context("Failed to open git repository")?;

    let object = repo
        .revparse_single(oid)
        .context(format!("Failed to resolve '{}'", oid))?;
    let commit = object
        .peel_to_commit()
        .context("Resolved object is not a commit")?;

    let new_tree = commit.tree().context("Failed to get commit tree")?;

    // For root commits, diff against an empty tree
    let parent_tree = if commit.parent_count() > 0 {
        Some(commit.parent(0)?.tree()?)
    } else {
        None
    };

    let diff = repo.diff_tree_to_tree(parent_tree.as_ref(), Some(&new_tree), None)?;

    let mut files: Vec<FileDiff> = Vec::new();

    for delta_idx in 0..diff.deltas().len() {
        let delta = diff.get_delta(delta_idx).expect("delta index in range");

        let old_path = delta
            .old_file()
            .path()
            .map(|p| p.to_string_lossy().into_owned());
        let new_path = delta
            .new_file()
            .path()
            .map(|p| p.to_string_lossy().into_owned());

        let patch = git2::Patch::from_diff(&diff, delta_idx)?
            .context("Failed to extract patch from diff")?;

        let mut hunks = Vec::new();
        for hunk_idx in 0..patch.num_hunks() {
            let (hunk_header, _num_lines) = patch.hunk(hunk_idx)?;

            let mut lines = Vec::new();
            for line_idx in 0..patch.num_lines_in_hunk(hunk_idx)? {
                let line = patch.line_in_hunk(hunk_idx, line_idx)?;
                let kind = match line.origin() {
                    '+' => DiffLineKind::Addition,
                    '-' => DiffLineKind::Deletion,
                    _ => DiffLineKind::Context,
                };
                let content = String::from_utf8_lossy(line.content()).to_string();
                lines.push(DiffLine { kind, content });
            }

            hunks.push(Hunk {
                old_start: hunk_header.old_start(),
                old_lines: hunk_header.old_lines(),
                new_start: hunk_header.new_start(),
                new_lines: hunk_header.new_lines(),
                lines,
            });
        }

        files.push(FileDiff {
            old_path,
            new_path,
            hunks,
        });
    }

    Ok(CommitDiff {
        commit: commit_info_from(&commit),
        files,
    })
}
