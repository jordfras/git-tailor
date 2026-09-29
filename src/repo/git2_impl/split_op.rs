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

//! Split a single commit into multiple commits — per file, per hunk, or per
//! hunk-group (where groups come from the fragmap clustering).  Also exposes
//! "count" helpers used by the UI to decide whether each strategy is
//! applicable.

use anyhow::{Context, Result};
use bstr::{BStr, BString, ByteSlice};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use crate::{Oid, fragmap};

use super::Git2Repo;
use super::hunks;
use super::reads;
use super::reword_op;

pub(super) fn split_commit_per_file(
    repo: &mut Git2Repo,
    commit_oid: &Oid,
    head_oid: &Oid,
) -> Result<()> {
    let target = load_split_commit(repo, commit_oid)?;

    let full_diff =
        repo.inner
            .diff_tree_to_tree(Some(&target.parent_tree), Some(&target.commit_tree), None)?;
    let file_deltas = file_pieces(&full_diff);
    let file_count = file_deltas.len();

    if file_count < 2 {
        anyhow::bail!("Commit touches fewer than 2 files — nothing to split");
    }

    repo.check_dirty_overlap(&collect_commit_paths(&full_diff, true))?;

    let mut current_base = initial_split_base(&target.commit)?;
    let mut current_tree_oid = target.parent_tree.id();
    for (piece_idx, &delta_idx) in file_deltas.iter().enumerate() {
        // The last piece takes the original tree verbatim rather than the
        // accumulated one, so the chain provably ends where the original commit
        // did — which is what lets the descendant replay be conflict-free. The
        // other strategies already do this.
        let new_tree_oid = if piece_idx == file_count - 1 {
            target.commit_tree.id()
        } else {
            let delta = full_diff.get_delta(delta_idx).expect("delta index valid");
            let base_tree = repo.inner.find_tree(current_tree_oid)?;
            hunks::apply_whole_deltas_to_tree(&repo.inner, &base_tree, [delta])?
        };

        current_base = Some(commit_split_piece(
            repo,
            &target.commit,
            new_tree_oid,
            current_base,
            piece_idx + 1,
            file_count,
        )?);
        current_tree_oid = new_tree_oid;
    }

    // `target`'s handles and the diff borrow the repository, and all of them
    // have destructors, so those borrows last until they are dropped — which
    // has to happen before `finalize_split` takes the repository mutably.
    let split_commit_oid = target.commit_oid;
    drop((target, full_diff));

    finalize_split(
        repo,
        split_commit_oid,
        head_oid,
        current_base.expect("loop produced at least two commits"),
        "git-tailor: split per-file",
    )
}

pub(super) fn split_commit_per_hunk(
    repo: &mut Git2Repo,
    commit_oid: &Oid,
    head_oid: &Oid,
) -> Result<()> {
    let target = load_split_commit(repo, commit_oid)?;

    let mut diff_opts = zero_context_diff_opts();
    let full_diff = repo.inner.diff_tree_to_tree(
        Some(&target.parent_tree),
        Some(&target.commit_tree),
        Some(&mut diff_opts),
    )?;

    let hunk_count = count_hunks(&full_diff)?;
    let hunkless = hunkless_deltas(&full_diff)?;
    let piece_count = hunk_count + hunkless.len();
    if piece_count < 2 {
        anyhow::bail!("Commit has fewer than 2 hunks — nothing to split per hunk");
    }

    repo.check_dirty_overlap(&collect_commit_paths(&full_diff, false))?;

    // Build one commit per hunk using incremental blob manipulation.  At each
    // step, recompute diff(current_tree → commit_tree) with 0 context and
    // apply exactly its first hunk directly to the blob — bypassing
    // apply_to_tree to avoid libgit2 validating rejected hunks against the
    // modified output buffer (which shifts line positions and causes "hunk
    // did not apply"). Changes with no hunk follow, one piece each.
    let mut current_base = initial_split_base(&target.commit)?;
    let mut current_tree_oid = target.parent_tree.id();
    for target_k in 0..piece_count {
        let next_tree_oid = if target_k == piece_count - 1 {
            target.commit_tree.id()
        } else if let Some(&delta_idx) = target_k
            .checked_sub(hunk_count)
            .and_then(|i| hunkless.get(i))
        {
            let current_tree = repo.inner.find_tree(current_tree_oid)?;
            let delta = full_diff
                .get_delta(delta_idx)
                .context("delta index in range")?;
            hunks::apply_whole_deltas_to_tree(&repo.inner, &current_tree, [delta])?
        } else {
            let current_tree = repo.inner.find_tree(current_tree_oid)?;
            let mut diff_opts = zero_context_diff_opts();
            let incremental_diff = repo.inner.diff_tree_to_tree(
                Some(&current_tree),
                Some(&target.commit_tree),
                Some(&mut diff_opts),
            )?;
            hunks::apply_single_hunk_to_tree(&repo.inner, &current_tree, &incremental_diff)
                .with_context(|| format!("applying hunk {}", target_k + 1))?
        };

        current_base = Some(commit_split_piece(
            repo,
            &target.commit,
            next_tree_oid,
            current_base,
            target_k + 1,
            piece_count,
        )?);
        current_tree_oid = next_tree_oid;
    }

    // `target`'s handles and the diff borrow the repository, and all of them
    // have destructors, so those borrows last until they are dropped — which
    // has to happen before `finalize_split` takes the repository mutably.
    let split_commit_oid = target.commit_oid;
    drop((target, full_diff));

    finalize_split(
        repo,
        split_commit_oid,
        head_oid,
        current_base.expect("loop produced at least two commits"),
        "git-tailor: split per-hunk",
    )
}

