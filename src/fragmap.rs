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

// Fragmap: chunk clustering for visualizing commit relationships
//
// Uses span propagation to map all hunk positions to a common reference
// frame (the final file version at HEAD), matching the algorithm from the
// original fragmap tool. Without propagation, line numbers from different
// commits refer to different file versions and cannot be compared directly.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::{CommitDiff, Oid, VirtualOid};

mod assignment;
mod attribution;
mod lineage;
mod position;
mod spg;
pub use assignment::{
    FragmentAssignment, HunkAssignment, HunkFragment, HunkGroupAssignment, LineRange,
};
pub use lineage::FileId;
use lineage::FileLineages;
pub use position::{ChangePos, CommitPos, HunkPos};
use spg::{build_file_clusters, build_file_clusters_with_target, deduplicate_clusters};

/// Collect each file's hunks per commit.
///
/// This is the shared grouping logic used by [`build_fragmap`],
/// [`assign_hunk_groups`], and [`dump_per_file_spg_stats`].
fn collect_file_commits(
    commit_diffs: &[CommitDiff],
    lineages: &FileLineages,
) -> HashMap<FileId, Vec<(CommitPos, Vec<HunkInfo>)>> {
    let mut file_commits: HashMap<FileId, Vec<(CommitPos, Vec<HunkInfo>)>> = HashMap::new();
    for (commit, diff) in commit_diffs.iter().enumerate() {
        let commit = CommitPos(commit);
        for (change, file) in diff.files.iter().enumerate() {
            if file.new_path.is_none() || file.hunks.is_empty() {
                continue;
            }
            let hunks: Vec<HunkInfo> = file
                .hunks
                .iter()
                .map(|h| HunkInfo {
                    old_start: h.old_start,
                    old_lines: h.old_lines,
                    new_start: h.new_start,
                    new_lines: h.new_lines,
                })
                .collect();
            let entry = file_commits
                .entry(lineages.of(commit, ChangePos(change)))
                .or_default();
            match entry.last_mut() {
                Some((last, earlier)) if *last == commit => earlier.extend(hunks),
                _ => entry.push((commit, hunks)),
            }
        }
    }
    file_commits
}

/// A span of line numbers within a specific file.
///
/// Represents a contiguous range of lines that were touched by a commit,
/// propagated forward to the final file version so all spans share the
/// same reference frame. This allows overlap-based clustering to correctly
/// detect which commits touch related code regions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSpan {
    /// The path the file was first seen under.
    pub path: PathBuf,
    /// The file, which the path alone does not identify once a path is reused.
    pub file: FileId,
    /// First line number (1-indexed) in the range.
    pub start_line: u32,
    /// Last line number (1-indexed) in the range, inclusive.
    pub end_line: u32,
}

/// Lightweight copy of the hunk header fields needed for propagation.
#[derive(Debug, Clone)]
struct HunkInfo {
    old_start: u32,
    old_lines: u32,
    new_start: u32,
    new_lines: u32,
}

/// (legacy) Extract FileSpans from a single commit diff without propagation.
/// Kept for tests that operate on individual commits.
#[cfg(test)]
fn extract_spans(commit_diff: &CommitDiff) -> Vec<FileSpan> {
    let mut spans = Vec::new();

    for (file_idx, file) in commit_diff.files.iter().enumerate() {
        let path = match &file.new_path {
            Some(p) => p.clone(),
            None => continue,
        };

        for hunk in &file.hunks {
            if hunk.new_lines == 0 {
                continue;
            }

            spans.push(FileSpan {
                path: path.clone(),
                file: FileId::numbered(file_idx),
                start_line: hunk.new_start,
                end_line: hunk.new_start + hunk.new_lines - 1,
            });
        }
    }

    spans
}

/// The kind of change a commit makes to a code region.
///
/// Used in the fragmap matrix to show how each commit touches each cluster.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchKind {
    /// The commit added new lines in this region (file was added or lines inserted).
    Added,
    /// The commit modified existing lines in this region.
    Modified,
    /// The commit deleted lines in this region.
    Deleted,
    /// The commit did not touch this region.
    None,
}

