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

use std::collections::HashMap;

use crate::{CommitDiff, Oid, VirtualOid};

mod assignment;
mod spg;
pub use assignment::{
    FragmentAssignment, HunkAssignment, HunkFragment, HunkGroupAssignment, LineRange,
};
use spg::{build_file_clusters, build_file_clusters_and_assign_hunks, deduplicate_clusters};

/// Build a map from every known file path to the canonical (earliest) name for
/// that file, following rename chains across commits.
///
/// The commit diffs must be in chronological order (oldest first).  When a
/// `FileDiff` has `old_path ≠ new_path` the old name's canonical entry is
/// propagated to the new name.
fn build_rename_map(commit_diffs: &[CommitDiff]) -> HashMap<String, String> {
    let mut canonical: HashMap<String, String> = HashMap::new();
    for diff in commit_diffs {
        for file in &diff.files {
            if let (Some(old), Some(new)) = (&file.old_path, &file.new_path)
                && old != new
            {
                let root = canonical.get(old).cloned().unwrap_or_else(|| old.clone());
                canonical.insert(new.clone(), root);
            }
        }
    }
    canonical
}

/// Resolve a file path to its canonical (earliest) name using the rename map.
fn canonical_path<'a>(path: &'a str, rename_map: &'a HashMap<String, String>) -> &'a str {
    rename_map.get(path).map(|s| s.as_str()).unwrap_or(path)
}