pub(super) fn split_commit_per_hunk_group(
    repo: &mut Git2Repo,
    commit_oid: &Oid,
    head_oid: &Oid,
    reference_oid: &Oid,
) -> Result<()> {
    let target = load_split_commit(repo, commit_oid)?;

    // Build the fragmap over all branch commits so hunk grouping reflects how
    // this commit interacts with its neighbors in the branch.  In --all mode
    // the root commit IS the reference point, so the commit being split must
    // be kept even when it equals `reference_oid`.
    let assignment = compute_hunk_group_assignment(repo, commit_oid, head_oid, reference_oid)?;

    let full_diff = hunk_group_diff(repo, &target)?;

    repo.check_dirty_overlap(&collect_commit_paths(&full_diff, false))?;

    let delta_hunk_assignments = delta_hunk_assignments(&assignment, &full_diff)?;
    let k_groups = touched_groups(&delta_hunk_assignments);
    // No fragmap column claims a change without hunks, so they get one piece
    // of their own after the groups.
    let has_hunkless = !hunkless_deltas(&full_diff)?.is_empty();
    let split_count = k_groups.len() + usize::from(has_hunkless);

    if split_count < 2 {
        anyhow::bail!("Commit has fewer than 2 hunk groups — nothing to split per hunk group");
    }

    // For each touched group gk (in order), build the intermediate tree by
    // applying all of K's hunks — and fragments of hunks — whose group index
    // ≤ gk to parent_tree in one sweep (positions relative to the original,
    // no cumulative offset issues).
    let mut current_base = initial_split_base(&target.commit)?;
    for out_pos in 0..split_count {
        let next_tree_oid = if out_pos == split_count - 1 {
            target.commit_tree.id()
        } else {
            let gk = *k_groups
                .get(out_pos)
                .expect("only the last piece has no group");
            let mut selected: BTreeMap<usize, hunks::DeltaSelection> = BTreeMap::new();
            for (delta_idx, hunk_assignments) in delta_hunk_assignments.iter().enumerate() {
                let chosen: Vec<hunks::HunkSelection> = hunk_assignments
                    .iter()
                    .enumerate()
                    .filter_map(|(h, hunk_assignment)| {
                        hunk_selection_for_prefix(h, hunk_assignment, gk)
                    })
                    .collect();
                if !chosen.is_empty() {
                    selected.insert(delta_idx, hunks::DeltaSelection::Hunks(chosen));
                }
            }
            hunks::apply_selected_hunks_to_tree(
                &repo.inner,
                &target.parent_tree,
                &full_diff,
                &selected,
            )
            .with_context(|| format!("building tree for hunk group {}", out_pos + 1))?
        };

        current_base = Some(commit_split_piece(
            repo,
            &target.commit,
            next_tree_oid,
            current_base,
            out_pos + 1,
            split_count,
        )?);
    }

    // `target`'s handles and the diff borrow the repository, and all of them
    // have destructors, so those borrows last until they are dropped — which
    // has to happen before `finalize_split` takes the repository mutably.
    let split_commit_oid = target.commit_oid;
    drop((target, full_diff));

    finalize_split(
        repo,
        split_commit_oid,
        head_oid,
        current_base.expect("loop produced at least two commits"),
        "git-tailor: split per-hunk-group",
    )
}