/// A cluster of overlapping FileSpans across multiple commits.
///
/// Represents a code region that multiple commits touch. Spans are merged only
/// where they overlap (within the same file); spans that merely touch, one
/// ending where the next begins, stay in separate clusters. Note that git's
/// merge does join touching changes, which is why a clean-looking fold can
/// still conflict -- see the note in README.md.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpanCluster {
    /// The merged spans in this cluster (typically one span per file touched).
    pub spans: Vec<FileSpan>,
    /// OIDs of commits that touch this cluster.
    pub commit_oids: Vec<VirtualOid>,
}

/// The complete fragmap: commits, span clusters, and the matrix showing
/// which commits touch which clusters.
///
/// The matrix is commits × clusters, where matrix[commit_idx][cluster_idx]
/// indicates how that commit touches that cluster.
#[derive(Debug, Clone)]
pub struct FragMap {
    /// The commits in order (oldest to newest).
    pub commits: Vec<VirtualOid>,
    /// The span clusters (code regions touched by commits).
    pub clusters: Vec<SpanCluster>,
    /// Matrix[commit_idx][cluster_idx] = TouchKind
    pub matrix: Vec<Vec<TouchKind>>,
}

/// Progress event emitted by [`build_fragmap`] to the caller's callback.
#[derive(Debug)]
pub enum FragMapProgress {
    /// Phase 1: clustering files. `files_done` files fully processed so far
    /// out of `files_total`. Called after each commit generation within a file
    /// so the caller can poll for interrupts even when one file is large.
    ClusteringFile {
        files_done: usize,
        files_total: usize,
    },
    /// Phase 2: deduplicating clusters (called once, before dedup runs).
    Deduplicating,
    /// Phase 3: building the touch-kind matrix. Called once per commit row so
    /// the caller can poll for interrupts during large repos.
    BuildingMatrix {
        commits_done: usize,
        commits_total: usize,
    },
    /// Reading each commit's diff, before any clustering: reported by callers
    /// that load the diffs themselves, as the per-hunk-group split does.
    ReadingCommits {
        commits_done: usize,
        commits_total: usize,
    },
}

/// The caller's progress callback asked to stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interrupted;

impl std::fmt::Display for Interrupted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("interrupted")
    }
}

impl std::error::Error for Interrupted {}

/// Build a fragmap from a list of commit diffs.
///
/// When `deduplicate` is `true` (the normal view), columns whose set of touching
/// commits is identical are merged into one, producing a compact matrix. Pass
/// `false` to keep every raw hunk cluster as its own column, which is useful
/// for debugging the cluster layout.
///
/// `progress` is called at key points during computation (see [`FragMapProgress`]).
/// Return `false` from the callback to interrupt and return `None`. Return `true`
/// to continue. The callback is called at fine granularity — once per commit
/// generation inside each file's SPG — so a single slow file remains responsive.
pub fn build_fragmap(
    commit_diffs: &[CommitDiff],
    deduplicate: bool,
    progress: &mut impl FnMut(FragMapProgress) -> bool,
) -> Option<FragMap> {
    let lineages = FileLineages::new(commit_diffs);
    let file_commits = collect_file_commits(commit_diffs, &lineages);
    let sorted_files = lineages.sorted(file_commits.keys().copied());

    let total = sorted_files.len();
    let mut clusters = Vec::new();

    for (i, &file) in sorted_files.iter().enumerate() {
        let mut poll = || {
            progress(FragMapProgress::ClusteringFile {
                files_done: i,
                files_total: total,
            })
        };
        let new_clusters = build_file_clusters(
            lineages.label(file),
            file,
            &file_commits[&file],
            commit_diffs,
            &mut poll,
        )?;
        clusters.extend(new_clusters);
    }

    if !progress(FragMapProgress::Deduplicating) {
        return None;
    }
    if deduplicate {
        deduplicate_clusters(&mut clusters);
    }

    let commits: Vec<VirtualOid> = commit_diffs.iter().map(|d| d.commit.oid.clone()).collect();
    let matrix = build_matrix(&commits, &clusters, commit_diffs, &lineages, progress)?;
    Some(FragMap {
        commits,
        clusters,
        matrix,
    })
}

