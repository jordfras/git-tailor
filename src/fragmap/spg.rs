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

// === SPG (Span Propagation Graph) implementation ===
//
// Faithfully implements the algorithm from the original fragmap tool
// (https://github.com/amollberg/fragmap). For each file, we build a
// directed acyclic graph where:
//
// - **Active nodes** represent actual hunks (code changes)
// - **Inactive nodes** represent propagated surviving spans
// - **Edges** connect overlapping nodes across commit generations
// - **SOURCE/SINK** are sentinels bounding the DAG
//
// Columns in the fragmap matrix correspond to unique paths through this
// DAG. When a new edge is registered from a node, its SINK edge is
// removed — this naturally invalidates paths that are "consumed" by
// later changes.

use std::collections::{HashMap, HashSet};

use crate::CommitDiff;

use super::{FileSpan, HunkInfo, SpanCluster};

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

/// The Span Propagation Graph for one file.
struct Spg {
    graph: HashMap<SpgNode, Vec<SpgNode>>,
    downstream_from_active: HashMap<SpgNode, bool>,
}

impl Spg {
    fn empty() -> Self {
        let source = source_node();
        let sink = sink_node();
        let mut graph = HashMap::new();
        graph.insert(source.clone(), vec![sink]);
        let mut dfa = HashMap::new();
        dfa.insert(source, false);
        Spg {
            graph,
            downstream_from_active: dfa,
        }
    }

    /// Register an edge from `from` to `to`, removing any existing SINK edge
    /// from `from`. This is the core SPG mutation: when a node gets a real
    /// successor, it no longer points directly to SINK.
    fn register(&mut self, from: &SpgNode, to: &SpgNode) {
        let sink = sink_node();
        let succs = self.graph.entry(from.clone()).or_default();
        succs.retain(|n| *n != sink);
        succs.push(to.clone());

        let from_dfa = self
            .downstream_from_active
            .get(from)
            .copied()
            .unwrap_or(from.is_active);
        self.downstream_from_active
            .entry(from.clone())
            .or_insert(from.is_active);
        let node_dfa = self
            .downstream_from_active
            .entry(to.clone())
            .or_insert(to.is_active);
        *node_dfa |= from_dfa;
    }

    /// Find all nodes that have SINK as a direct successor (the current frontier).
    fn sink_connected_nodes(&self) -> Vec<SpgNode> {
        let sink = sink_node();
        self.graph
            .iter()
            .filter(|(_, succs)| succs.contains(&sink))
            .map(|(node, _)| node.clone())
            .collect()
    }
}

/// Map the START (inclusive) of a surviving span forward through hunks.
///
/// Uses boundary-based absolute mapping matching the original fragmap's
/// RowLut. Each hunk's `from_old`/`from_new` boundaries define breakpoints;
/// surviving positions are mapped relative to the nearest preceding "end"
/// boundary.
pub(super) fn spg_map_start(line: i64, hunks: &[HunkInfo]) -> i64 {
    let mut ref_old: i64 = 0;
    let mut ref_new: i64 = 0;
    let mut has_ref = false;

    for hunk in hunks {
        let old = SpgSpan::from_old_hunk(hunk);
        let new = SpgSpan::from_new_hunk(hunk);

        if line < old.end {
            break;
        }

        ref_old = old.end;
        ref_new = new.end;
        has_ref = true;
    }

    if has_ref {
        line - ref_old + ref_new
    } else {
        line
    }
}

/// Map the END (exclusive) of a surviving span forward through hunks.
///
/// Like `spg_map_start` but checks `line - 1` against boundaries, since
/// the end is exclusive and the actual last line is `line - 1`.
pub(super) fn spg_map_end(line: i64, hunks: &[HunkInfo]) -> i64 {
    let check = line - 1;
    let mut ref_old: i64 = 0;
    let mut ref_new: i64 = 0;
    let mut has_ref = false;

    for hunk in hunks {
        let old = SpgSpan::from_old_hunk(hunk);
        let new = SpgSpan::from_new_hunk(hunk);

        if check < old.end {
            break;
        }

        ref_old = old.end;
        ref_new = new.end;
        has_ref = true;
    }

    if has_ref {
        line - ref_old + ref_new
    } else {
        line
    }
}