pub(super) fn count_split_per_file(repo: &Git2Repo, commit_oid: &Oid) -> Result<usize> {
    let target = load_split_commit(repo, commit_oid)?;
    let diff =
        repo.inner
            .diff_tree_to_tree(Some(&target.parent_tree), Some(&target.commit_tree), None)?;
    Ok(file_pieces(&diff).len())
}

/// The deltas a per-file split gives a piece each: all but the replaced
/// deletions, which go with their addition.
fn file_pieces(diff: &git2::Diff<'_>) -> Vec<usize> {
    let replaced = hunks::replaced_deletions(diff);
    (0..diff.deltas().len())
        .filter(|delta_idx| !replaced.contains_key(delta_idx))
        .collect()
}

pub(super) fn count_split_per_hunk(repo: &Git2Repo, commit_oid: &Oid) -> Result<usize> {
    let target = load_split_commit(repo, commit_oid)?;
    let mut diff_opts = zero_context_diff_opts();
    let diff = repo.inner.diff_tree_to_tree(
        Some(&target.parent_tree),
        Some(&target.commit_tree),
        Some(&mut diff_opts),
    )?;
    Ok(count_hunks(&diff)? + hunkless_deltas(&diff)?.len())
}

pub(super) fn count_split_per_hunk_group(
    repo: &Git2Repo,
    commit_oid: &Oid,
    head_oid: &Oid,
    reference_oid: &Oid,
) -> Result<usize> {
    let assignment = compute_hunk_group_assignment(repo, commit_oid, head_oid, reference_oid)?;
    let target = load_split_commit(repo, commit_oid)?;
    let diff = hunk_group_diff(repo, &target)?;
    let groups = touched_groups(&delta_hunk_assignments(&assignment, &diff)?);
    let has_hunkless = !hunkless_deltas(&diff)?.is_empty();
    Ok(groups.len() + usize::from(has_hunkless))
}

/// Each delta's hunk assignments, indexed by delta then hunk: a whole-hunk
/// group, or per-fragment groups when the hunk spans several columns. A
/// replaced deletion gets none, since its addition does its work.
fn delta_hunk_assignments(
    assignment: &fragmap::HunkGroupAssignment,
    diff: &git2::Diff<'_>,
) -> Result<Vec<Vec<fragmap::HunkAssignment>>> {
    let replaced = hunks::replaced_deletions(diff);
    hunk_counts(diff)?
        .into_iter()
        .enumerate()
        .map(|(delta_idx, num_hunks)| {
            if replaced.contains_key(&delta_idx) {
                return Ok(Vec::new());
            }
            let delta = diff.get_delta(delta_idx).context("delta index")?;
            let file_assignments = assignment
                .by_file
                .get(&delta_path(&delta).unwrap_or_default());
            Ok((0..num_hunks)
                .map(|h| {
                    file_assignments
                        .and_then(|fa| fa.get(h))
                        .cloned()
                        .unwrap_or(fragmap::HunkAssignment::Whole { group: 0 })
                })
                .collect())
        })
        .collect()
}

/// The groups `delta_hunk_assignments` touch, in order: only these produce
/// pieces, not every column the full fragmap has.
fn touched_groups(delta_hunk_assignments: &[Vec<fragmap::HunkAssignment>]) -> Vec<usize> {
    let touched: BTreeSet<usize> = delta_hunk_assignments
        .iter()
        .flatten()
        .flat_map(|hunk_assignment| hunk_assignment.groups())
        .collect();
    touched.into_iter().collect()
}

