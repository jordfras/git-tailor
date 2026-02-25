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

/// Convert git2::Time to time::OffsetDateTime.
fn git_time_to_offset_datetime(git_time: git2::Time) -> time::OffsetDateTime {
    let offset_seconds = git_time.offset_minutes() * 60;
    let utc_offset =
        time::UtcOffset::from_whole_seconds(offset_seconds).unwrap_or(time::UtcOffset::UTC);

    time::OffsetDateTime::from_unix_timestamp(git_time.seconds())
        .unwrap_or(time::OffsetDateTime::UNIX_EPOCH)
        .to_offset(utc_offset)
}

/// Extract `CommitInfo` metadata from a `git2::Commit`.
fn commit_info_from(commit: &git2::Commit) -> CommitInfo {
    let author_time = commit.author().when();
    let commit_time = commit.time();

    CommitInfo {
        oid: commit.id().to_string(),
        summary: commit.summary().unwrap_or("").to_string(),
        author: commit.author().name().unwrap_or("").to_string(),
        date: commit.time().seconds().to_string(),
        parent_oids: commit.parent_ids().map(|id| id.to_string()).collect(),
        message: commit.message().unwrap_or("").to_string(),
        author_email: commit.author().email().unwrap_or("").to_string(),
        author_date: git_time_to_offset_datetime(author_time),
        committer: commit.committer().name().unwrap_or("").to_string(),
        committer_email: commit.committer().email().unwrap_or("").to_string(),
        commit_date: git_time_to_offset_datetime(commit_time),
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

/// Extract commit diff with zero context lines, suitable for fragmap analysis.
///
/// The fragmap algorithm needs each logical change as its own hunk. With
/// the default 3-line context, git merges adjacent hunks together which
/// produces fewer but larger hunks — breaking the SPG's fine-grained
/// span tracking.
pub fn commit_diff_for_fragmap(oid: &str) -> Result<CommitDiff> {
    commit_diff_for_fragmap_in(".", oid)
}

/// Internal: extract commit diff for fragmap in a specific repository path.
pub fn commit_diff_for_fragmap_in(repo_path: &str, oid: &str) -> Result<CommitDiff> {
    let repo = git2::Repository::open(repo_path).context("Failed to open git repository")?;

    let object = repo
        .revparse_single(oid)
        .context(format!("Failed to resolve '{}'", oid))?;
    let commit = object
        .peel_to_commit()
        .context("Resolved object is not a commit")?;

    let new_tree = commit.tree().context("Failed to get commit tree")?;

    let parent_tree = if commit.parent_count() > 0 {
        Some(commit.parent(0)?.tree()?)
    } else {
        None
    };

    let mut opts = git2::DiffOptions::new();
    opts.context_lines(0);
    opts.interhunk_lines(0);

    let diff = repo.diff_tree_to_tree(parent_tree.as_ref(), Some(&new_tree), Some(&mut opts))?;

    extract_commit_diff(&repo, &diff, &commit)
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

    extract_commit_diff(&repo, &diff, &commit)
}

fn extract_commit_diff(
    _repo: &git2::Repository,
    diff: &git2::Diff,
    commit: &git2::Commit,
) -> Result<CommitDiff> {
    Ok(CommitDiff {
        commit: commit_info_from(commit),
        files: extract_files_from_diff(diff)?,
    })
}

/// Extract `FileDiff` entries from a `git2::Diff`.
///
/// Shared by both commit-based diffs and synthetic (staged/unstaged) diffs.
fn extract_files_from_diff(diff: &git2::Diff) -> Result<Vec<FileDiff>> {
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

        let status = match delta.status() {
            git2::Delta::Unmodified => crate::DeltaStatus::Unmodified,
            git2::Delta::Added => crate::DeltaStatus::Added,
            git2::Delta::Deleted => crate::DeltaStatus::Deleted,
            git2::Delta::Modified => crate::DeltaStatus::Modified,
            git2::Delta::Renamed => crate::DeltaStatus::Renamed,
            git2::Delta::Copied => crate::DeltaStatus::Copied,
            git2::Delta::Ignored => crate::DeltaStatus::Ignored,
            git2::Delta::Untracked => crate::DeltaStatus::Untracked,
            git2::Delta::Typechange => crate::DeltaStatus::Typechange,
            git2::Delta::Unreadable => crate::DeltaStatus::Unreadable,
            git2::Delta::Conflicted => crate::DeltaStatus::Conflicted,
        };

        let patch = git2::Patch::from_diff(diff, delta_idx)?
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
            status,
            hunks,
        });
    }

    Ok(files)
}

/// Build a minimal synthetic `CommitInfo` for staged/unstaged pseudo-commits.
fn synthetic_commit_info(oid: &str, summary: &str) -> CommitInfo {
    CommitInfo {
        oid: oid.to_string(),
        summary: summary.to_string(),
        author: String::new(),
        date: String::new(),
        parent_oids: vec![],
        message: summary.to_string(),
        author_email: String::new(),
        author_date: time::OffsetDateTime::UNIX_EPOCH,
        committer: String::new(),
        committer_email: String::new(),
        commit_date: time::OffsetDateTime::UNIX_EPOCH,
    }
}

/// Return a synthetic `CommitDiff` for changes staged in the index (index vs HEAD).
///
/// Returns `None` when the index is clean (no staged changes).
pub fn staged_diff() -> Option<CommitDiff> {
    let repo = git2::Repository::open(".").ok()?;
    let head = repo.head().ok()?.peel_to_tree().ok();

    let mut opts = git2::DiffOptions::new();
    opts.context_lines(0);
    opts.interhunk_lines(0);

    let diff = repo
        .diff_tree_to_index(head.as_ref(), None, Some(&mut opts))
        .ok()?;

    let files = extract_files_from_diff(&diff).ok()?;
    if files.iter().all(|f| f.hunks.is_empty()) {
        return None;
    }

    Some(CommitDiff {
        commit: synthetic_commit_info("staged", "Staged changes"),
        files,
    })
}

/// Return a synthetic `CommitDiff` for unstaged working-tree changes (workdir vs index).
///
/// Returns `None` when the working tree is clean relative to the index.
pub fn unstaged_diff() -> Option<CommitDiff> {
    let repo = git2::Repository::open(".").ok()?;

    let mut opts = git2::DiffOptions::new();
    opts.context_lines(0);
    opts.interhunk_lines(0);

    let diff = repo.diff_index_to_workdir(None, Some(&mut opts)).ok()?;

    let files = extract_files_from_diff(&diff).ok()?;
    if files.iter().all(|f| f.hunks.is_empty()) {
        return None;
    }

    Some(CommitDiff {
        commit: synthetic_commit_info("unstaged", "Unstaged changes"),
        files,
    })
}
