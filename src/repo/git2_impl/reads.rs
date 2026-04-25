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

//! Read-only repository queries: HEAD/reference resolution, commit listing,
//! diff extraction (committed, staged, unstaged), index/config inspection.

use anyhow::{Context, Result};

use super::Git2Repo;
use crate::{CommitDiff, CommitInfo, DiffLine, DiffLineKind, FileDiff, Hunk, Oid, VirtualOid};

pub(super) fn head_oid(repo: &Git2Repo) -> Result<Oid> {
    Ok(Oid::from(
        repo.inner
            .head()
            .context("Failed to get HEAD")?
            .target()
            .context("HEAD is not a direct reference")?,
    ))
}

pub(super) fn find_reference_point(repo: &Git2Repo, commit_ish: &str) -> Result<Oid> {
    let target_object = repo
        .inner
        .revparse_single(commit_ish)
        .context(format!("Failed to resolve '{}'", commit_ish))?;
    let target_oid = target_object.id();

    let head = repo.inner.head().context("Failed to get HEAD")?;
    let head_oid = head.target().context("HEAD is not a direct reference")?;

    let reference_oid = repo
        .inner
        .merge_base(head_oid, target_oid)
        .context("Failed to find merge base")?;

    Ok(Oid::from(reference_oid))
}

pub(super) fn list_commits(
    repo: &Git2Repo,
    from_oid: &Oid,
    to_oid: &Oid,
) -> Result<Vec<CommitInfo>> {
    let from_object = repo
        .inner
        .revparse_single(from_oid.long())
        .context(format!("Failed to resolve '{}'", from_oid))?;
    let from_commit_oid = from_object.id();

    let to_object = repo
        .inner
        .revparse_single(to_oid.long())
        .context(format!("Failed to resolve '{}'", to_oid))?;
    let to_commit_oid = to_object.id();

    let mut revwalk = repo.inner.revwalk()?;
    revwalk.push(from_commit_oid)?;

    let mut commits = Vec::new();

    for oid_result in revwalk {
        let oid = oid_result?;
        let commit = repo.inner.find_commit(oid)?;
        commits.push(commit_info_from(&commit));

        if oid == to_commit_oid {
            break;
        }
    }

    commits.reverse();
    Ok(commits)
}

pub(super) fn commit_diff(repo: &Git2Repo, oid: &Oid) -> Result<CommitDiff> {
    let object = repo
        .inner
        .revparse_single(oid.long())
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

    let diff = repo
        .inner
        .diff_tree_to_tree(parent_tree.as_ref(), Some(&new_tree), None)?;

    extract_commit_diff(&diff, &commit)
}

pub(super) fn commit_diff_for_fragmap(repo: &Git2Repo, oid: &Oid) -> Result<CommitDiff> {
    let object = repo
        .inner
        .revparse_single(oid.long())
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

    let mut diff =
        repo.inner
            .diff_tree_to_tree(parent_tree.as_ref(), Some(&new_tree), Some(&mut opts))?;

    diff.find_similar(None)?;

    extract_commit_diff(&diff, &commit)
}

pub(super) fn staged_diff(repo: &Git2Repo) -> Option<CommitDiff> {
    let head = repo.inner.head().ok()?.peel_to_tree().ok();

    let mut opts = git2::DiffOptions::new();
    opts.context_lines(0);
    opts.interhunk_lines(0);

    let diff = repo
        .inner
        .diff_tree_to_index(head.as_ref(), None, Some(&mut opts))
        .ok()?;

    let files = extract_files_from_diff(&diff).ok()?;
    if files.iter().all(|f| f.hunks.is_empty()) {
        return None;
    }

    Some(CommitDiff {
        commit: synthetic_commit_info(VirtualOid::Staged, "(staged changes)"),
        files,
    })
}

pub(super) fn unstaged_diff(repo: &Git2Repo) -> Option<CommitDiff> {
    let mut opts = git2::DiffOptions::new();
    opts.context_lines(0);
    opts.interhunk_lines(0);

    let diff = repo
        .inner
        .diff_index_to_workdir(None, Some(&mut opts))
        .ok()?;

    let files = extract_files_from_diff(&diff).ok()?;
    if files.iter().all(|f| f.hunks.is_empty()) {
        return None;
    }

    Some(CommitDiff {
        commit: synthetic_commit_info(VirtualOid::Unstaged, "(unstaged changes)"),
        files,
    })
}

pub(super) fn workdir(repo: &Git2Repo) -> Option<std::path::PathBuf> {
    repo.inner.workdir().map(|p| p.to_path_buf())
}

