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

//! Span Propagation Graph.
//!
//! Faithfully implements the algorithm from the original fragmap tool
//! (https://github.com/amollberg/fragmap). For each file, we build a
//! directed acyclic graph where:
//!
//! - **Active nodes** represent actual hunks (code changes)
//! - **Inactive nodes** represent propagated surviving spans
//! - **Edges** connect overlapping nodes across commit generations
//! - **SOURCE/SINK** are sentinels bounding the DAG
//!
//! Columns in the fragmap matrix correspond to unique paths through this
//! DAG. When a new edge is registered from a node, its SINK edge is
//! removed — this naturally invalidates paths that are "consumed" by
//! later changes.

mod build;
mod columns;
mod paths;
mod shared_tail_lists;

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::{CommitDiff, VirtualOid};

use super::{CommitPos, FileId, FileSpan, HunkInfo, SpanCluster};
use build::build_file_spg;
use columns::{spg_columns, spg_target_columns};
use paths::{spg_all_paths, spg_enumerate_paths};

/// Half-open interval `[start, end)` for SPG span computations.
/// Uses `i64` to safely handle arithmetic with large sentinel values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct SpgSpan {
    pub(super) start: i64,
    pub(super) end: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SpgOverlap {
    None,
    Point,
    Interval,
}

const SPG_SENTINEL: i64 = 100_000_000;

impl SpgSpan {
    pub(super) fn is_empty(&self) -> bool {
        self.start >= self.end
    }

    /// Overlap classification matching the original fragmap's `Span.overlap()`.
    pub(super) fn overlap(&self, other: &SpgSpan) -> SpgOverlap {
        if (self.start == other.start || self.end == other.end)
            || !(self.end <= other.start || other.end <= self.start)
        {
            if self.is_empty() || other.is_empty() {
                SpgOverlap::Point
            } else {
                SpgOverlap::Interval
            }
        } else {
            SpgOverlap::None
        }
    }

    pub(super) fn from_old_hunk(h: &HunkInfo) -> Self {
        let mut start = h.old_start as i64;
        if h.old_lines == 0 {
            start += 1;
        }
        SpgSpan {
            start,
            end: start + h.old_lines as i64,
        }
    }

    pub(super) fn from_new_hunk(h: &HunkInfo) -> Self {
        let mut start = h.new_start as i64;
        if h.new_lines == 0 {
            start += 1;
        }
        SpgSpan {
            start,
            end: start + h.new_lines as i64,
        }
    }
}

/// A node in the Span Propagation Graph.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SpgNode {
    /// Commit index (generation). -1 for SOURCE, `i32::MAX` for SINK.
    generation: i32,
    is_active: bool,
    old_span: SpgSpan,
    new_span: SpgSpan,
}

fn source_node() -> SpgNode {
    SpgNode {
        generation: -1,
        is_active: false,
        old_span: SpgSpan { start: 1, end: 1 },
        new_span: SpgSpan {
            start: 0,
            end: SPG_SENTINEL,
        },
    }
}

fn sink_node() -> SpgNode {
    SpgNode {
        generation: i32::MAX,
        is_active: false,
        old_span: SpgSpan {
            start: 0,
            end: SPG_SENTINEL,
        },
        new_span: SpgSpan { start: 1, end: 1 },
    }
}

/// A node of an [`Spg`], numbered in the order it was first seen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct NodeId(usize);

const SOURCE: NodeId = NodeId(0);
const SINK: NodeId = NodeId(1);

/// One value per node of an [`Spg`].
#[derive(Clone)]
struct PerNode<T>(Vec<T>);

impl<T> PerNode<T> {
    /// As many of `value` as `like` has nodes.
    fn like<U>(like: &PerNode<U>, value: T) -> Self
    where
        T: Clone,
    {
        PerNode(vec![value; like.0.len()])
    }

    fn len(&self) -> usize {
        self.0.len()
    }

    fn ids(&self) -> impl Iterator<Item = NodeId> + use<T> {
        (0..self.0.len()).map(NodeId)
    }

    fn push(&mut self, value: T) -> NodeId {
        self.0.push(value);
        NodeId(self.0.len() - 1)
    }
}

impl<T> std::ops::Index<NodeId> for PerNode<T> {
    type Output = T;

    fn index(&self, node: NodeId) -> &T {
        &self.0[node.0]
    }
}

impl<T> std::ops::IndexMut<NodeId> for PerNode<T> {
    fn index_mut(&mut self, node: NodeId) -> &mut T {
        &mut self.0[node.0]
    }
}