/// Compute the hunk group assignment for splitting commit `commit_oid`.
///
/// Each hunk of `commit_oid` in `commit_diffs` is assigned — whole — to a
/// group by its column and its exact relation set: the earlier commits whose
/// output its removed lines rewrote and the later commits that reworked its
/// added lines, computed by composing the line maps of the intermediate diffs
/// (see the `attribution` module).
///
/// The column is what the matrix deduplicates on, the set of commits touching
/// the cluster, so hunks in different columns never merge. Relation alone is
/// not enough: a hunk that only adds lines rewrites nobody's output and so
/// relates to nothing, and a commit made entirely of such hunks would collapse
/// into one group and be refused however many columns it occupies.
///
/// Hunks are kept whole deliberately: 0-context hunks are separated by
/// unchanged lines, so whole-hunk pieces are never adjacent and the resulting
/// commits stay unrelated to each other in the matrix.  Slicing hunks into
/// per-relation fragments — however exact — interleaves the pieces, and the
/// clustering then relates them at every seam (verified empirically; the
/// result is far noisier than the split it replaces).  The single exception
/// is the rescue: when every hunk shares one relation set the split would be
/// refused, so one hunk with more than one distinct per-line relation pattern
/// has each of its fragments routed to its own group by that exact pattern —
/// the minimal cutting needed to make the commit splittable at all.  (A
/// prefix/suffix cut is not always enough: a hunk whose two ends both relate
/// to the same commit around an unrelated middle needs the middle carved out
/// specifically, not lumped into whichever end it is compared against.)
///
/// `progress` hears a [`FragMapProgress::ClusteringFile`] for each file the
/// commit touches. Returns `Ok(None)` if `commit_oid` is not found in
/// `commit_diffs`.
pub fn assign_hunk_groups(
    commit_diffs: &[CommitDiff],
    commit_oid: &Oid,
    progress: &mut impl FnMut(FragMapProgress) -> bool,
) -> Result<Option<HunkGroupAssignment>, Interrupted> {
    let Some(k) = commit_diffs
        .iter()
        .position(|d| d.commit.oid.as_oid() == Some(commit_oid))
        .map(CommitPos)
    else {
        return Ok(None);
    };

    let lineages = FileLineages::new(commit_diffs);
    let file_commits = collect_file_commits(commit_diffs, &lineages);

    // Attribute every hunk, walking files alphabetically and hunks top to
    // bottom so groups appear in a stable, positional order.
    let mut attributed: Vec<(FileId, Vec<Vec<attribution::AttributedFragment>>)> = Vec::new();
    for file in lineages.sorted(file_commits.keys().copied()) {
        if let Some(per_hunk) = attribution::attribute_target_hunks(&file_commits[&file], k) {
            attributed.push((file, per_hunk));
        }
    }

    // A hunk's relation set is the union of its lines' relation sets.
    let hunk_union = |fragments: &[attribution::AttributedFragment]| -> Vec<CommitPos> {
        let mut union: Vec<CommitPos> = fragments
            .iter()
            .flat_map(|f| f.related.iter().copied())
            .collect();
        union.sort();
        union.dedup();
        union
    };
    // Rescue check: when every hunk has the same relation set the split would
    // be refused, so pick ONE hunk with more than one distinct per-line
    // relation pattern — its fragments are then routed individually below.
    let distinct_unions: HashSet<Vec<CommitPos>> = attributed
        .iter()
        .flat_map(|(_, per_hunk)| per_hunk.iter().map(|fragments| hunk_union(fragments)))
        .collect();
    let mut cut_target: Option<(FileId, HunkPos)> = None;
    if distinct_unions.len() < 2 {
        'search: for &(file, ref per_hunk) in &attributed {
            for (hunk, fragments) in per_hunk.iter().enumerate() {
                let distinct_patterns: HashSet<&Vec<CommitPos>> =
                    fragments.iter().map(|f| &f.related).collect();
                if distinct_patterns.len() >= 2 {
                    cut_target = Some((file, HunkPos(hunk)));
                    break 'search;
                }
            }
        }
    }

    // Which column each of K's hunks sits in.
    //
    // Hunks in different columns are separate pieces of the commit and must
    // never merge, whatever their relation sets say. Relation alone cannot see
    // this: a hunk that only inserts lines rewrites nobody's output, so every
    // such hunk comes back relating to nothing and they collapse into a single
    // group even when they are in different files entirely.
    let mut column_of: HashMap<(FileId, HunkPos), Vec<VirtualOid>> = HashMap::new();
    let mut clusters_of: HashMap<FileId, Vec<(SpanCluster, Option<spg::SpgSpan>)>> = HashMap::new();
    let files_total = attributed.len();
    for (files_done, &(file, _)) in attributed.iter().enumerate() {
        let mut poll = || {
            progress(FragMapProgress::ClusteringFile {
                files_done,
                files_total,
            });
            true
        };
        let Some(clusters) = build_file_clusters_with_target(
            lineages.label(file),
            file,
            &file_commits[&file],
            commit_diffs,
            Some(k),
            &mut poll,
        ) else {
            continue;
        };
        let Some((_, k_hunks)) = file_commits[&file].iter().find(|(commit, _)| *commit == k) else {
            continue;
        };
        for (hunk_idx, hunk) in k_hunks.iter().enumerate() {
            let start = hunk.new_start as i64;
            let end = start + (hunk.new_lines.max(1)) as i64;
            // A hunk can graze several columns; it belongs to the one it
            // overlaps most, since it is kept whole.
            let mut best: Option<(i64, usize)> = None;
            for (cluster_idx, (_, target_span)) in clusters.iter().enumerate() {
                let Some(span) = target_span else { continue };
                let overlap = end.min(span.end) - start.max(span.start);
                if overlap > 0 && best.is_none_or(|(most, _)| overlap > most) {
                    best = Some((overlap, cluster_idx));
                }
            }
            if let Some((_, cluster_idx)) = best {
                // Identified by the set of commits touching the cluster, which
                // is precisely what the matrix deduplicates columns on: two
                // regions touched by the same commits are one column, even in
                // different files, so hunks in them belong together.
                let mut touching = clusters[cluster_idx].0.commit_oids.clone();
                touching.sort();
                column_of.insert((file, HunkPos(hunk_idx)), touching);
            }
        }
        clusters_of.insert(file, clusters);
    }

    // The column a line range in `file` sits in, for placing the cut hunk's
    // fragments. A fragment and a whole hunk that belong to the same column and
    // relate to the same commits are one piece, so both must be keyed alike.
    let column_at = |file: FileId, range: &assignment::LineRange| -> Option<Vec<VirtualOid>> {
        let clusters = clusters_of.get(&file)?;
        let (start, end) = (
            range.start as i64,
            (range.end as i64).max(range.start as i64 + 1),
        );
        let mut best: Option<(i64, usize)> = None;
        for (cluster_idx, (_, target_span)) in clusters.iter().enumerate() {
            let Some(span) = target_span else { continue };
            let overlap = end.min(span.end) - start.max(span.start);
            if overlap > 0 && best.is_none_or(|(most, _)| overlap > most) {
                best = Some((overlap, cluster_idx));
            }
        }
        let (_, cluster_idx) = best?;
        let mut touching = clusters[cluster_idx].0.commit_oids.clone();
        touching.sort();
        Some(touching)
    };

    // Group hunks by column and relation set — and the cut hunk's sides by
    // relation set alone, since they are pieces of one hunk and what is
    // separable there is bounded by the hunk, not by how many columns the
    // pairings generate. Indexed in order of first appearance so the split
    // pieces come out positionally.
    type GroupKey = (Option<Vec<VirtualOid>>, Vec<CommitPos>);
    let mut patterns: Vec<GroupKey> = Vec::new();
    let group_of = |key: GroupKey, patterns: &mut Vec<GroupKey>| -> usize {
        match patterns.iter().position(|p| *p == key) {
            Some(group) => group,
            None => {
                patterns.push(key);
                patterns.len() - 1
            }
        }
    };

    let mut by_file: HashMap<FileId, Vec<HunkAssignment>> = HashMap::new();
    for &(file, ref per_hunk) in &attributed {
        let entries: Vec<HunkAssignment> = per_hunk
            .iter()
            .enumerate()
            .map(|(hunk_idx, fragments)| {
                let hunk = HunkPos(hunk_idx);
                if cut_target == Some((file, hunk)) {
                    let assigned: Vec<FragmentAssignment> = fragments
                        .iter()
                        .map(|f| FragmentAssignment {
                            fragment: f.fragment,
                            group: group_of(
                                (column_at(file, &f.fragment.new_lines), f.related.clone()),
                                &mut patterns,
                            ),
                        })
                        .collect();
                    HunkAssignment::Fragmented {
                        fragments: assigned,
                    }
                } else {
                    HunkAssignment::Whole {
                        group: group_of(
                            (column_of.get(&(file, hunk)).cloned(), hunk_union(fragments)),
                            &mut patterns,
                        ),
                    }
                }
            })
            .collect();
        by_file.insert(file, entries);
    }

    let by_change = (0..commit_diffs[k.0].files.len())
        .map(|change| {
            by_file
                .get(&lineages.of(k, ChangePos(change)))
                .cloned()
                .unwrap_or_default()
        })
        .collect();

    Ok(Some(HunkGroupAssignment {
        group_count: patterns.len(),
        by_change,
    }))
}