pub(super) fn read_index_stage(repo: &Git2Repo, path: &str, stage: i32) -> Result<Option<Vec<u8>>> {
    let mut index = repo.inner.index().context("failed to read index")?;
    index
        .read(true)
        .context("failed to refresh index from disk")?;
    let Some(entry) = index.get_path(std::path::Path::new(path), stage) else {
        return Ok(None);
    };
    let blob = repo
        .inner
        .find_blob(entry.id)
        .context("failed to find blob for index stage")?;
    Ok(Some(blob.content().to_vec()))
}

pub(super) fn get_config_string(repo: &Git2Repo, key: &str) -> Option<String> {
    repo.inner.config().ok()?.get_string(key).ok()
}

pub(super) fn default_branch(repo: &Git2Repo) -> Option<String> {
    let reference = repo.inner.find_reference("refs/remotes/origin/HEAD").ok()?;
    let target = reference.symbolic_target()?;
    // Strip the "refs/remotes/" prefix so the caller can pass the result
    // directly to find_reference_point (e.g. "origin/main").
    target.strip_prefix("refs/remotes/").map(str::to_string)
}

pub(super) fn root_commit_oid(repo: &Git2Repo) -> Result<Oid> {
    let mut revwalk = repo.inner.revwalk()?;
    revwalk.push_head()?;
    for oid_result in revwalk {
        let oid = oid_result?;
        let commit = repo.inner.find_commit(oid)?;
        if commit.parent_count() == 0 {
            return Ok(Oid::from(oid));
        }
    }
    anyhow::bail!("No root commit found reachable from HEAD")
}

pub(super) fn commit_info_from(commit: &git2::Commit) -> CommitInfo {
    let author_time = commit.author().when();
    let commit_time = commit.time();

    CommitInfo {
        oid: VirtualOid::Real(Oid::from(commit.id())),
        summary: commit.summary().unwrap_or("").to_string(),
        author: commit.author().name().map(|s| s.to_string()),
        date: Some(commit.time().seconds().to_string()),
        parent_oids: commit.parent_ids().map(Oid::from).collect(),
        message: commit.message().unwrap_or("").to_string(),
        author_email: commit.author().email().map(|s| s.to_string()),
        author_date: Some(git_time_to_offset_datetime(author_time)),
        committer: commit.committer().name().map(|s| s.to_string()),
        committer_email: commit.committer().email().map(|s| s.to_string()),
        commit_date: Some(git_time_to_offset_datetime(commit_time)),
    }
}

pub(super) fn synthetic_commit_info(oid: VirtualOid, summary: &str) -> CommitInfo {
    CommitInfo {
        oid,
        summary: summary.to_string(),
        author: None,
        date: None,
        parent_oids: vec![],
        message: summary.to_string(),
        author_email: None,
        author_date: None,
        committer: None,
        committer_email: None,
        commit_date: None,
    }
}

fn extract_commit_diff(diff: &git2::Diff, commit: &git2::Commit) -> Result<CommitDiff> {
    Ok(CommitDiff {
        commit: commit_info_from(commit),
        files: extract_files_from_diff(diff)?,
    })
}

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

fn git_time_to_offset_datetime(git_time: git2::Time) -> time::OffsetDateTime {
    let offset_seconds = git_time.offset_minutes() * 60;
    let utc_offset =
        time::UtcOffset::from_whole_seconds(offset_seconds).unwrap_or(time::UtcOffset::UTC);

    time::OffsetDateTime::from_unix_timestamp(git_time.seconds())
        .unwrap_or(time::OffsetDateTime::UNIX_EPOCH)
        .to_offset(utc_offset)
}

#[cfg(test)]
mod tests {
    use super::git_time_to_offset_datetime;

    #[test]
    fn utc_epoch_stays_at_zero() {
        let t = git2::Time::new(0, 0);
        let dt = git_time_to_offset_datetime(t);
        assert_eq!(dt.unix_timestamp(), 0);
        assert_eq!(dt.offset(), time::UtcOffset::UTC);
    }

    #[test]
    fn positive_offset_applied_correctly() {
        // 60-minute (UTC+1) offset: same instant, but hour should read as 1.
        let t = git2::Time::new(0, 60);
        let dt = git_time_to_offset_datetime(t);
        assert_eq!(dt.unix_timestamp(), 0);
        let expected_offset = time::UtcOffset::from_whole_seconds(3600).unwrap();
        assert_eq!(dt.offset(), expected_offset);
        assert_eq!(dt.hour(), 1);
    }

    #[test]
    fn negative_offset_applied_correctly() {
        // −300-minute (UTC−5) offset: same instant, hour reads as 19 on previous day.
        let t = git2::Time::new(0, -300);
        let dt = git_time_to_offset_datetime(t);
        assert_eq!(dt.unix_timestamp(), 0);
        let expected_offset = time::UtcOffset::from_whole_seconds(-18000).unwrap();
        assert_eq!(dt.offset(), expected_offset);
        assert_eq!(dt.hour(), 19);
    }
}