/// Compute surviving parts of a span after splitting around hunks and
/// mapping forward. This is the SPG equivalent of `moved_span` in the
/// original — it implements the "overhang" algorithm.
///
/// Uses `SpgSpan::from_old_hunk` for split boundaries (which adds +1
/// to `old_start` for pure insertions), matching the original's
/// `Span.from_old()` semantics.
pub(super) fn spg_moved_span(prev_new_span: &SpgSpan, hunks: &[HunkInfo]) -> Vec<SpgSpan> {
    if prev_new_span.is_empty() {
        return vec![];
    }

    let mut remaining = vec![(prev_new_span.start, prev_new_span.end)];
    for hunk in hunks {
        let old_span = SpgSpan::from_old_hunk(hunk);
        let old_start = old_span.start;
        let old_end = old_span.end;
        let mut next = Vec::new();
        for (s, e) in remaining {
            if e <= old_start || s >= old_end {
                next.push((s, e));
            } else {
                if s < old_start {
                    next.push((s, old_start));
                }
                if e > old_end {
                    next.push((old_end, e));
                }
            }
        }
        remaining = next;
    }

    remaining
        .into_iter()
        .filter(|(s, e)| e > s)
        .map(|(s, e)| SpgSpan {
            start: spg_map_start(s, hunks),
            end: spg_map_end(e, hunks),
        })
        .filter(|sp| !sp.is_empty())
        .collect()
}

/// Register edges from overlapping prev_nodes to a new node.
///
/// Uses multi-level overlap priority matching the original fragmap:
/// 1. Register ALL prev_nodes with interval overlap
///
/// 2–5. Fallback levels with point-overlap filters (register at most one)
fn spg_add_on_top_of(spg: &mut Spg, prev_nodes: &[SpgNode], node: &SpgNode) {
    let cur_range = &node.old_span;
    let mut registered = false;

    // Level 1: register ALL prev_nodes with INTERVAL_OVERLAP
    for prev in prev_nodes {
        if cur_range.overlap(&prev.new_span) == SpgOverlap::Interval {
            spg.register(prev, node);
            registered = true;
        }
    }

    // Level 2: any overlap, excluding point-on-border to downstream-from-active
    if !registered {
        for prev in prev_nodes {
            let ov = cur_range.overlap(&prev.new_span);
            if ov != SpgOverlap::None {
                let on_border =
                    cur_range.start == prev.new_span.start || cur_range.end == prev.new_span.end;
                let is_dfa = spg
                    .downstream_from_active
                    .get(prev)
                    .copied()
                    .unwrap_or(false);
                if !(ov == SpgOverlap::Point && on_border && is_dfa) {
                    spg.register(prev, node);
                    registered = true;
                    break;
                }
            }
        }
    }

    // Level 3: any overlap, excluding point-on-border to active nodes
    if !registered {
        for prev in prev_nodes {
            let ov = cur_range.overlap(&prev.new_span);
            if ov != SpgOverlap::None {
                let on_border =
                    cur_range.start == prev.new_span.start || cur_range.end == prev.new_span.end;
                if !(ov == SpgOverlap::Point && on_border && prev.is_active) {
                    spg.register(prev, node);
                    registered = true;
                    break;
                }
            }
        }
    }

    // Level 4: any overlap to inactive nodes only
    if !registered {
        for prev in prev_nodes {
            if cur_range.overlap(&prev.new_span) != SpgOverlap::None && !prev.is_active {
                spg.register(prev, node);
                registered = true;
                break;
            }
        }
    }

    // Level 5: any overlap at all
    if !registered {
        for prev in prev_nodes {
            if cur_range.overlap(&prev.new_span) != SpgOverlap::None {
                spg.register(prev, node);
                registered = true;
                break;
            }
        }
    }

    spg.register(node, &sink_node());
    debug_assert!(
        registered,
        "SPG: node {:?} has no overlap with any prev_node",
        node
    );
}