/// The Span Propagation Graph for one file. Equal nodes are one node.
struct Spg {
    nodes: PerNode<SpgNode>,
    index: HashMap<SpgNode, NodeId>,
    succs: PerNode<Vec<NodeId>>,
    downstream_from_active: PerNode<bool>,
    /// Every node that has had an edge to SINK, possibly since replaced.
    frontier: Vec<NodeId>,
}

impl Spg {
    fn empty() -> Self {
        let mut spg = Spg {
            nodes: PerNode(Vec::new()),
            index: HashMap::new(),
            succs: PerNode(Vec::new()),
            downstream_from_active: PerNode(Vec::new()),
            frontier: Vec::new(),
        };
        let source = spg.node(source_node());
        let sink = spg.node(sink_node());
        debug_assert_eq!((source, sink), (SOURCE, SINK));
        spg.register(SOURCE, SINK);
        spg
    }

    fn node(&mut self, node: SpgNode) -> NodeId {
        if let Some(&existing) = self.index.get(&node) {
            return existing;
        }
        self.downstream_from_active.push(node.is_active);
        self.succs.push(Vec::new());
        let id = self.nodes.push(node.clone());
        self.index.insert(node, id);
        id
    }

    /// Register an edge from `from` to `to`, removing any existing SINK edge
    /// from `from`. This is the core SPG mutation: when a node gets a real
    /// successor, it no longer points directly to SINK.
    fn register(&mut self, from: NodeId, to: NodeId) {
        let succs = &mut self.succs[from];
        succs.retain(|&n| n != SINK);
        succs.push(to);
        if to == SINK {
            self.frontier.push(from);
        }
        self.downstream_from_active[to] |= self.downstream_from_active[from];
    }

    /// Find all nodes that have SINK as a direct successor (the current frontier).
    fn sink_connected_nodes(&mut self) -> Vec<NodeId> {
        self.frontier.sort_unstable();
        self.frontier.dedup();
        let succs = &self.succs;
        self.frontier.retain(|&n| succs[n].contains(&SINK));
        self.frontier.clone()
    }
}

/// Deduplicate clusters by activation pattern (BriefFragmap equivalent).
///
/// Columns whose CHANGE/NO_CHANGE pattern across commits is identical are
/// merged into a single column. This matches the original fragmap tool's
/// `BriefFragmap._group_by_patch_connection()` which groups columns with
/// the same binary connection string.
pub(super) fn deduplicate_clusters(clusters: &mut Vec<SpanCluster>) {
    // Build activation pattern (sorted commit_oids) for each cluster
    for c in clusters.iter_mut() {
        c.commit_oids.sort();
    }
    let mut seen: HashSet<Vec<VirtualOid>> = HashSet::new();
    clusters.retain(|c| seen.insert(c.commit_oids.clone()));
}

/// Build all `SpanCluster` entries for a single file path.
///
/// Runs the SPG for the given file and converts each unique path through the
/// DAG into a `SpanCluster` that records which commits touch it.
///
/// `poll` is forwarded to the SPG builder and called after each commit
/// generation. Return `false` from `poll` to interrupt; `None` is returned
/// in that case.
pub(super) fn build_file_clusters(
    path: &Path,
    file: FileId,
    commits_for_file: &[(CommitPos, Vec<HunkInfo>)],
    commit_diffs: &[CommitDiff],
    poll: &mut impl FnMut() -> bool,
) -> Option<Vec<SpanCluster>> {
    let spg = build_file_spg(commits_for_file, poll)?;
    let paths = spg_all_paths(&spg, poll)?;

    let mut clusters: Vec<SpanCluster> = Vec::new();
    for path_nodes in &paths {
        let mut commit_oids: Vec<VirtualOid> = Vec::new();
        let mut last_active_span: Option<SpgSpan> = None;

        for node in path_nodes {
            if node.is_active
                && node.generation >= 0
                && (node.generation as usize) < commit_diffs.len()
            {
                let oid = &commit_diffs[node.generation as usize].commit.oid;
                if !commit_oids.contains(oid) {
                    commit_oids.push(oid.clone());
                }
                last_active_span = Some(node.new_span);
            }
        }

        if let Some(sp) = last_active_span
            && !commit_oids.is_empty()
        {
            clusters.push(span_cluster(path, file, commit_oids, sp));
        }
    }

    Some(clusters)
}