/// The 0-context diff a per-hunk-group split works from, whose hunk indices
/// line up with the fragmap's assignment. That assignment detects renames, so
/// this has to as well: otherwise a renamed file shows up as an unrelated
/// delete+add pair, and its hunk indices (and even its path) no longer match.
fn hunk_group_diff<'r>(repo: &'r Git2Repo, target: &SplitTarget<'r>) -> Result<git2::Diff<'r>> {
    let mut diff_opts = zero_context_diff_opts();
    let mut diff = repo.inner.diff_tree_to_tree(
        Some(&target.parent_tree),
        Some(&target.commit_tree),
        Some(&mut diff_opts),
    )?;
    diff.find_similar(None)?;
    Ok(diff)
}

/// Peel a set of selected files out of `commit_oid` into a follow-up commit,
/// keeping everything else in the first (original-message) commit. A picked
/// file takes along any deletion it replaces.
pub(super) fn split_commit_out_files(
    repo: &mut Git2Repo,
    commit_oid: &Oid,
    file_paths: &[PathBuf],
    head_oid: &Oid,
) -> Result<()> {
    if file_paths.is_empty() {
        anyhow::bail!("No files selected — nothing to split out");
    }

    let target = load_split_commit(repo, commit_oid)?;

    let full_diff =
        repo.inner
            .diff_tree_to_tree(Some(&target.parent_tree), Some(&target.commit_tree), None)?;
    let file_count = full_diff.deltas().len();

    let chosen = chosen_file_deltas(&full_diff, file_paths)?;
    if chosen.len() >= file_count {
        anyhow::bail!("Every file is selected — nothing would remain in the original commit");
    }

    repo.check_dirty_overlap(&collect_commit_paths(&full_diff, true))?;

    // The first commit keeps every change except the selected files: the full
    // commit tree with each of them reverted, which can never conflict.
    let chosen_deltas = chosen
        .iter()
        .map(|&delta_idx| {
            full_diff
                .get_delta(delta_idx)
                .context("delta index in range")
        })
        .collect::<Result<Vec<_>>>()?;
    let rest_tree_oid =
        hunks::revert_deltas_in_tree(&repo.inner, &target.commit_tree, &chosen_deltas)?;

    let base = initial_split_base(&target.commit)?;
    let original_message = target.commit.message_bytes().as_bstr();
    let first = commit_with_message(
        repo,
        &target.commit,
        rest_tree_oid,
        base,
        original_message,
        reword_op::encoding_for(&target.commit, original_message),
    )?;

    let suffix = if file_paths.len() == 1 {
        BString::from(crate::domain::path_to_bytes(&file_paths[0]))
    } else {
        BString::from(format!("{} files", file_paths.len()))
    };
    let peeled_message = hunks::summary_suffix_message(original_message, suffix.as_bstr());
    let second = commit_with_message(
        repo,
        &target.commit,
        target.commit_tree.id(),
        Some(first),
        peeled_message.as_bstr(),
        suffixed_encoding(&target.commit, suffix.as_bstr(), peeled_message.as_bstr()),
    )?;

    // `target`'s handles and the diff borrow the repository, and all of them
    // have destructors, so those borrows last until they are dropped — which
    // has to happen before `finalize_split` takes the repository mutably.
    let split_commit_oid = target.commit_oid;
    drop((target, full_diff));

    finalize_split(
        repo,
        split_commit_oid,
        head_oid,
        second,
        "git-tailor: split out files",
    )
}