/// Handle prev_nodes that still point to SINK after all `add_on_top_of`
/// calls. Creates simple propagated copies so they remain reachable.
fn spg_update_dangling(spg: &mut Spg, prev_nodes: &[SpgNode], generation: i32) {
    let sink = sink_node();
    for prev in prev_nodes {
        let still_has_sink = spg
            .graph
            .get(prev)
            .map(|succs| succs.contains(&sink))
            .unwrap_or(false);
        if still_has_sink {
            let propagated = SpgNode {
                generation,
                is_active: false,
                old_span: prev.new_span,
                new_span: prev.new_span,
            };
            spg.register(prev, &propagated);
            spg.register(&propagated, &sink);
        }
    }
}

/// Recursively enumerate all paths from `source` to `sink` through the DAG.
fn spg_enumerate_paths(
    graph: &HashMap<SpgNode, Vec<SpgNode>>,
    source: &SpgNode,
    sink: &SpgNode,
) -> Vec<Vec<SpgNode>> {
    if source == sink {
        return vec![vec![sink.clone()]];
    }

    let succs = match graph.get(source) {
        Some(s) => s,
        None => return vec![],
    };

    let mut sorted_succs = succs.clone();
    sorted_succs.sort_by_key(|n| {
        (
            n.new_span.start,
            n.old_span.start,
            n.new_span.end,
            n.old_span.end,
        )
    });

    let mut paths = Vec::new();
    for succ in &sorted_succs {
        for mut sub_path in spg_enumerate_paths(graph, succ, sink) {
            sub_path.insert(0, source.clone());
            paths.push(sub_path);
        }
    }

    paths
}

/// Enumerate all unique paths through an SPG, deduplicated by active-node
/// signature and filtered to exclude empty paths (no active nodes).
/// Output is sorted by earliest active node position for deterministic ordering.
fn spg_all_paths(spg: &Spg) -> Vec<Vec<SpgNode>> {
    let source = source_node();
    let sink = sink_node();

    let raw_paths = spg_enumerate_paths(&spg.graph, &source, &sink);

    let mut seen: HashSet<Vec<(i32, SpgSpan)>> = HashSet::new();
    let mut result = Vec::new();
    for path in raw_paths {
        let key: Vec<(i32, SpgSpan)> = path
            .iter()
            .filter(|n| n.is_active)
            .map(|n| (n.generation, n.new_span))
            .collect();
        if !key.is_empty() && seen.insert(key) {
            result.push(path);
        }
    }

    // Sort by active node positions: first by generation, then by new_span.start
    result.sort_by(|a, b| {
        let a_key: Vec<(i32, i64)> = a
            .iter()
            .filter(|n| n.is_active)
            .map(|n| (n.generation, n.new_span.start))
            .collect();
        let b_key: Vec<(i32, i64)> = b
            .iter()
            .filter(|n| n.is_active)
            .map(|n| (n.generation, n.new_span.start))
            .collect();
        a_key.cmp(&b_key)
    });

    result
}

/// Build the SPG for a single file from its commits and hunks.
fn build_file_spg(commits: &[(usize, Vec<HunkInfo>)]) -> Spg {
    let mut spg = Spg::empty();

    for (commit_idx, hunks) in commits {
        let gen = *commit_idx as i32;

        let mut prev_nodes = spg.sink_connected_nodes();
        prev_nodes.retain(|n| !n.new_span.is_empty());
        prev_nodes.sort_by_key(|n| {
            (
                n.new_span.start,
                n.old_span.start,
                n.new_span.end,
                n.old_span.end,
            )
        });

        // Create active nodes for this commit's hunks
        let active_nodes: Vec<SpgNode> = hunks
            .iter()
            .map(|h| SpgNode {
                generation: gen,
                is_active: true,
                old_span: SpgSpan::from_old_hunk(h),
                new_span: SpgSpan::from_new_hunk(h),
            })
            .collect();

        // Propagate prev_nodes: split surviving parts around hunks
        let mut propagated_nodes: Vec<SpgNode> = Vec::new();
        for prev in &prev_nodes {
            for m in spg_moved_span(&prev.new_span, hunks) {
                propagated_nodes.push(SpgNode {
                    generation: gen,
                    is_active: false,
                    old_span: prev.new_span,
                    new_span: m,
                });
            }
        }

        // Combine active + propagated, sorted by old_span (node_by_old)
        let mut all_new_nodes = active_nodes;
        all_new_nodes.extend(propagated_nodes);
        all_new_nodes.sort_by_key(|n| {
            (
                n.old_span.start,
                n.new_span.start,
                n.old_span.end,
                n.new_span.end,
            )
        });

        for cur_node in &all_new_nodes {
            spg_add_on_top_of(&mut spg, &prev_nodes, cur_node);
        }

        spg_update_dangling(&mut spg, &prev_nodes, gen);
    }

    spg
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
    let mut seen: HashSet<Vec<String>> = HashSet::new();
    clusters.retain(|c| seen.insert(c.commit_oids.clone()));
}