/// Collect per-file hunk lists grouped by canonical path.
///
/// This is the shared grouping logic used by [`build_fragmap`],
/// [`assign_hunk_groups`], and [`dump_per_file_spg_stats`].
fn collect_file_commits(
    commit_diffs: &[CommitDiff],
    rename_map: &HashMap<String, String>,
) -> HashMap<String, Vec<(usize, Vec<HunkInfo>)>> {
    let mut file_commits: HashMap<String, Vec<(usize, Vec<HunkInfo>)>> = HashMap::new();
    for (commit_idx, diff) in commit_diffs.iter().enumerate() {
        for file in &diff.files {
            let path = match &file.new_path {
                Some(p) => p.clone(),
                None => continue,
            };
            let key = canonical_path(&path, rename_map).to_owned();
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
            if !hunks.is_empty() {
                let entry = file_commits.entry(key).or_default();
                if let Some(last) = entry.last_mut()
                    && last.0 == commit_idx
                {
                    last.1.extend(hunks);
                    continue;
                }
                entry.push((commit_idx, hunks));
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
    /// The file path (from the new version of the file).
    pub path: String,
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

    for file in &commit_diff.files {
        let path = match &file.new_path {
            Some(p) => p.clone(),
            None => continue,
        };

        for hunk in &file.hunks {
            if hunk.new_lines == 0 {
                continue;
            }

            spans.push(FileSpan {
                path: path.to_string(),
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

/// A cluster of overlapping or adjacent FileSpans across multiple commits.
///
/// Represents a code region that multiple commits touch. Spans are merged
/// when they overlap or are adjacent (within same file).
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
}

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
    let rename_map = build_rename_map(commit_diffs);
    let file_commits = collect_file_commits(commit_diffs, &rename_map);
    let mut sorted_paths: Vec<String> = file_commits.keys().cloned().collect();
    sorted_paths.sort();

    let total = sorted_paths.len();
    let mut clusters = Vec::new();

    for (i, path) in sorted_paths.iter().enumerate() {
        let commits_for_file = &file_commits[path];
        let mut poll = || {
            progress(FragMapProgress::ClusteringFile {
                files_done: i,
                files_total: total,
            })
        };
        let new_clusters = build_file_clusters(path, commits_for_file, commit_diffs, &mut poll)?;
        clusters.extend(new_clusters);
    }

    if !progress(FragMapProgress::Deduplicating) {
        return None;
    }
    if deduplicate {
        deduplicate_clusters(&mut clusters);
    }

    let commits: Vec<VirtualOid> = commit_diffs.iter().map(|d| d.commit.oid.clone()).collect();
    let matrix = build_matrix(&commits, &clusters, commit_diffs, &rename_map, progress)?;
    Some(FragMap {
        commits,
        clusters,
        matrix,
    })
}

/// Compute the fragmap-based hunk group assignment for commit `commit_oid`.
///
/// Uses the same SPG clustering and deduplication as [`build_fragmap`] with
/// `deduplicate = true`. Each hunk of `commit_oid` in `commit_diffs` is
/// assigned to the deduplicated fragmap cluster (column) it belongs to; see
/// [`HunkGroupAssignment`] for the result shape.
///
/// Returns `None` if `commit_oid` is not found in `commit_diffs`.
pub fn assign_hunk_groups(
    commit_diffs: &[CommitDiff],
    commit_oid: &Oid,
) -> Option<HunkGroupAssignment> {
    let k_idx = commit_diffs
        .iter()
        .position(|d| d.commit.oid.as_oid() == Some(commit_oid))?;

    let rename_map = build_rename_map(commit_diffs);
    let file_commits = collect_file_commits(commit_diffs, &rename_map);

    let mut sorted_paths: Vec<&String> = file_commits.keys().collect();
    sorted_paths.sort();

    // Phase 1: accumulate pre-dedup clusters (in the same order as build_fragmap)
    // and record, for each of K's hunks, ALL pre-dedup clusters it appears in
    // together with the sub-region of the hunk each cluster's path claims.
    //
    // A hunk can appear on multiple SPG paths with different commit_oids,
    // so it may belong to multiple clusters (and after dedup, multiple groups).
    let mut pre_dedup_clusters: Vec<SpanCluster> = Vec::new();
    let mut k_hunk_touches: HashMap<String, Vec<Vec<spg::ClusterTouch>>> = HashMap::new();

    for path in &sorted_paths {
        let commits_for_file = &file_commits[*path];
        let file_cluster_offset = pre_dedup_clusters.len();

        let has_k = commits_for_file.iter().any(|(idx, _)| *idx == k_idx);
        let (file_clusters, hunk_touches) = if has_k {
            let analysis = build_file_clusters_and_assign_hunks(
                path,
                commits_for_file,
                commit_diffs,
                k_idx,
                &mut || true,
            )
            .expect("no-op poll never interrupts");
            (analysis.clusters, analysis.target_hunk_touches)
        } else {
            (
                build_file_clusters(path, commits_for_file, commit_diffs, &mut || true)
                    .expect("no-op poll never interrupts"),
                vec![],
            )
        };

        if has_k && !hunk_touches.is_empty() {
            let global_touches: Vec<Vec<spg::ClusterTouch>> = hunk_touches
                .iter()
                .map(|touches| {
                    touches
                        .iter()
                        .map(|touch| spg::ClusterTouch {
                            cluster_idx: file_cluster_offset + touch.cluster_idx,
                            ..*touch
                        })
                        .collect()
                })
                .collect();
            k_hunk_touches.insert((*path).clone(), global_touches);
        }

        pre_dedup_clusters.extend(file_clusters);
    }

    // Phase 2: compute the dedup mapping, replicating the logic of
    // deduplicate_clusters but also building pre_idx → group_idx.
    for cluster in &mut pre_dedup_clusters {
        cluster.commit_oids.sort();
    }
    let mut seen_patterns: Vec<Vec<VirtualOid>> = Vec::new();
    let mut group_idx_for_prededup: Vec<usize> = Vec::with_capacity(pre_dedup_clusters.len());
    for cluster in &pre_dedup_clusters {
        if let Some(pos) = seen_patterns.iter().position(|p| p == &cluster.commit_oids) {
            group_idx_for_prededup.push(pos);
        } else {
            let new_idx = seen_patterns.len();
            seen_patterns.push(cluster.commit_oids.clone());
            group_idx_for_prededup.push(new_idx);
        }
    }
    let group_count = seen_patterns.len();

    // Phase 3: Assign each of K's hunks — or fragments of them — to dedup
    // groups.
    //
    // A hunk can belong to multiple pre-dedup clusters (via different SPG
    // paths) which may map to different dedup groups.  Every group that K
    // contributes to should get at least one hunk assigned, so the split
    // produces the same number of commits as fragmap columns K touches.
    // Where a hunk's group claims separate cleanly it is partitioned so that
    // each column can receive its own part — see `assignment::assign_hunks`.
    let mut all_claims: Vec<assignment::HunkClaims> = Vec::new();
    for (path, hunk_touches) in &k_hunk_touches {
        let k_hunks = file_commits[path]
            .iter()
            .find(|(idx, _)| *idx == k_idx)
            .map(|(_, hunks)| hunks.as_slice())
            .unwrap_or(&[]);
        for (hunk_idx, touches) in hunk_touches.iter().enumerate() {
            let hunk = &k_hunks[hunk_idx];
            let claims = touches
                .iter()
                .map(|touch| assignment::GroupClaim {
                    group: group_idx_for_prededup[touch.cluster_idx],
                    old_overlap: touch.old_overlap.map(spg_span_to_line_range),
                    new_overlap: touch.new_overlap.map(spg_span_to_line_range),
                })
                .collect();
            all_claims.push(assignment::HunkClaims {
                path: path.clone(),
                hunk_idx,
                old_span: spg_span_to_line_range(spg::SpgSpan::from_old_hunk(hunk)),
                new_span: spg_span_to_line_range(spg::SpgSpan::from_new_hunk(hunk)),
                claims,
            });
        }
    }

    Some(assignment::assign_hunks(&all_claims, group_count))
}

/// Convert an SPG span to the `u32` line-range form used by the assignment.
fn spg_span_to_line_range(span: spg::SpgSpan) -> assignment::LineRange {
    assignment::LineRange {
        start: span.start.max(0) as u32,
        end: span.end.max(0) as u32,
    }
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
    /// earlier neighbour in `cluster_idx` should be rendered as squashable.
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
    rename_map: &HashMap<String, String>,
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
                    determine_touch_kind(commit_diff, cluster, rename_map);
            }
        }
    }

    Some(matrix)
}

/// Determine how a commit touches a cluster (Added/Modified/Deleted).
///
/// Looks at the files in the commit that overlap with the cluster's spans
/// to classify the type of change. Uses the rename map to match file paths
/// across renames.
fn determine_touch_kind(
    commit_diff: &CommitDiff,
    cluster: &SpanCluster,
    rename_map: &HashMap<String, String>,
) -> TouchKind {
    for cluster_span in &cluster.spans {
        let cluster_canonical = canonical_path(&cluster_span.path, rename_map);
        for file in &commit_diff.files {
            let file_path = file.new_path.as_ref().or(file.old_path.as_ref());
            let matches = file_path
                .map(|p| canonical_path(p, rename_map) == cluster_canonical)
                .unwrap_or(false);
            if matches {
                if file.old_path.is_none() && file.new_path.is_some() {
                    return TouchKind::Added;
                } else if file.old_path.is_some() && file.new_path.is_none() {
                    return TouchKind::Deleted;
                } else {
                    return TouchKind::Modified;
                }
            }
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