pub(super) fn split_commit_out_hunks(
    repo: &mut Git2Repo,
    commit_oid: &Oid,
    hunks: &[(usize, usize)],
    head_oid: &Oid,
    context_lines: u32,
) -> Result<()> {
    if hunks.is_empty() {
        anyhow::bail!("No hunks selected — nothing to split out");
    }

    let target = load_split_commit(repo, commit_oid)?;

    // Must match the context level the caller used to derive `hunks`'
    // (delta_idx, hunk_idx) pairs — more context can merge adjacent hunks
    // into one, shifting indices relative to a differently-configured diff.
    let mut diff_opts = git2::DiffOptions::new();
    diff_opts.context_lines(context_lines);
    let full_diff = repo.inner.diff_tree_to_tree(
        Some(&target.parent_tree),
        Some(&target.commit_tree),
        Some(&mut diff_opts),
    )?;

    let hunk_counts = hunk_counts(&full_diff)?;
    let selected: HashSet<(usize, usize)> = hunks.iter().copied().collect();
    let replaced = hunks::replaced_deletions(&full_diff);
    validate_hunk_selection(&selected, &hunk_counts, &replaced)?;

    repo.check_dirty_overlap(&collect_commit_paths(&full_diff, false))?;

    let rest_tree_oid = rest_tree(
        &repo.inner,
        &target.parent_tree,
        &full_diff,
        &hunk_counts,
        &replaced,
        &selected,
    )?;

    // Two-tree trick: since `rest_tree_oid` already excludes the selected
    // hunks, replaying the full original tree back in on top represents
    // exactly those hunks' changes — no second apply_selected_hunks_to_tree
    // call needed (mirrors split_commit_out_files' own use of the same trick).
    let base = initial_split_base(&target.commit)?;
    let original_message = target.commit.message_bytes().as_bstr();
    let first = commit_with_message(
        repo,
        &target.commit,
        rest_tree_oid,
        base,
        original_message,
        reword_op::encoding_for(&target.commit, original_message),
    )?;

    let suffix = hunk_selection_suffix(&full_diff, &selected)?;
    let peeled_message = hunks::summary_suffix_message(original_message, suffix.as_bstr());
    let second = commit_with_message(
        repo,
        &target.commit,
        target.commit_tree.id(),
        Some(first),
        peeled_message.as_bstr(),
        suffixed_encoding(&target.commit, suffix.as_bstr(), peeled_message.as_bstr()),
    )?;

    // `target`'s handles and the diff borrow the repository, and all of them
    // have destructors, so those borrows last until they are dropped — which
    // has to happen before `finalize_split` takes the repository mutably.
    let split_commit_oid = target.commit_oid;
    drop((target, full_diff));

    finalize_split(
        repo,
        split_commit_oid,
        head_oid,
        second,
        "git-tailor: split out hunks",
    )
}

/// Refuse a selection naming a hunk beyond `hunk_counts`, one that picks
/// nothing but replaced deletions, or one that would leave nothing in the
/// original commit.
fn validate_hunk_selection(
    selected: &HashSet<(usize, usize)>,
    hunk_counts: &[usize],
    replaced: &BTreeMap<usize, usize>,
) -> Result<()> {
    for &(delta_idx, hunk_idx) in selected {
        let valid = hunk_counts.get(delta_idx).is_some_and(|&n| hunk_idx < n);
        if !valid {
            anyhow::bail!("Invalid hunk selection: delta {delta_idx}, hunk {hunk_idx}");
        }
    }
    let picked = selected
        .iter()
        .filter(|(delta_idx, _)| !replaced.contains_key(delta_idx))
        .count();
    if picked == 0 {
        anyhow::bail!(
            "The selected hunks only delete what another change replaces — nothing to split out"
        );
    }
    let mut total_hunks = 0;
    let mut has_hunkless = false;
    for (delta_idx, &num_hunks) in hunk_counts.iter().enumerate() {
        if !replaced.contains_key(&delta_idx) {
            total_hunks += num_hunks;
            has_hunkless |= num_hunks == 0;
        }
    }
    if picked >= total_hunks && !has_hunkless {
        anyhow::bail!("Every hunk is selected — nothing would remain in the original commit");
    }
    Ok(())
}

/// The tree of everything in `diff` that `selected` leaves behind: the
/// unselected hunks, and every hunkless change, since nobody can pick one.
///
/// Every delta but a replaced deletion gets an entry, even one with all its
/// hunks unselected: a delta absent from the map keeps its parent-tree content,
/// which would revert the file. A replaced deletion stays out, so it happens
/// exactly when its addition does.
fn rest_tree(
    repo: &git2::Repository,
    parent_tree: &git2::Tree<'_>,
    diff: &git2::Diff<'_>,
    hunk_counts: &[usize],
    replaced: &BTreeMap<usize, usize>,
    selected: &HashSet<(usize, usize)>,
) -> Result<git2::Oid> {
    let rest: BTreeMap<usize, hunks::DeltaSelection> = hunk_counts
        .iter()
        .enumerate()
        .filter(|(delta_idx, _)| !replaced.contains_key(delta_idx))
        .map(|(delta_idx, &num_hunks)| {
            if num_hunks == 0 {
                return (delta_idx, hunks::DeltaSelection::Whole);
            }
            let unselected = (0..num_hunks)
                .filter(|hunk_idx| !selected.contains(&(delta_idx, *hunk_idx)))
                .map(|hunk_idx| hunks::HunkSelection::Whole { hunk_idx })
                .collect();
            (delta_idx, hunks::DeltaSelection::Hunks(unselected))
        })
        .collect();
    hunks::apply_selected_hunks_to_tree(repo, parent_tree, diff, &rest)
}