fn span_cluster(
    path: &Path,
    file: FileId,
    commit_oids: Vec<VirtualOid>,
    last_span: SpgSpan,
) -> SpanCluster {
    SpanCluster {
        spans: vec![FileSpan {
            path: path.to_path_buf(),
            file,
            start_line: last_span.start.max(1) as u32,
            end_line: (last_span.end - 1).max(1) as u32,
        }],
        commit_oids,
    }
}

/// One cluster per distinct set of commits touching the file, in the order
/// `build_file_clusters` first lists each set — all that deduplication keeps
/// of it, without the cost of listing every path.
pub(super) fn build_file_columns(
    path: &Path,
    file: FileId,
    commits_for_file: &[(CommitPos, Vec<HunkInfo>)],
    commit_diffs: &[CommitDiff],
    poll: &mut impl FnMut() -> bool,
) -> Option<Vec<SpanCluster>> {
    let spg = build_file_spg(commits_for_file, poll)?;
    Some(
        spg_columns(&spg, poll)?
            .into_iter()
            .map(|column| {
                let commit_oids = column
                    .generations
                    .iter()
                    .map(|&generation| commit_diffs[generation as usize].commit.oid.clone())
                    .collect();
                span_cluster(path, file, commit_oids, column.last_span)
            })
            .collect(),
    )
}

/// A column one of the target commit's hunks sits in.
pub(super) struct TargetColumn {
    /// The hunk's span, in the target commit's own coordinates.
    pub(super) span: SpgSpan,
    /// The commits touching the column, sorted. This is what the matrix
    /// deduplicates columns on, so regions touched by the same commits are one
    /// column even in different files.
    pub(super) touching: Vec<VirtualOid>,
}

/// The columns `target`'s hunks in this file sit in, in matrix order: for
/// each hunk, the first column through it.
pub(super) fn build_target_columns(
    commits_for_file: &[(CommitPos, Vec<HunkInfo>)],
    commit_diffs: &[CommitDiff],
    target: CommitPos,
    poll: &mut impl FnMut() -> bool,
) -> Option<Vec<TargetColumn>> {
    let spg = build_file_spg(commits_for_file, poll)?;
    Some(
        spg_target_columns(&spg, target.0 as i32, poll)?
            .into_iter()
            .map(|(span, generations)| {
                let mut touching: Vec<VirtualOid> = generations
                    .iter()
                    .map(|&generation| commit_diffs[generation as usize].commit.oid.clone())
                    .collect();
                touching.sort();
                TargetColumn { span, touching }
            })
            .collect(),
    )
}

/// Enumerate all SPG paths for each file and the raw path count.
/// Used by `dump_per_file_spg_stats`.
pub(super) fn enumerate_file_spg_paths(
    commits: &[(CommitPos, Vec<HunkInfo>)],
) -> (usize, usize, usize) {
    let spg = build_file_spg(commits, &mut || true).expect("no-op poll never interrupts");
    let node_count = spg.nodes.len();
    let raw_paths =
        spg_enumerate_paths(&spg, SOURCE, &mut || true).expect("no-op poll never interrupts");
    let deduped_paths = spg_all_paths(&spg, &mut || true).expect("no-op poll never interrupts");
    (node_count, raw_paths.len(), deduped_paths.len())
}