impl FragMap {
    /// Find the single commit this commit can be squashed into, if any.
    ///
    /// Returns `Some(target_idx)` when every cluster the commit touches is
    /// squashable (no conflicting commits in between) and all clusters
    /// point to the same single earlier commit. Returns `None` otherwise.
    pub fn squash_target(&self, commit_idx: usize) -> Option<usize> {
        let mut target: Option<usize> = None;

        for cluster_idx in 0..self.clusters.len() {
            if self.matrix[commit_idx][cluster_idx] == TouchKind::None {
                continue;
            }

            let earlier = (0..commit_idx)
                .rev()
                .find(|&i| self.matrix[i][cluster_idx] != TouchKind::None);

            let earlier_idx = earlier?;

            match self.cluster_relation(earlier_idx, commit_idx, cluster_idx) {
                SquashRelation::Squashable => match target {
                    None => target = Some(earlier_idx),
                    Some(t) if t == earlier_idx => {}
                    Some(_) => return None,
                },
                _ => return None,
            }
        }

        target
    }

    /// Check if a commit is fully squashable into a single other commit.
    pub fn is_fully_squashable(&self, commit_idx: usize) -> bool {
        self.squash_target(commit_idx).is_some()
    }

    /// Check whether two commits both touch at least one common cluster.
    pub fn shares_cluster_with(&self, a: usize, b: usize) -> bool {
        if a == b {
            return false;
        }
        (0..self.clusters.len())
            .any(|c| self.matrix[a][c] != TouchKind::None && self.matrix[b][c] != TouchKind::None)
    }