/// The deltas of `diff` that `file_paths` pick, with each deletion a picked
/// addition replaces. Picking only the replaced deletion is refused.
fn chosen_file_deltas(diff: &git2::Diff<'_>, file_paths: &[PathBuf]) -> Result<BTreeSet<usize>> {
    let file_count = diff.deltas().len();
    let mut chosen: BTreeSet<usize> = BTreeSet::new();
    for path in file_paths {
        let before = chosen.len();
        chosen.extend((0..file_count).filter(|&delta_idx| {
            diff.get_delta(delta_idx)
                .is_some_and(|delta| delta_path(&delta).as_deref() == Some(path.as_path()))
        }));
        if chosen.len() == before {
            anyhow::bail!("File not changed by this commit: {}", path.display());
        }
    }
    let delta_display = |delta_idx: usize| {
        diff.get_delta(delta_idx)
            .and_then(|delta| delta_path(&delta))
            .unwrap_or_default()
            .display()
            .to_string()
    };
    for (&deletion, &addition) in &hunks::replaced_deletions(diff) {
        match (chosen.contains(&deletion), chosen.contains(&addition)) {
            (true, false) => anyhow::bail!(
                "{} is replaced by {} — pick that instead",
                delta_display(deletion),
                delta_display(addition)
            ),
            (false, true) => {
                chosen.insert(deletion);
            }
            _ => {}
        }
    }
    Ok(chosen)
}

/// Build the "(...)" suffix for the split-out commit's summary: the touched
/// file's name when the selection is confined to one file (matching
/// `split_commit_out_files`' style), or a hunk/file count otherwise.
fn hunk_selection_suffix(
    full_diff: &git2::Diff,
    selected: &HashSet<(usize, usize)>,
) -> Result<BString> {
    let touched_deltas: BTreeSet<usize> =
        selected.iter().map(|&(delta_idx, _)| delta_idx).collect();
    if touched_deltas.len() == 1 {
        let delta_idx = *touched_deltas.iter().next().expect("checked len == 1");
        let delta = full_diff
            .get_delta(delta_idx)
            .context("delta index in range")?;
        return Ok(BString::from(crate::domain::path_to_bytes(
            &delta_path(&delta).unwrap_or_default(),
        )));
    }
    Ok(BString::from(format!(
        "{} hunks across {} files",
        selected.len(),
        touched_deltas.len()
    )))
}

/// Resolved inputs to a split operation.  `commit_oid` is the parsed form of
/// the caller's `&str` argument; `parent_tree` is an empty tree when `commit`
/// is a root commit.
struct SplitTarget<'r> {
    commit_oid: git2::Oid,
    commit: git2::Commit<'r>,
    parent_tree: git2::Tree<'r>,
    commit_tree: git2::Tree<'r>,
}

/// Parse `commit_oid`, look up the commit, validate it's not a merge, and
/// load the parent and commit trees.
fn load_split_commit<'r>(repo: &'r Git2Repo, commit_oid: &Oid) -> Result<SplitTarget<'r>> {
    let oid = git2::Oid::from(commit_oid);
    let commit = repo.inner.find_commit(oid)?;
    if commit.parent_count() > 1 {
        anyhow::bail!("Cannot split a merge commit");
    }
    if commit.parent_count() == 0 {
        // The first piece would become an orphan root, which behind a graft
        // severs the branch from the history that was never fetched.
        repo.refuse_shallow_root(oid)?;
    }
    let parent_tree = if commit.parent_count() == 0 {
        repo.empty_tree()?
    } else {
        commit.parent(0)?.tree()?
    };
    let commit_tree = commit.tree()?;
    Ok(SplitTarget {
        commit_oid: oid,
        commit,
        parent_tree,
        commit_tree,
    })
}

/// Parent OID for the first split piece, or `None` for a root commit (the
/// first piece becomes a new orphan root and subsequent pieces stack on it).
fn initial_split_base(commit: &git2::Commit<'_>) -> Result<Option<git2::Oid>> {
    if commit.parent_count() == 0 {
        Ok(None)
    } else {
        Ok(Some(commit.parent_id(0)?))
    }
}