/// Diagnostic: dump per-file SPG stats (for debugging, not used in production).
#[allow(dead_code)]
pub(super) fn dump_per_file_spg_stats(commit_diffs: &[CommitDiff]) {
    let lineages = super::FileLineages::new(commit_diffs);
    let file_commits = super::collect_file_commits(commit_diffs, &lineages);

    for file in lineages.sorted(file_commits.keys().copied()) {
        let path = lineages.label(file);
        let commits_for_file = &file_commits[&file];
        let (node_count, raw_path_count, deduped_path_count) =
            enumerate_file_spg_paths(commits_for_file);
        let gens: Vec<usize> = commits_for_file
            .iter()
            .map(|(commit, _)| commit.0)
            .collect();
        eprintln!(
            "FILE: {} | gens={:?} | nodes={} | raw_paths={} | deduped_paths={}",
            path.display(),
            gens,
            node_count,
            raw_path_count,
            deduped_path_count
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spgspan_overlap_same_start_interval() {
        // [0,5) and [0,10): same start → Interval
        let a = SpgSpan { start: 0, end: 5 };
        let b = SpgSpan { start: 0, end: 10 };
        assert_eq!(a.overlap(&b), SpgOverlap::Interval);
    }

    #[test]
    fn spgspan_overlap_same_end_interval() {
        // [3,10) and [0,10): same end → Interval
        let a = SpgSpan { start: 3, end: 10 };
        let b = SpgSpan { start: 0, end: 10 };
        assert_eq!(a.overlap(&b), SpgOverlap::Interval);
    }

    #[test]
    fn spgspan_overlap_partial_interval() {
        // [3,8) and [5,12): partial overlap, no shared boundary → Interval
        let a = SpgSpan { start: 3, end: 8 };
        let b = SpgSpan { start: 5, end: 12 };
        assert_eq!(a.overlap(&b), SpgOverlap::Interval);
    }

    #[test]
    fn spgspan_overlap_contained_interval() {
        // [2,7) contained in [0,10), no shared boundary → Interval
        let a = SpgSpan { start: 2, end: 7 };
        let b = SpgSpan { start: 0, end: 10 };
        assert_eq!(a.overlap(&b), SpgOverlap::Interval);
    }

    #[test]
    fn spgspan_overlap_adjacent_is_none() {
        // [3,5) and [5,8): end of a == start of b in a half-open interval.
        // Condition: !(5<=5 || 8<=3) = !(true) = false; (3==5||5==8) = false → None
        let a = SpgSpan { start: 3, end: 5 };
        let b = SpgSpan { start: 5, end: 8 };
        assert_eq!(a.overlap(&b), SpgOverlap::None);
    }

    #[test]
    fn spgspan_overlap_disjoint_is_none() {
        let a = SpgSpan { start: 0, end: 3 };
        let b = SpgSpan { start: 5, end: 10 };
        assert_eq!(a.overlap(&b), SpgOverlap::None);
    }

    #[test]
    fn spgspan_overlap_empty_at_shared_start_is_point() {
        // [5,5) (empty) and [5,10): same start fires → outer true, is_empty → Point
        let a = SpgSpan { start: 5, end: 5 };
        let b = SpgSpan { start: 5, end: 10 };
        assert_eq!(a.overlap(&b), SpgOverlap::Point);
    }

    #[test]
    fn spgspan_overlap_both_empty_same_position_is_point() {
        let a = SpgSpan { start: 5, end: 5 };
        let b = SpgSpan { start: 5, end: 5 };
        assert_eq!(a.overlap(&b), SpgOverlap::Point);
    }

    #[test]
    fn spgspan_overlap_empty_not_at_boundary_is_none() {
        // Empty span [3,3) vs [5,10): no shared endpoint, not adjacent → None
        let a = SpgSpan { start: 3, end: 3 };
        let b = SpgSpan { start: 5, end: 10 };
        assert_eq!(a.overlap(&b), SpgOverlap::None);
    }

    #[test]
    fn from_old_hunk_pure_insertion_start_adjusted() {
        // old_lines=0 means "insertion before old_start+1" → start is shifted +1
        let h = HunkInfo {
            old_start: 10,
            old_lines: 0,
            new_start: 10,
            new_lines: 5,
        };
        let sp = SpgSpan::from_old_hunk(&h);
        // start = 10+1=11, end = 11+0=11 (empty span signals insertion point)
        assert_eq!(sp, SpgSpan { start: 11, end: 11 });
    }

    #[test]
    fn from_new_hunk_pure_deletion_start_adjusted() {
        // new_lines=0 means "pure deletion, no new lines" → start is shifted +1
        let h = HunkInfo {
            old_start: 10,
            old_lines: 3,
            new_start: 10,
            new_lines: 0,
        };
        let sp = SpgSpan::from_new_hunk(&h);
        // start = 10+1=11, end = 11+0=11 (empty span)
        assert_eq!(sp, SpgSpan { start: 11, end: 11 });
    }

    #[test]
    fn from_old_hunk_normal_no_adjustment() {
        let h = HunkInfo {
            old_start: 10,
            old_lines: 5,
            new_start: 10,
            new_lines: 8,
        };
        let sp = SpgSpan::from_old_hunk(&h);
        assert_eq!(sp, SpgSpan { start: 10, end: 15 });
    }

    #[test]
    fn from_new_hunk_normal_no_adjustment() {
        let h = HunkInfo {
            old_start: 10,
            old_lines: 5,
            new_start: 10,
            new_lines: 8,
        };
        let sp = SpgSpan::from_new_hunk(&h);
        assert_eq!(sp, SpgSpan { start: 10, end: 18 });
    }
}