/// Build all `SpanCluster` entries for a single file path.
///
/// Runs the SPG for the given file and converts each unique path through the
/// DAG into a `SpanCluster` that records which commits touch it.
pub(super) fn build_file_clusters(
    path: &str,
    commits_for_file: &[(usize, Vec<HunkInfo>)],
    commit_diffs: &[CommitDiff],
) -> Vec<SpanCluster> {
    let spg = build_file_spg(commits_for_file);
    let paths = spg_all_paths(&spg);
    let mut clusters = Vec::new();

    for path_nodes in &paths {
        let mut commit_oids: Vec<String> = Vec::new();
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

        if let Some(sp) = last_active_span {
            if !commit_oids.is_empty() {
                clusters.push(SpanCluster {
                    spans: vec![FileSpan {
                        path: path.to_string(),
                        start_line: sp.start.max(1) as u32,
                        end_line: (sp.end - 1).max(1) as u32,
                    }],
                    commit_oids,
                });
            }
        }
    }

    clusters
}

/// Enumerate all SPG paths for each file and the raw path count.
/// Used by `dump_per_file_spg_stats`.
pub(super) fn enumerate_file_spg_paths(
    commits: &[(usize, Vec<HunkInfo>)],
) -> (usize, usize, usize) {
    let spg = build_file_spg(commits);
    let node_count = spg.graph.len();
    let raw_paths = spg_enumerate_paths(&spg.graph, &source_node(), &sink_node());
    let deduped_paths = spg_all_paths(&spg);
    (node_count, raw_paths.len(), deduped_paths.len())
}