/// Collect the file paths touched by `diff`.  When `exclude_gitlinks` is set,
/// submodule-pointer deltas are skipped — used by the per-file split path
/// which applies them via tree manipulation only and so cannot be tripped by
/// a dirty submodule state.
fn collect_commit_paths(diff: &git2::Diff<'_>, exclude_gitlinks: bool) -> HashSet<PathBuf> {
    diff.deltas()
        .filter(|d| {
            !exclude_gitlinks
                || (d.new_file().mode() != git2::FileMode::Commit
                    && d.old_file().mode() != git2::FileMode::Commit)
        })
        .filter_map(|d| {
            d.new_file()
                .path()
                .or_else(|| d.old_file().path())
                .map(Path::to_path_buf)
        })
        .collect()
}

/// `DiffOptions` configured to keep adjacent hunks separate (no surrounding
/// or inter-hunk context).
fn zero_context_diff_opts() -> git2::DiffOptions {
    let mut opts = git2::DiffOptions::new();
    opts.context_lines(0);
    opts.interhunk_lines(0);
    opts
}

/// Indices of the deltas with no hunk: a binary file, an empty file, a mode
/// change. No hunk-level split can select them, so each strategy has to place
/// them deliberately. A replaced deletion is left out: it goes with its
/// addition.
fn hunkless_deltas(diff: &git2::Diff<'_>) -> Result<Vec<usize>> {
    let replaced = hunks::replaced_deletions(diff);
    let hunkless = hunk_counts(diff)?
        .into_iter()
        .enumerate()
        .filter(|&(delta_idx, num_hunks)| num_hunks == 0 && !replaced.contains_key(&delta_idx))
        .map(|(delta_idx, _)| delta_idx)
        .collect();
    Ok(hunkless)
}

/// The number of hunks in each of `diff`'s deltas, in delta order.
fn hunk_counts(diff: &git2::Diff<'_>) -> Result<Vec<usize>> {
    (0..diff.deltas().len())
        .map(|delta_idx| Ok(git2::Patch::from_diff(diff, delta_idx)?.map_or(0, |p| p.num_hunks())))
        .collect()
}

/// Total hunk count across all files in `diff`, leaving out replaced
/// deletions: their addition's hunks do their work.
fn count_hunks(diff: &git2::Diff<'_>) -> Result<usize> {
    let replaced = hunks::replaced_deletions(diff);
    let count = hunk_counts(diff)?
        .into_iter()
        .enumerate()
        .filter(|(delta_idx, _)| !replaced.contains_key(delta_idx))
        .map(|(_, num_hunks)| num_hunks)
        .sum();
    Ok(count)
}

/// The selection of hunk `hunk_idx` for the intermediate tree containing all
/// groups ≤ `gk`: the whole hunk, a partial selection of its fragments, or
/// `None` when nothing of it belongs yet.
fn hunk_selection_for_prefix(
    hunk_idx: usize,
    hunk_assignment: &fragmap::HunkAssignment,
    gk: usize,
) -> Option<hunks::HunkSelection> {
    match hunk_assignment {
        fragmap::HunkAssignment::Whole { group } => {
            (*group <= gk).then_some(hunks::HunkSelection::Whole { hunk_idx })
        }
        fragmap::HunkAssignment::Fragmented { fragments } => {
            if !fragments.iter().any(|f| f.group <= gk) {
                return None;
            }
            if fragments.iter().all(|f| f.group <= gk) {
                return Some(hunks::HunkSelection::Whole { hunk_idx });
            }
            Some(hunks::HunkSelection::Partial {
                hunk_idx,
                fragments: fragments
                    .iter()
                    .map(|f| hunks::FragmentSelection {
                        fragment: f.fragment,
                        selected: f.group <= gk,
                    })
                    .collect(),
            })
        }
    }
}