    /// Determine the relationship between two commits for a specific cluster.
    ///
    /// Returns `NoRelation` if one or both commits don't touch the cluster,
    /// `Squashable` if both touch it with no collisions in between, or
    /// `Conflicting` if both touch it with other commits in between.
    pub fn cluster_relation(
        &self,
        earlier_commit_idx: usize,
        later_commit_idx: usize,
        cluster_idx: usize,
    ) -> SquashRelation {
        if earlier_commit_idx >= self.commits.len()
            || later_commit_idx >= self.commits.len()
            || cluster_idx >= self.clusters.len()
        {
            return SquashRelation::NoRelation;
        }

        if earlier_commit_idx >= later_commit_idx {
            return SquashRelation::NoRelation;
        }

        let earlier_touches = self.matrix[earlier_commit_idx][cluster_idx] != TouchKind::None;
        let later_touches = self.matrix[later_commit_idx][cluster_idx] != TouchKind::None;

        if !earlier_touches || !later_touches {
            return SquashRelation::NoRelation;
        }

        for commit_idx in (earlier_commit_idx + 1)..later_commit_idx {
            if self.matrix[commit_idx][cluster_idx] != TouchKind::None {
                return SquashRelation::Conflicting;
            }
        }

        SquashRelation::Squashable
    }

