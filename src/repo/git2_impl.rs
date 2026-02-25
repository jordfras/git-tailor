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

use anyhow::{Context, Result};
use std::collections::HashSet;

use crate::{CommitDiff, CommitInfo, DiffLine, DiffLineKind, FileDiff, Hunk};

use super::GitRepo;

/// Concrete git repository backed by `libgit2` via the `git2` crate.
///
/// Construct with [`Git2Repo::open`]; then use through the [`GitRepo`] trait.
pub struct Git2Repo {
    inner: git2::Repository,
}

impl Git2Repo {
    /// Try to open a git repository by iteratively trying the given path and
    /// its parents until a repository root is found.
    pub fn open(mut path: std::path::PathBuf) -> Result<Self> {
        loop {
            let result = git2::Repository::open(&path);
            if let Ok(repo) = result {
                return Ok(Git2Repo { inner: repo });
            }
            if !path.pop() {
                anyhow::bail!("Could not find git repository root");
            }
        }
    }
}

impl GitRepo for Git2Repo {
    fn head_oid(&self) -> Result<String> {
        Ok(self
            .inner
            .head()
            .context("Failed to get HEAD")?
            .target()
            .context("HEAD is not a direct reference")?
            .to_string())
    }

    fn find_reference_point(&self, commit_ish: &str) -> Result<String> {
        let target_object = self
            .inner
            .revparse_single(commit_ish)
            .context(format!("Failed to resolve '{}'", commit_ish))?;
        let target_oid = target_object.id();

        let head = self.inner.head().context("Failed to get HEAD")?;
        let head_oid = head.target().context("HEAD is not a direct reference")?;

        let reference_oid = self
            .inner
            .merge_base(head_oid, target_oid)
            .context("Failed to find merge base")?;

        Ok(reference_oid.to_string())
    }

    fn list_commits(&self, from_oid: &str, to_oid: &str) -> Result<Vec<CommitInfo>> {
        let from_object = self
            .inner
            .revparse_single(from_oid)
            .context(format!("Failed to resolve '{}'", from_oid))?;
        let from_commit_oid = from_object.id();

        let to_object = self
            .inner
            .revparse_single(to_oid)
            .context(format!("Failed to resolve '{}'", to_oid))?;
        let to_commit_oid = to_object.id();

        let mut revwalk = self.inner.revwalk()?;
        revwalk.push(from_commit_oid)?;

        let mut commits = Vec::new();

        for oid_result in revwalk {
            let oid = oid_result?;
            let commit = self.inner.find_commit(oid)?;
            commits.push(commit_info_from(&commit));

            if oid == to_commit_oid {
                break;
            }
        }

        commits.reverse();
        Ok(commits)
    }

    fn commit_diff(&self, oid: &str) -> Result<CommitDiff> {
        let object = self
            .inner
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

        let diff = self
            .inner
            .diff_tree_to_tree(parent_tree.as_ref(), Some(&new_tree), None)?;

        extract_commit_diff(&diff, &commit)
    }

    fn commit_diff_for_fragmap(&self, oid: &str) -> Result<CommitDiff> {
        let object = self
            .inner
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

        let diff =
            self.inner
                .diff_tree_to_tree(parent_tree.as_ref(), Some(&new_tree), Some(&mut opts))?;

        extract_commit_diff(&diff, &commit)
    }