/// Diagnostic: dump per-file SPG stats (for debugging, not used in production).
#[doc(hidden)]
pub fn dump_per_file_spg_stats(commit_diffs: &[CommitDiff]) {
    let mut file_commits: HashMap<String, Vec<(usize, Vec<HunkInfo>)>> = HashMap::new();

    for (commit_idx, diff) in commit_diffs.iter().enumerate() {
        for file in &diff.files {
            let path = match &file.new_path {
                Some(p) => p.clone(),
                None => continue,
            };
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
                let entry = file_commits.entry(path).or_default();
                if let Some(last) = entry.last_mut() {
                    if last.0 == commit_idx {
                        last.1.extend(hunks);
                        continue;
                    }
                }
                entry.push((commit_idx, hunks));
            }
        }
    }

    let mut sorted_paths: Vec<&String> = file_commits.keys().collect();
    sorted_paths.sort();

    for path in sorted_paths {
        let commits_for_file = &file_commits[path];
        let (node_count, raw_path_count, deduped_path_count) =
            enumerate_file_spg_paths(commits_for_file);
        let gens: Vec<usize> = commits_for_file.iter().map(|(g, _)| *g).collect();
        eprintln!(
            "FILE: {} | gens={:?} | nodes={} | raw_paths={} | deduped_paths={}",
            path, gens, node_count, raw_path_count, deduped_path_count
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================
    // SpgSpan::overlap() — the fundamental SPG primitive
    // =========================================================

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

    // =========================================================
    // SpgSpan::from_old_hunk / from_new_hunk
    // =========================================================

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

    // =========================================================
    // spg_map_start / spg_map_end
    // =========================================================

    // Both functions use HunkInfo { old_start:10, old_lines:5, new_start:10, new_lines:8 }
    // → from_old_hunk: [10,15), from_new_hunk: [10,18), delta = +3.

    #[test]
    fn spg_map_start_before_hunk_no_shift() {
        let h = vec![HunkInfo {
            old_start: 10,
            old_lines: 5,
            new_start: 10,
            new_lines: 8,
        }];
        // line=5 < old.end=15 → break, has_ref=false → no shift
        assert_eq!(spg_map_start(5, &h), 5);
    }

    #[test]
    fn spg_map_start_exactly_at_old_end_boundary() {
        let h = vec![HunkInfo {
            old_start: 10,
            old_lines: 5,
            new_start: 10,
            new_lines: 8,
        }];
        // line=15 NOT < 15 → ref_old=15, ref_new=18 → 15-15+18=18
        assert_eq!(spg_map_start(15, &h), 18);
    }

    #[test]
    fn spg_map_end_before_hunk_no_shift() {
        let h = vec![HunkInfo {
            old_start: 10,
            old_lines: 5,
            new_start: 10,
            new_lines: 8,
        }];
        // line=15, check=14 < old.end=15 → break, has_ref=false → no shift
        assert_eq!(spg_map_end(15, &h), 15);
    }

    #[test]
    fn spg_map_end_after_hunk_shifted() {
        let h = vec![HunkInfo {
            old_start: 10,
            old_lines: 5,
            new_start: 10,
            new_lines: 8,
        }];
        // line=20, check=19 NOT < 15 → ref_old=15, ref_new=18 → 20-15+18=23
        assert_eq!(spg_map_end(20, &h), 23);
    }

    // =========================================================
    // spg_moved_span edge cases
    // =========================================================

    #[test]
    fn spg_moved_span_entirely_before_hunk_unchanged() {
        // Span [1,5) with hunk old=[10,15): span ends before hunk → passes unchanged.
        let h = vec![HunkInfo {
            old_start: 10,
            old_lines: 5,
            new_start: 10,
            new_lines: 8,
        }];
        let result = spg_moved_span(&SpgSpan { start: 1, end: 5 }, &h);
        assert_eq!(result, vec![SpgSpan { start: 1, end: 5 }]);
    }

    #[test]
    fn spg_moved_span_entirely_after_hunk_shifted() {
        // Span [20,25) with hunk old=[5,10), new=[5,15): delta +5.
        // old.end=10, new.end=15. start: 20-10+15=25. end: 25-10+15=30.
        let h = vec![HunkInfo {
            old_start: 5,
            old_lines: 5,
            new_start: 5,
            new_lines: 10,
        }];
        let result = spg_moved_span(&SpgSpan { start: 20, end: 25 }, &h);
        assert_eq!(result, vec![SpgSpan { start: 25, end: 30 }]);
    }

    #[test]
    fn spg_moved_span_entirely_consumed_by_deletion() {
        // Span [10,15) with a hunk that deletes exactly [10,15).
        // After split: neither fragment survives → empty.
        let h = vec![HunkInfo {
            old_start: 10,
            old_lines: 5,
            new_start: 10,
            new_lines: 0,
        }];
        let result = spg_moved_span(&SpgSpan { start: 10, end: 15 }, &h);
        assert!(result.is_empty());
    }

    #[test]
    fn spg_moved_span_split_around_hunk() {
        // Span [5,20) with hunk old=[10,15), new=[10,18): split into before and after.
        // [5,10) → unchanged. [15,20) → 15-15+18=18, 20-15+18=23.
        let h = vec![HunkInfo {
            old_start: 10,
            old_lines: 5,
            new_start: 10,
            new_lines: 8,
        }];
        let result = spg_moved_span(&SpgSpan { start: 5, end: 20 }, &h);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], SpgSpan { start: 5, end: 10 });
        assert_eq!(result[1], SpgSpan { start: 18, end: 23 });
    }

    #[test]
    fn spg_moved_span_pure_insertion_hunk_shifts_later_span() {
        // Hunk: pure insertion at old_start=5, old_lines=0 → from_old_hunk gives [6,6) (empty).
        // Span [10,15) starts after the empty old_span, so splits around [6,6):
        //   s=10 >= old_end=6 → push (10,15) unchanged in split.
        // Map: old.end=6, new.end=8 (5+3). ref_old=6, ref_new=8.
        //   start: 10-6+8=12. end: 15-6+8=17.
        let h = vec![HunkInfo {
            old_start: 5,
            old_lines: 0,
            new_start: 5,
            new_lines: 3,
        }];
        let result = spg_moved_span(&SpgSpan { start: 10, end: 15 }, &h);
        assert_eq!(result, vec![SpgSpan { start: 12, end: 17 }]);
    }
}