    /// Determine whether a connector between `commit_idx` and its earliest
    /// earlier neighbor in `cluster_idx` should be rendered as squashable.
    ///
    /// Returns `None` if there is no earlier touch in the cluster.
    /// Returns `Some(true)` when the entire commit is fully squashable into
    /// that specific earlier commit, `Some(false)` otherwise.
    pub fn connector_squashable(&self, commit_idx: usize, cluster_idx: usize) -> Option<bool> {
        let earlier = (0..commit_idx)
            .rev()
            .find(|&i| self.matrix[i][cluster_idx] != TouchKind::None)?;

        Some(self.squash_target(commit_idx).is_some_and(|t| t == earlier))
    }
}

/// Build the commits × clusters matrix with TouchKind values.
///
/// For each (commit, cluster), determine if the commit touches the cluster
/// and if so, classify the touch as Added/Modified/Deleted.
fn build_matrix(
    commits: &[VirtualOid],
    clusters: &[SpanCluster],
    commit_diffs: &[CommitDiff],
    lineages: &FileLineages,
    progress: &mut impl FnMut(FragMapProgress) -> bool,
) -> Option<Vec<Vec<TouchKind>>> {
    let total = commits.len();
    let mut matrix = vec![vec![TouchKind::None; clusters.len()]; total];

    for (commit_idx, commit_oid) in commits.iter().enumerate() {
        if !progress(FragMapProgress::BuildingMatrix {
            commits_done: commit_idx,
            commits_total: total,
        }) {
            return None;
        }
        let commit_diff = &commit_diffs[commit_idx];

        for (cluster_idx, cluster) in clusters.iter().enumerate() {
            if cluster.commit_oids.contains(commit_oid) {
                matrix[commit_idx][cluster_idx] =
                    determine_touch_kind(commit_idx, commit_diff, cluster, lineages);
            }
        }
    }

    Some(matrix)
}

/// Determine how a commit touches a cluster (Added/Modified/Deleted), from
/// the status of its change to the cluster's file.
fn determine_touch_kind(
    commit_idx: usize,
    commit_diff: &CommitDiff,
    cluster: &SpanCluster,
    lineages: &FileLineages,
) -> TouchKind {
    for cluster_span in &cluster.spans {
        let change = (0..commit_diff.files.len()).find(|&change| {
            lineages.of(CommitPos(commit_idx), ChangePos(change)) == cluster_span.file
        });
        if let Some(change) = change {
            // From the delta's own status, not from which paths are set:
            // libgit2 fills in *both* `old_file` and `new_file` for an add
            // and for a delete, so inferring from a missing path made both
            // of those unreachable and rendered every touch as `Modified`.
            return match commit_diff.files[change].status {
                crate::DeltaStatus::Added | crate::DeltaStatus::Untracked => TouchKind::Added,
                crate::DeltaStatus::Deleted => TouchKind::Deleted,
                _ => TouchKind::Modified,
            };
        }
    }

    TouchKind::None
}

/// The relationship between two commits within a specific cluster.
///
/// Used to determine if commits that touch the same cluster can be
/// safely squashed together, following the original fragmap logic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SquashRelation {
    /// Neither commit (or only one) touches this cluster.
    NoRelation,
    /// Both commits touch the cluster with no collisions in between.
    /// These commits can potentially be squashed (yellow in UI).
    Squashable,
    /// Both commits touch the cluster with collisions (commits in between
    /// also touch it). Squashing would conflict (red in UI).
    Conflicting,
}

#[cfg(test)]
mod tests;