    fn staged_diff(&self) -> Option<CommitDiff> {
        let head = self.inner.head().ok()?.peel_to_tree().ok();

        let mut opts = git2::DiffOptions::new();
        opts.context_lines(0);
        opts.interhunk_lines(0);

        let diff = self
            .inner
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

    fn unstaged_diff(&self) -> Option<CommitDiff> {
        let mut opts = git2::DiffOptions::new();
        opts.context_lines(0);
        opts.interhunk_lines(0);

        let diff = self
            .inner
            .diff_index_to_workdir(None, Some(&mut opts))
            .ok()?;

        let files = extract_files_from_diff(&diff).ok()?;
        if files.iter().all(|f| f.hunks.is_empty()) {
            return None;
        }

        Some(CommitDiff {
            commit: synthetic_commit_info("unstaged", "Unstaged changes"),
            files,
        })
    }

    fn split_commit_per_file(&self, commit_oid: &str, head_oid: &str) -> Result<()> {
        let repo = &self.inner;

        let commit_git_oid =
            git2::Oid::from_str(commit_oid).context("Invalid commit OID for split")?;
        let commit = repo.find_commit(commit_git_oid)?;

        if commit.parent_count() != 1 {
            anyhow::bail!("Can only split a commit with exactly one parent (merge commits and root commits are not supported)");
        }
        let parent_commit = commit.parent(0)?;
        let parent_tree = parent_commit.tree()?;
        let commit_tree = commit.tree()?;

        // Compute full diff to enumerate files and for the overlap check
        let full_diff = repo.diff_tree_to_tree(Some(&parent_tree), Some(&commit_tree), None)?;
        let file_count = full_diff.deltas().len();

        if file_count < 2 {
            anyhow::bail!("Commit touches fewer than 2 files — nothing to split");
        }

        // Collect the file paths touched by this commit
        let commit_paths: HashSet<String> = full_diff
            .deltas()
            .filter_map(|d| {
                d.new_file()
                    .path()
                    .or_else(|| d.old_file().path())
                    .map(|p| p.to_string_lossy().into_owned())
            })
            .collect();

        // Dirty overlap check: refuse if staged/unstaged changes share any files
        let mut overlapping: Vec<String> = Vec::new();
        for synthetic_diff in [self.staged_diff(), self.unstaged_diff()]
            .into_iter()
            .flatten()
        {
            for file in &synthetic_diff.files {
                let path = file
                    .new_path
                    .as_deref()
                    .or(file.old_path.as_deref())
                    .unwrap_or("");
                if commit_paths.contains(path) && !overlapping.contains(&path.to_string()) {
                    overlapping.push(path.to_string());
                }
            }
        }
        if !overlapping.is_empty() {
            overlapping.sort();
            anyhow::bail!(
                "Cannot split: staged/unstaged changes overlap with: {}",
                overlapping.join(", ")
            );
        }

        // Create one commit per file, each building on the previous
        let mut current_base_oid = parent_commit.id();
        for delta_idx in 0..file_count {
            let delta = full_diff.get_delta(delta_idx).expect("delta index valid");
            let path = delta
                .new_file()
                .path()
                .or_else(|| delta.old_file().path())
                .expect("delta has a path")
                .to_string_lossy()
                .into_owned();

            // Diff from parent→commit scoped to this specific file
            let mut opts = git2::DiffOptions::new();
            opts.pathspec(&path);
            let file_diff =
                repo.diff_tree_to_tree(Some(&parent_tree), Some(&commit_tree), Some(&mut opts))?;

            let base_commit = repo.find_commit(current_base_oid)?;
            let base_tree = base_commit.tree()?;

            let mut new_index = repo.apply_to_tree(&base_tree, &file_diff, None)?;
            if new_index.has_conflicts() {
                anyhow::bail!("Conflict applying changes for file: {}", path);
            }
            let new_tree_oid = new_index.write_tree_to(repo)?;
            let new_tree = repo.find_tree(new_tree_oid)?;

            let author = commit.author();
            let committer = commit.committer();
            let message = format!(
                "{} ({}/{})",
                commit.summary().unwrap_or("split"),
                delta_idx + 1,
                file_count
            );

            let new_oid = repo.commit(
                None,
                &author,
                &committer,
                &message,
                &new_tree,
                &[&base_commit],
            )?;
            current_base_oid = new_oid;
        }

        // Rebase descendant commits (those between commit_oid exclusive and head_oid inclusive)
        let head_git_oid = git2::Oid::from_str(head_oid).context("Invalid head OID")?;
        if head_git_oid != commit_git_oid {
            let mut revwalk = repo.revwalk()?;
            revwalk.push(head_git_oid)?;

            // Collect descendants newest-first, stop at (but exclude) the split commit
            let mut descendants: Vec<git2::Oid> = Vec::new();
            for oid_result in revwalk {
                let oid = oid_result?;
                if oid == commit_git_oid {
                    break;
                }
                descendants.push(oid);
            }
            // Reverse to get oldest-first so we cherry-pick in order
            descendants.reverse();

            for desc_oid in descendants {
                let desc_commit = repo.find_commit(desc_oid)?;
                let onto_commit = repo.find_commit(current_base_oid)?;

                let mut cherry_index =
                    repo.cherrypick_commit(&desc_commit, &onto_commit, 0, None)?;
                if cherry_index.has_conflicts() {
                    anyhow::bail!(
                        "Conflict rebasing {} onto split result",
                        &desc_oid.to_string()[..10]
                    );
                }
                let new_tree_oid = cherry_index.write_tree_to(repo)?;
                let new_tree = repo.find_tree(new_tree_oid)?;

                let author = desc_commit.author();
                let committer = desc_commit.committer();
                let new_oid = repo.commit(
                    None,
                    &author,
                    &committer,
                    desc_commit.message().unwrap_or(""),
                    &new_tree,
                    &[&onto_commit],
                )?;
                current_base_oid = new_oid;
            }
        }

        // Fast-forward the branch ref that HEAD points to
        let head_ref = repo.head()?;
        let branch_refname = head_ref
            .resolve()
            .context("HEAD is not a symbolic ref")?
            .name()
            .context("Ref has no name")?
            .to_string();
        repo.reference(
            &branch_refname,
            current_base_oid,
            true,
            "git-tailor: split per-file",
        )?;

        Ok(())
    }

    fn split_commit_per_hunk(&self, commit_oid: &str, head_oid: &str) -> Result<()> {
        let repo = &self.inner;

        let commit_git_oid =
            git2::Oid::from_str(commit_oid).context("Invalid commit OID for split")?;
        let commit = repo.find_commit(commit_git_oid)?;

        if commit.parent_count() != 1 {
            anyhow::bail!("Can only split a commit with exactly one parent (merge commits and root commits are not supported)");
        }
        let parent_commit = commit.parent(0)?;
        let parent_tree = parent_commit.tree()?;
        let commit_tree = commit.tree()?;

        // Compute diff with 0 context lines so adjacent hunks stay separate
        let mut diff_opts = git2::DiffOptions::new();
        diff_opts.context_lines(0);
        diff_opts.interhunk_lines(0);
        let full_diff =
            repo.diff_tree_to_tree(Some(&parent_tree), Some(&commit_tree), Some(&mut diff_opts))?;

        // Count total hunks across all files
        let mut hunk_count = 0usize;
        full_diff.foreach(
            &mut |_, _| true,
            None,
            Some(&mut |_, _| {
                hunk_count += 1;
                true
            }),
            None,
        )?;

        if hunk_count < 2 {
            anyhow::bail!("Commit has fewer than 2 hunks — nothing to split per hunk");
        }

        // Collect the file paths touched by this commit for the dirty overlap check
        let commit_paths: HashSet<String> = full_diff
            .deltas()
            .filter_map(|d| {
                d.new_file()
                    .path()
                    .or_else(|| d.old_file().path())
                    .map(|p| p.to_string_lossy().into_owned())
            })
            .collect();

        let mut overlapping: Vec<String> = Vec::new();
        for synthetic_diff in [self.staged_diff(), self.unstaged_diff()]
            .into_iter()
            .flatten()
        {
            for file in &synthetic_diff.files {
                let path = file
                    .new_path
                    .as_deref()
                    .or(file.old_path.as_deref())
                    .unwrap_or("");
                if commit_paths.contains(path) && !overlapping.contains(&path.to_string()) {
                    overlapping.push(path.to_string());
                }
            }
        }
        if !overlapping.is_empty() {
            overlapping.sort();
            anyhow::bail!(
                "Cannot split: staged/unstaged changes overlap with: {}",
                overlapping.join(", ")
            );
        }

        // Build one commit per hunk.
        //
        // For hunk k we apply the first k+1 hunks of the diff (hunks 0..=k) to the
        // original parent_tree. Each resulting tree differs from the previous one by
        // exactly one hunk, so the commit log shows one hunk per entry.
        let mut current_base_oid = parent_commit.id();
        for target_k in 0..hunk_count {
            let cumulative_tree_oid = if target_k == hunk_count - 1 {
                // Last iteration: use the known commit tree to avoid any rounding error.
                commit_tree.id()
            } else {
                let mut counter = 0usize;
                let mut apply_opts = git2::ApplyOptions::new();
                apply_opts.hunk_callback(|_hunk| {
                    let accept = counter <= target_k;
                    counter += 1;
                    accept
                });
                let mut idx =
                    repo.apply_to_tree(&parent_tree, &full_diff, Some(&mut apply_opts))?;
                if idx.has_conflicts() {
                    anyhow::bail!(
                        "Conflict building intermediate tree for hunk {}",
                        target_k + 1
                    );
                }
                idx.write_tree_to(repo)?
            };

            let cumulative_tree = repo.find_tree(cumulative_tree_oid)?;
            let base_commit = repo.find_commit(current_base_oid)?;

            let author = commit.author();
            let committer = commit.committer();
            let message = format!(
                "{} ({}/{})",
                commit.summary().unwrap_or("split"),
                target_k + 1,
                hunk_count
            );

            let new_oid = repo.commit(
                None,
                &author,
                &committer,
                &message,
                &cumulative_tree,
                &[&base_commit],
            )?;
            current_base_oid = new_oid;
        }

        // Rebase descendant commits onto the new tip
        let head_git_oid = git2::Oid::from_str(head_oid).context("Invalid head OID")?;
        if head_git_oid != commit_git_oid {
            let mut revwalk = repo.revwalk()?;
            revwalk.push(head_git_oid)?;

            let mut descendants: Vec<git2::Oid> = Vec::new();
            for oid_result in revwalk {
                let oid = oid_result?;
                if oid == commit_git_oid {
                    break;
                }
                descendants.push(oid);
            }
            descendants.reverse();

            for desc_oid in descendants {
                let desc_commit = repo.find_commit(desc_oid)?;
                let onto_commit = repo.find_commit(current_base_oid)?;

                let mut cherry_index =
                    repo.cherrypick_commit(&desc_commit, &onto_commit, 0, None)?;
                if cherry_index.has_conflicts() {
                    anyhow::bail!(
                        "Conflict rebasing {} onto split result",
                        &desc_oid.to_string()[..10]
                    );
                }
                let new_tree_oid = cherry_index.write_tree_to(repo)?;
                let new_tree = repo.find_tree(new_tree_oid)?;

                let author = desc_commit.author();
                let committer = desc_commit.committer();
                let new_oid = repo.commit(
                    None,
                    &author,
                    &committer,
                    desc_commit.message().unwrap_or(""),
                    &new_tree,
                    &[&onto_commit],
                )?;
                current_base_oid = new_oid;
            }
        }

        // Fast-forward the branch ref that HEAD points to
        let head_ref = repo.head()?;
        let branch_refname = head_ref.resolve().context("HEAD is not a symbolic ref")?;
        let branch_name = branch_refname
            .name()
            .context("Ref has no name")?
            .to_string();
        repo.reference(
            &branch_name,
            current_base_oid,
            true,
            "git-tailor: split per-hunk",
        )?;

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

pub(crate) fn git_time_to_offset_datetime(git_time: git2::Time) -> time::OffsetDateTime {
    let offset_seconds = git_time.offset_minutes() * 60;
    let utc_offset =
        time::UtcOffset::from_whole_seconds(offset_seconds).unwrap_or(time::UtcOffset::UTC);

    time::OffsetDateTime::from_unix_timestamp(git_time.seconds())
        .unwrap_or(time::OffsetDateTime::UNIX_EPOCH)
        .to_offset(utc_offset)
}

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
