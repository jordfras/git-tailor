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
use std::collections::{HashMap, HashSet};

use crate::{fragmap, CommitDiff, CommitInfo, DiffLine, DiffLineKind, FileDiff, Hunk};

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

        self.check_dirty_overlap(&commit_paths)?;

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

        let head_git_oid = git2::Oid::from_str(head_oid).context("Invalid head OID")?;
        current_base_oid =
            self.rebase_descendants(commit_git_oid, head_git_oid, current_base_oid)?;
        self.advance_branch_ref(current_base_oid, "git-tailor: split per-file")?;

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

        self.check_dirty_overlap(&commit_paths)?;

        // Build one commit per hunk using incremental blob manipulation.
        //
        // At each step, recompute diff(current_tree → commit_tree) with 0 context
        // and apply exactly its first hunk directly to the blob — bypassing
        // apply_to_tree entirely to avoid libgit2 validating rejected hunks against
        // the modified output buffer (which shifts line positions and causes
        // "hunk did not apply").
        let mut current_base_oid = parent_commit.id();
        let mut current_tree_oid = parent_tree.id();
        for target_k in 0..hunk_count {
            let next_tree_oid = if target_k == hunk_count - 1 {
                commit_tree.id()
            } else {
                let current_tree = repo.find_tree(current_tree_oid)?;
                let mut diff_opts = git2::DiffOptions::new();
                diff_opts.context_lines(0);
                diff_opts.interhunk_lines(0);
                let incremental_diff = repo.diff_tree_to_tree(
                    Some(&current_tree),
                    Some(&commit_tree),
                    Some(&mut diff_opts),
                )?;
                apply_single_hunk_to_tree(repo, &current_tree, &incremental_diff)
                    .with_context(|| format!("applying hunk {}", target_k + 1))?
            };

            let next_tree = repo.find_tree(next_tree_oid)?;
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
                &next_tree,
                &[&base_commit],
            )?;
            current_base_oid = new_oid;
            current_tree_oid = next_tree_oid;
        }

        let head_git_oid = git2::Oid::from_str(head_oid).context("Invalid head OID")?;
        current_base_oid =
            self.rebase_descendants(commit_git_oid, head_git_oid, current_base_oid)?;
        self.advance_branch_ref(current_base_oid, "git-tailor: split per-hunk")?;

        Ok(())
    }

    fn split_commit_per_hunk_group(
        &self,
        commit_oid: &str,
        head_oid: &str,
        reference_oid: &str,
    ) -> Result<()> {
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

        // Build the fragmap over all branch commits so hunk grouping reflects
        // how this commit interacts with its neighbours in the branch.
        let branch_commits = self.list_commits(head_oid, reference_oid)?;
        let branch_diffs: Vec<crate::CommitDiff> = branch_commits
            .iter()
            .filter(|c| c.oid != reference_oid && !c.oid.starts_with("synthetic:"))
            .filter_map(|c| self.commit_diff_for_fragmap(&c.oid).ok())
            .collect();

        let (_, assignment) =
            fragmap::assign_hunk_groups(&branch_diffs, commit_oid).ok_or_else(|| {
                anyhow::anyhow!("Commit {} not found in branch diff list", commit_oid)
            })?;

        // Build a 0-context full diff (parent_tree → commit_tree) for tree
        // manipulation; hunk indices here correspond to those in `assignment`.
        let mut diff_opts = git2::DiffOptions::new();
        diff_opts.context_lines(0);
        diff_opts.interhunk_lines(0);
        let full_diff =
            repo.diff_tree_to_tree(Some(&parent_tree), Some(&commit_tree), Some(&mut diff_opts))?;

        let commit_paths: HashSet<String> = full_diff
            .deltas()
            .filter_map(|d| {
                d.new_file()
                    .path()
                    .or_else(|| d.old_file().path())
                    .map(|p| p.to_string_lossy().into_owned())
            })
            .collect();
        self.check_dirty_overlap(&commit_paths)?;

        // Build delta_hunk_groups: for each (delta_idx, hunk_idx), its group index.
        let mut delta_hunk_groups: Vec<Vec<usize>> = Vec::new();
        let num_deltas = full_diff.deltas().len();
        for delta_idx in 0..num_deltas {
            let delta = full_diff.get_delta(delta_idx).context("delta index")?;
            let path = delta
                .new_file()
                .path()
                .or_else(|| delta.old_file().path())
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            let patch = git2::Patch::from_diff(&full_diff, delta_idx)?;
            let num_hunks = patch.as_ref().map(|p| p.num_hunks()).unwrap_or(0);
            let file_groups = assignment.get(&path);
            let groups_for_delta: Vec<usize> = (0..num_hunks)
                .map(|h| file_groups.and_then(|fg| fg.get(h).copied()).unwrap_or(0))
                .collect();
            delta_hunk_groups.push(groups_for_delta);
        }

        // Determine which group indices K's hunks actually touch (sorted).
        // Only these produce output commits — not every column the full fragmap has.
        let mut touched: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
        for groups in &delta_hunk_groups {
            touched.extend(groups);
        }
        let k_groups: Vec<usize> = touched.into_iter().collect();
        let split_count = k_groups.len();

        if split_count < 2 {
            anyhow::bail!("Commit has fewer than 2 hunk groups — nothing to split per hunk group");
        }

        // For each touched group gk (in order), build the intermediate tree by
        // applying all of K's hunks whose group index ≤ gk to parent_tree in one
        // sweep (positions relative to the original, no cumulative offset issues).
        let mut current_base_oid = parent_commit.id();
        let mut current_tree_oid = parent_tree.id();
        for (out_pos, &gk) in k_groups.iter().enumerate() {
            let next_tree_oid = if out_pos == split_count - 1 {
                commit_tree.id()
            } else {
                // Collect (delta_idx → hunk_indices) for hunks in groups 0..=gk.
                let mut selected: HashMap<usize, Vec<usize>> = HashMap::new();
                for (delta_idx, hunk_groups) in delta_hunk_groups.iter().enumerate() {
                    let chosen: Vec<usize> = hunk_groups
                        .iter()
                        .enumerate()
                        .filter_map(|(h, &g)| if g <= gk { Some(h) } else { None })
                        .collect();
                    if !chosen.is_empty() {
                        selected.insert(delta_idx, chosen);
                    }
                }
                apply_selected_hunks_to_tree(repo, &parent_tree, &full_diff, &selected)
                    .with_context(|| format!("building tree for hunk group {}", out_pos + 1))?
            };

            let next_tree = repo.find_tree(next_tree_oid)?;
            let base_commit = repo.find_commit(current_base_oid)?;

            let author = commit.author();
            let committer = commit.committer();
            let message = format!(
                "{} ({}/{})",
                commit.summary().unwrap_or("split"),
                out_pos + 1,
                split_count
            );
            let new_oid = repo.commit(
                None,
                &author,
                &committer,
                &message,
                &next_tree,
                &[&base_commit],
            )?;
            current_base_oid = new_oid;
            current_tree_oid = next_tree_oid;
        }

        let head_git_oid = git2::Oid::from_str(head_oid).context("Invalid head OID")?;
        current_base_oid =
            self.rebase_descendants(commit_git_oid, head_git_oid, current_base_oid)?;
        self.advance_branch_ref(current_base_oid, "git-tailor: split per-hunk-group")?;

        let _ = current_tree_oid; // suppress unused warning
        Ok(())
    }

    fn count_split_per_file(&self, commit_oid: &str) -> Result<usize> {
        let repo = &self.inner;
        let oid = git2::Oid::from_str(commit_oid).context("Invalid commit OID")?;
        let commit = repo.find_commit(oid)?;
        if commit.parent_count() != 1 {
            anyhow::bail!("Can only split a commit with exactly one parent");
        }
        let parent_tree = commit.parent(0)?.tree()?;
        let commit_tree = commit.tree()?;
        let diff = repo.diff_tree_to_tree(Some(&parent_tree), Some(&commit_tree), None)?;
        Ok(diff.deltas().len())
    }

    fn count_split_per_hunk(&self, commit_oid: &str) -> Result<usize> {
        let repo = &self.inner;
        let oid = git2::Oid::from_str(commit_oid).context("Invalid commit OID")?;
        let commit = repo.find_commit(oid)?;
        if commit.parent_count() != 1 {
            anyhow::bail!("Can only split a commit with exactly one parent");
        }
        let parent_tree = commit.parent(0)?.tree()?;
        let commit_tree = commit.tree()?;
        let mut diff_opts = git2::DiffOptions::new();
        diff_opts.context_lines(0);
        diff_opts.interhunk_lines(0);
        let diff =
            repo.diff_tree_to_tree(Some(&parent_tree), Some(&commit_tree), Some(&mut diff_opts))?;
        let mut count = 0usize;
        diff.foreach(
            &mut |_, _| true,
            None,
            Some(&mut |_, _| {
                count += 1;
                true
            }),
            None,
        )?;
        Ok(count)
    }

    fn count_split_per_hunk_group(
        &self,
        commit_oid: &str,
        head_oid: &str,
        reference_oid: &str,
    ) -> Result<usize> {
        let branch_commits = self.list_commits(head_oid, reference_oid)?;
        let branch_diffs: Vec<crate::CommitDiff> = branch_commits
            .iter()
            .filter(|c| c.oid != reference_oid && !c.oid.starts_with("synthetic:"))
            .filter_map(|c| self.commit_diff_for_fragmap(&c.oid).ok())
            .collect();

        let (_, assignment) =
            fragmap::assign_hunk_groups(&branch_diffs, commit_oid).ok_or_else(|| {
                anyhow::anyhow!("Commit {} not found in branch diff list", commit_oid)
            })?;

        // Count only the group indices that K's own hunks touch.
        let touched: std::collections::HashSet<usize> =
            assignment.values().flatten().copied().collect();
        Ok(touched.len())
    }

    fn reword_commit(&self, commit_oid: &str, new_message: &str, head_oid: &str) -> Result<()> {
        let repo = &self.inner;

        let commit_git_oid =
            git2::Oid::from_str(commit_oid).context("Invalid commit OID for reword")?;
        let head_git_oid = git2::Oid::from_str(head_oid).context("Invalid HEAD OID for reword")?;
        let commit = repo.find_commit(commit_git_oid)?;

        let parents: Vec<git2::Commit> = (0..commit.parent_count())
            .map(|i| commit.parent(i))
            .collect::<std::result::Result<_, _>>()?;
        let parent_refs: Vec<&git2::Commit> = parents.iter().collect();

        let new_oid = repo.commit(
            None,
            &commit.author(),
            &commit.committer(),
            new_message,
            &commit.tree()?,
            &parent_refs,
        )?;

        let tip = self.rebase_descendants(commit_git_oid, head_git_oid, new_oid)?;
        self.advance_branch_ref(tip, "reword: update branch ref")?;
        Ok(())
    }

    fn get_config_string(&self, key: &str) -> Option<String> {
        self.inner.config().ok()?.get_string(key).ok()
    }

    fn drop_commit(&self, commit_oid: &str, head_oid: &str) -> Result<super::RebaseOutcome> {
        let repo = &self.inner;

        let commit_git_oid =
            git2::Oid::from_str(commit_oid).context("Invalid commit OID for drop")?;
        let head_git_oid = git2::Oid::from_str(head_oid).context("Invalid HEAD OID for drop")?;
        let commit = repo.find_commit(commit_git_oid)?;

        if commit.parent_count() != 1 {
            anyhow::bail!("Cannot drop a merge or root commit");
        }
        let parent_oid = commit.parent_id(0)?;

        let original_branch_oid = head_oid.to_string();

        // Collect descendants: commits strictly between commit_oid and head_oid.
        let descendants = self.collect_descendants(commit_git_oid, head_git_oid)?;

        // Cherry-pick each descendant onto the new chain, starting from the
        // dropped commit's parent.
        let result = self.cherry_pick_chain(parent_oid, &descendants)?;
        match result {
            CherryPickResult::Complete(tip) => {
                self.advance_branch_ref(tip, "git-tailor: drop commit")?;
                self.checkout_head()?;
                Ok(super::RebaseOutcome::Complete)
            }
            CherryPickResult::Conflict {
                tip,
                conflicting_idx,
            } => {
                let conflicting_oid = descendants[conflicting_idx];
                let remaining: Vec<String> = descendants[conflicting_idx + 1..]
                    .iter()
                    .map(|oid| oid.to_string())
                    .collect();

                Ok(super::RebaseOutcome::Conflict(super::ConflictState {
                    original_branch_oid,
                    new_tip_oid: tip.to_string(),
                    conflicting_commit_oid: conflicting_oid.to_string(),
                    remaining_oids: remaining,
                    conflicting_files: collect_conflict_files(repo),
                    still_unresolved: false,
                }))
            }
        }
    }

    fn drop_commit_continue(&self, state: &super::ConflictState) -> Result<super::RebaseOutcome> {
        let repo = &self.inner;

        let tip_oid =
            git2::Oid::from_str(&state.new_tip_oid).context("Invalid tip OID in conflict state")?;
        let conflicting_oid = git2::Oid::from_str(&state.conflicting_commit_oid)
            .context("Invalid conflicting OID in conflict state")?;
        let conflicting_commit = repo.find_commit(conflicting_oid)?;
        let onto_commit = repo.find_commit(tip_oid)?;

        // Re-read index from disk — the user (or another process) resolved
        // conflicts by editing the on-disk index.
        let mut index = repo.index()?;
        index.read(true)?;
        if index.has_conflicts() {
            // The user pressed Enter but some files still have conflict
            // markers. Stay in DropConflict mode with a refreshed file list
            // so the dialog keeps the user informed rather than bailing out
            // and leaving the repo in a broken state.
            return Ok(super::RebaseOutcome::Conflict(super::ConflictState {
                original_branch_oid: state.original_branch_oid.clone(),
                new_tip_oid: state.new_tip_oid.clone(),
                conflicting_commit_oid: state.conflicting_commit_oid.clone(),
                remaining_oids: state.remaining_oids.clone(),
                conflicting_files: collect_conflict_files(repo),
                still_unresolved: true,
            }));
        }

        let new_tree_oid = index.write_tree()?;
        let new_tree = repo.find_tree(new_tree_oid)?;

        let new_tip = repo.commit(
            None,
            &conflicting_commit.author(),
            &conflicting_commit.committer(),
            conflicting_commit.message().unwrap_or(""),
            &new_tree,
            &[&onto_commit],
        )?;

        // Continue cherry-picking remaining descendants.
        let remaining: Vec<git2::Oid> = state
            .remaining_oids
            .iter()
            .map(|s| git2::Oid::from_str(s))
            .collect::<std::result::Result<_, _>>()
            .context("Invalid OID in remaining list")?;

        let result = self.cherry_pick_chain(new_tip, &remaining)?;
        match result {
            CherryPickResult::Complete(final_tip) => {
                self.advance_branch_ref(final_tip, "git-tailor: drop commit (continue)")?;
                self.checkout_head()?;
                Ok(super::RebaseOutcome::Complete)
            }
            CherryPickResult::Conflict {
                tip,
                conflicting_idx,
            } => {
                let conflicting_oid = remaining[conflicting_idx];
                let new_remaining: Vec<String> = remaining[conflicting_idx + 1..]
                    .iter()
                    .map(|oid| oid.to_string())
                    .collect();

                Ok(super::RebaseOutcome::Conflict(super::ConflictState {
                    original_branch_oid: state.original_branch_oid.clone(),
                    new_tip_oid: tip.to_string(),
                    conflicting_commit_oid: conflicting_oid.to_string(),
                    remaining_oids: new_remaining,
                    conflicting_files: collect_conflict_files(repo),
                    still_unresolved: false,
                }))
            }
        }
    }

    fn drop_commit_abort(&self, state: &super::ConflictState) -> Result<()> {
        let original_oid = git2::Oid::from_str(&state.original_branch_oid)
            .context("Invalid original branch OID in conflict state")?;
        self.advance_branch_ref(original_oid, "git-tailor: drop commit (abort)")?;
        self.checkout_head()?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Private helpers for drop/conflict operations
// ---------------------------------------------------------------------------

/// Collect paths of all files that have conflict entries (stage > 0) in the
/// repository's current index.  Returns them sorted for a stable display order.
fn collect_conflict_files(repo: &git2::Repository) -> Vec<String> {
    let mut index = match repo.index() {
        Ok(i) => i,
        Err(_) => return Vec::new(),
    };
    let _ = index.read(true);
    let mut paths: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for entry in index.iter() {
        // stage is encoded in the high bits of flags
        let stage = (entry.flags >> 12) & 0x3;
        if stage > 0 {
            if let Ok(p) = std::str::from_utf8(&entry.path) {
                paths.insert(p.to_string());
            }
        }
    }
    paths.into_iter().collect()
}

// ---------------------------------------------------------------------------
// Private helpers for split operations (not part of the GitRepo trait)
// ---------------------------------------------------------------------------

/// Apply the first hunk of the first non-empty delta in `diff` to `base_tree`
/// and return the resulting tree OID.
///
/// The diff must have been computed from `base_tree`, so the hunk's old-side
/// content matches exactly.  This avoids `apply_to_tree` with `hunk_callback`
/// filtering, which fails because libgit2 validates rejected hunks against the
/// already-modified output buffer (whose line positions have shifted).
fn apply_single_hunk_to_tree(
    repo: &git2::Repository,
    base_tree: &git2::Tree,
    diff: &git2::Diff,
) -> Result<git2::Oid> {
    for delta_idx in 0..diff.deltas().len() {
        let mut patch = match git2::Patch::from_diff(diff, delta_idx)? {
            Some(p) => p,
            None => continue,
        };
        if patch.num_hunks() == 0 {
            continue;
        }
        let delta = diff.get_delta(delta_idx).context("delta index in range")?;
        let file_path = delta
            .new_file()
            .path()
            .or_else(|| delta.old_file().path())
            .context("delta has no file path")?
            .to_owned();

        let (old_content, mode) = match delta.status() {
            git2::Delta::Added => {
                let m: u32 = delta.new_file().mode().into();
                (Vec::new(), m)
            }
            _ => {
                let entry = base_tree
                    .get_path(&file_path)
                    .with_context(|| format!("'{}' not in base tree", file_path.display()))?;
                let blob = repo.find_blob(entry.id())?;
                (blob.content().to_owned(), entry.filemode() as u32)
            }
        };

        let new_content = apply_hunk_to_content(&old_content, &mut patch, 0)
            .with_context(|| format!("applying hunk to '{}'", file_path.display()))?;

        let new_blob_oid = repo.blob(&new_content)?;

        // Load base_tree into an in-memory index, update the one file, write tree.
        let mut idx = git2::Index::new()?;
        idx.read_tree(base_tree)?;

        let path_bytes = file_path
            .to_str()
            .context("file path is not valid UTF-8")?
            .as_bytes()
            .to_vec();
        idx.add(&git2::IndexEntry {
            ctime: git2::IndexTime::new(0, 0),
            mtime: git2::IndexTime::new(0, 0),
            dev: 0,
            ino: 0,
            mode,
            uid: 0,
            gid: 0,
            file_size: new_content.len() as u32,
            id: new_blob_oid,
            flags: 0,
            flags_extended: 0,
            path: path_bytes,
        })?;

        return idx.write_tree_to(repo).map_err(Into::into);
    }
    Ok(base_tree.id())
}

/// Apply hunk `hunk_idx` from `patch` to `content`, returning the new bytes.
///
/// Replacement splices in context + added lines, dropping deleted lines.
fn apply_hunk_to_content(
    content: &[u8],
    patch: &mut git2::Patch,
    hunk_idx: usize,
) -> Result<Vec<u8>> {
    let (hunk_header, _) = patch.hunk(hunk_idx)?;
    let old_start = hunk_header.old_start() as usize; // 1-based
    let old_count = hunk_header.old_lines() as usize;

    let lines = split_lines_keep_eol(content);

    let num_lines = patch.num_lines_in_hunk(hunk_idx)?;
    let mut replacement: Vec<Vec<u8>> = Vec::new();
    for line_idx in 0..num_lines {
        let line = patch.line_in_hunk(hunk_idx, line_idx)?;
        match line.origin() {
            ' ' | '+' => replacement.push(line.content().to_owned()),
            _ => {}
        }
    }

    // old_start is 1-based.  For a substitution or deletion (old_count > 0) it
    // is the first line to remove, so the 0-based index is old_start-1.
    // For a pure insertion (old_count == 0) git convention says "insert after
    // line old_start", so the splice point is old_start (0-based).
    let start = if old_count == 0 {
        old_start.min(lines.len())
    } else {
        old_start.saturating_sub(1).min(lines.len())
    };
    let end = (start + old_count).min(lines.len());

    let mut result: Vec<u8> = Vec::new();
    for l in &lines[..start] {
        result.extend_from_slice(l);
    }
    for r in &replacement {
        result.extend_from_slice(r);
    }
    for l in &lines[end..] {
        result.extend_from_slice(l);
    }
    Ok(result)
}

/// Split raw bytes into lines keeping each `\n` terminator attached.
fn split_lines_keep_eol(data: &[u8]) -> Vec<&[u8]> {
    let mut lines: Vec<&[u8]> = Vec::new();
    let mut start = 0;
    for (i, &b) in data.iter().enumerate() {
        if b == b'\n' {
            lines.push(&data[start..=i]);
            start = i + 1;
        }
    }
    if start < data.len() {
        lines.push(&data[start..]);
    }
    lines
}

/// Apply the hunks at `hunk_indices` from `patch` to `content` in a single
/// sweep, using each hunk's original `old_start` position.  Hunks NOT listed
/// in `hunk_indices` are preserved unchanged.
///
/// `hunk_indices` need not be sorted on entry; the function sorts them by
/// `old_start` before processing.
fn apply_multiple_hunks_to_content(
    content: &[u8],
    patch: &mut git2::Patch,
    hunk_indices: &[usize],
) -> Result<Vec<u8>> {
    // Sort by old_start so we can sweep top-to-bottom through the original.
    let mut sorted: Vec<(usize, u32)> = hunk_indices
        .iter()
        .map(|&i| {
            patch
                .hunk(i)
                .map(|(h, _)| (i, h.old_start()))
                .map_err(anyhow::Error::from)
        })
        .collect::<Result<_>>()?;
    sorted.sort_by_key(|&(_, s)| s);

    let lines = split_lines_keep_eol(content);
    let mut result: Vec<u8> = Vec::new();
    let mut src_pos: usize = 0;

    for (hunk_idx, _) in &sorted {
        let (hunk_header, _) = patch.hunk(*hunk_idx)?;
        let old_start = hunk_header.old_start() as usize; // 1-based
        let old_count = hunk_header.old_lines() as usize;

        let splice_start = if old_count == 0 {
            old_start.min(lines.len())
        } else {
            old_start.saturating_sub(1).min(lines.len())
        };

        for line in &lines[src_pos..splice_start] {
            result.extend_from_slice(line);
        }
        src_pos = splice_start + old_count;

        let num_lines = patch.num_lines_in_hunk(*hunk_idx)?;
        for line_idx in 0..num_lines {
            let line = patch.line_in_hunk(*hunk_idx, line_idx)?;
            if matches!(line.origin(), ' ' | '+') {
                result.extend_from_slice(line.content());
            }
        }
    }

    for line in &lines[src_pos..] {
        result.extend_from_slice(line);
    }
    Ok(result)
}

/// Apply a selected subset of hunks from `full_diff` to `parent_tree`,
/// returning the new tree OID.
///
/// `selected_hunks` maps each delta index to the list of hunk indices
/// (within that delta) to apply.  Files with no selected hunks keep their
/// original content from `parent_tree`.
fn apply_selected_hunks_to_tree(
    repo: &git2::Repository,
    parent_tree: &git2::Tree,
    full_diff: &git2::Diff,
    selected_hunks: &HashMap<usize, Vec<usize>>,
) -> Result<git2::Oid> {
    let mut idx = git2::Index::new()?;
    idx.read_tree(parent_tree)?;

    for (&delta_idx, hunk_indices) in selected_hunks {
        if hunk_indices.is_empty() {
            continue;
        }

        let mut patch = match git2::Patch::from_diff(full_diff, delta_idx)? {
            Some(p) => p,
            None => continue,
        };

        let delta = full_diff
            .get_delta(delta_idx)
            .context("delta index in range")?;
        let file_path = delta
            .new_file()
            .path()
            .or_else(|| delta.old_file().path())
            .context("delta has no file path")?
            .to_owned();

        let (old_content, mode) = match delta.status() {
            git2::Delta::Added => {
                let m: u32 = delta.new_file().mode().into();
                (Vec::new(), m)
            }
            _ => {
                let entry = parent_tree
                    .get_path(&file_path)
                    .with_context(|| format!("'{}' not in parent tree", file_path.display()))?;
                let blob = repo.find_blob(entry.id())?;
                (blob.content().to_owned(), entry.filemode() as u32)
            }
        };

        let new_content =
            apply_multiple_hunks_to_content(&old_content, &mut patch, hunk_indices)
                .with_context(|| format!("applying selected hunks to '{}'", file_path.display()))?;

        let path_bytes = file_path
            .to_str()
            .context("file path is not valid UTF-8")?
            .as_bytes()
            .to_vec();

        if delta.status() == git2::Delta::Deleted && new_content.is_empty() {
            // All lines removed → delete the file from the intermediate tree.
            idx.remove(&file_path, 0)?;
        } else {
            let new_blob_oid = repo.blob(&new_content)?;
            idx.add(&git2::IndexEntry {
                ctime: git2::IndexTime::new(0, 0),
                mtime: git2::IndexTime::new(0, 0),
                dev: 0,
                ino: 0,
                mode,
                uid: 0,
                gid: 0,
                file_size: new_content.len() as u32,
                id: new_blob_oid,
                flags: 0,
                flags_extended: 0,
                path: path_bytes,
            })?;
        }
    }

    idx.write_tree_to(repo).map_err(Into::into)
}

impl Git2Repo {
    /// Refuse if any staged or unstaged change touches a file in `commit_paths`.
    fn check_dirty_overlap(&self, commit_paths: &HashSet<String>) -> Result<()> {
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
        Ok(())
    }

    /// Cherry-pick all commits strictly between `stop_oid` (exclusive) and
    /// `head_oid` (inclusive) onto `tip`, returning the new tip OID.
    fn rebase_descendants(
        &self,
        stop_oid: git2::Oid,
        head_oid: git2::Oid,
        mut tip: git2::Oid,
    ) -> Result<git2::Oid> {
        let repo = &self.inner;
        if head_oid == stop_oid {
            return Ok(tip);
        }

        let mut revwalk = repo.revwalk()?;
        revwalk.push(head_oid)?;

        let mut descendants: Vec<git2::Oid> = Vec::new();
        for oid_result in revwalk {
            let oid = oid_result?;
            if oid == stop_oid {
                break;
            }
            descendants.push(oid);
        }
        descendants.reverse();

        for desc_oid in descendants {
            let desc_commit = repo.find_commit(desc_oid)?;
            let onto_commit = repo.find_commit(tip)?;

            let mut cherry_index = repo.cherrypick_commit(&desc_commit, &onto_commit, 0, None)?;
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
            tip = repo.commit(
                None,
                &author,
                &committer,
                desc_commit.message().unwrap_or(""),
                &new_tree,
                &[&onto_commit],
            )?;
        }

        Ok(tip)
    }

    /// Fast-forward the branch ref that HEAD currently points to.
    fn advance_branch_ref(&self, new_tip: git2::Oid, log_msg: &str) -> Result<()> {
        let repo = &self.inner;
        let head_ref = repo.head()?;
        let branch_refname = head_ref
            .resolve()
            .context("HEAD is not a symbolic ref")?
            .name()
            .context("Ref has no name")?
            .to_string();
        repo.reference(&branch_refname, new_tip, true, log_msg)?;
        Ok(())
    }

    /// Collect OIDs strictly between `stop_oid` (exclusive) and `head_oid`
    /// (inclusive), returned oldest-first.
    fn collect_descendants(
        &self,
        stop_oid: git2::Oid,
        head_oid: git2::Oid,
    ) -> Result<Vec<git2::Oid>> {
        let repo = &self.inner;
        if head_oid == stop_oid {
            return Ok(Vec::new());
        }

        let mut revwalk = repo.revwalk()?;
        revwalk.push(head_oid)?;

        let mut descendants: Vec<git2::Oid> = Vec::new();
        for oid_result in revwalk {
            let oid = oid_result?;
            if oid == stop_oid {
                break;
            }
            descendants.push(oid);
        }
        descendants.reverse();
        Ok(descendants)
    }

    /// Cherry-pick a sequence of commits onto `tip`, returning the final tip
    /// or the point at which a conflict was detected.
    ///
    /// On conflict the conflicted index is written to the working tree so the
    /// user can resolve it. The returned `conflicting_idx` identifies which
    /// element of `commits` conflicted.
    fn cherry_pick_chain(
        &self,
        mut tip: git2::Oid,
        commits: &[git2::Oid],
    ) -> Result<CherryPickResult> {
        let repo = &self.inner;

        for (idx, &desc_oid) in commits.iter().enumerate() {
            let desc_commit = repo.find_commit(desc_oid)?;
            let onto_commit = repo.find_commit(tip)?;

            let mut cherry_index = repo.cherrypick_commit(&desc_commit, &onto_commit, 0, None)?;
            if cherry_index.has_conflicts() {
                self.write_conflicts_to_workdir(&cherry_index, &onto_commit)?;
                return Ok(CherryPickResult::Conflict {
                    tip,
                    conflicting_idx: idx,
                });
            }

            let new_tree_oid = cherry_index.write_tree_to(repo)?;
            let new_tree = repo.find_tree(new_tree_oid)?;

            tip = repo.commit(
                None,
                &desc_commit.author(),
                &desc_commit.committer(),
                desc_commit.message().unwrap_or(""),
                &new_tree,
                &[&onto_commit],
            )?;
        }

        Ok(CherryPickResult::Complete(tip))
    }

    /// Write a conflicted merge index to the repo index and working tree so
    /// the user can resolve conflicts manually.
    fn write_conflicts_to_workdir(
        &self,
        cherry_index: &git2::Index,
        onto_commit: &git2::Commit,
    ) -> Result<()> {
        let repo = &self.inner;

        // Point the branch at the onto commit so HEAD matches the partially
        // rebased chain.
        self.advance_branch_ref(onto_commit.id(), "git-tailor: drop commit (conflict)")?;

        // Write the conflicted index entries (including conflict markers) into
        // the repo's index so `git status` and the user's editor see them.
        let mut repo_index = repo.index()?;
        for entry in cherry_index.iter() {
            // Stage 0 = normal, stages 1-3 = conflict (base/ours/theirs).
            // We need to preserve all stages so the user's tools can resolve.
            repo_index.add(&entry)?;
        }
        repo_index.write()?;

        // Check out the index to the working tree. Force-checkout writes
        // conflict markers into the working-tree files.
        let mut checkout = git2::build::CheckoutBuilder::new();
        checkout.force();
        checkout.allow_conflicts(true);
        repo.checkout_index(Some(&mut repo_index), Some(&mut checkout))?;

        Ok(())
    }

    /// Reset the working tree and index to match HEAD.
    fn checkout_head(&self) -> Result<()> {
        let mut checkout = git2::build::CheckoutBuilder::new();
        checkout.force();
        self.inner.checkout_head(Some(&mut checkout))?;
        Ok(())
    }
}

/// Internal result from `cherry_pick_chain`.
enum CherryPickResult {
    Complete(git2::Oid),
    Conflict {
        tip: git2::Oid,
        conflicting_idx: usize,
    },
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
        author: commit.author().name().map(|s| s.to_string()),
        date: Some(commit.time().seconds().to_string()),
        parent_oids: commit.parent_ids().map(|id| id.to_string()).collect(),
        message: commit.message().unwrap_or("").to_string(),
        author_email: commit.author().email().map(|s| s.to_string()),
        author_date: Some(git_time_to_offset_datetime(author_time)),
        committer: commit.committer().name().map(|s| s.to_string()),
        committer_email: commit.committer().email().map(|s| s.to_string()),
        commit_date: Some(git_time_to_offset_datetime(commit_time)),
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