/// Run the fragmap hunk-group clustering for the branch `head_oid..reference_oid`
/// and return the per-file group assignment for `commit_oid`.
///
/// `commit_oid` is kept even when it equals `reference_oid`, which in `--all`
/// mode it does for the root commit: dropping it would take the commit being
/// assigned out of the list it is then looked up in.
fn compute_hunk_group_assignment(
    repo: &Git2Repo,
    commit_oid: &Oid,
    head_oid: &Oid,
    reference_oid: &Oid,
) -> Result<fragmap::HunkGroupAssignment> {
    let branch_commits = reads::list_commits(repo, head_oid, reference_oid)?;
    let branch_diffs: Vec<crate::CommitDiff> = branch_commits
        .iter()
        .filter(|c| {
            let is_reference = c.oid.as_oid() == Some(reference_oid);
            let is_split_commit = c.oid.as_oid() == Some(commit_oid);
            (!is_reference || is_split_commit) && !c.oid.is_synthetic()
        })
        .map(|c| {
            c.oid
                .as_oid()
                .map(|oid| reads::commit_diff_for_fragmap(repo, oid))
                .transpose()
        })
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect();

    fragmap::assign_hunk_groups(&branch_diffs, commit_oid)
        .ok_or_else(|| anyhow::anyhow!("Commit {} not found in branch diff list", commit_oid))
}

/// Create one piece of a split: a commit with the given tree, parented on
/// `current_base` (or as an orphan root when `None`), inheriting author and
/// committer from `original` and using a numbered split message.
fn commit_split_piece(
    repo: &Git2Repo,
    original: &git2::Commit<'_>,
    new_tree_oid: git2::Oid,
    current_base: Option<git2::Oid>,
    piece_num: usize,
    total_pieces: usize,
) -> Result<git2::Oid> {
    let message = hunks::split_message(original.message_bytes().as_bstr(), piece_num, total_pieces);
    commit_with_message(
        repo,
        original,
        new_tree_oid,
        current_base,
        message.as_bstr(),
        // "(n/total)" is ASCII, which is all `suffixed_encoding` needs to know.
        original.message_encoding().ok().flatten(),
    )
}

/// The `encoding` header for `original`'s message with `suffix` appended.
///
/// ASCII reads the same in every encoding git accepts, so an ASCII suffix
/// leaves the message in the original's encoding — even when the result also
/// happens to parse as UTF-8.
fn suffixed_encoding<'c>(
    original: &'c git2::Commit<'_>,
    suffix: &BStr,
    message: &BStr,
) -> Option<&'c str> {
    if suffix.is_ascii() {
        original.message_encoding().ok().flatten()
    } else {
        reword_op::encoding_for(original, message)
    }
}

/// Create a commit with the given tree and message, parented on `current_base`
/// (or as an orphan root when `None`), inheriting author and committer from
/// `original`.
fn commit_with_message(
    repo: &Git2Repo,
    original: &git2::Commit<'_>,
    new_tree_oid: git2::Oid,
    current_base: Option<git2::Oid>,
    message: &BStr,
    encoding: Option<&str>,
) -> Result<git2::Oid> {
    let new_tree = repo.inner.find_tree(new_tree_oid)?;
    let parents: Vec<git2::Commit> = match current_base {
        Some(oid) => vec![repo.inner.find_commit(oid)?],
        None => vec![],
    };
    let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
    repo.commit_preserving_message(
        &original.author(),
        &original.committer(),
        message,
        encoding,
        &new_tree,
        &parent_refs,
    )
}

/// New-or-old path of a delta, owned.
fn delta_path(delta: &git2::DiffDelta<'_>) -> Option<PathBuf> {
    delta
        .new_file()
        .path()
        .or_else(|| delta.old_file().path())
        .map(Path::to_path_buf)
}

/// Replay descendants of the split commit onto the last split piece and
/// fast-forward the branch ref.
fn finalize_split(
    repo: &mut Git2Repo,
    original_commit_oid: git2::Oid,
    head_oid: &Oid,
    final_tip: git2::Oid,
    log_msg: &str,
) -> Result<()> {
    let head_git_oid = git2::Oid::from(head_oid);
    if repo.range_has_merge(Some(original_commit_oid), head_git_oid)? {
        anyhow::bail!("Cannot split: a merge commit lies between this commit and HEAD");
    }
    let rebased_tip =
        repo.replay_descendants_conflict_free(original_commit_oid, head_git_oid, final_tip)?;
    repo.advance_branch_ref(rebased_tip, log_msg)?;
    Ok(())
}
