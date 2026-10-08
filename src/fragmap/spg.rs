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

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use crate::{CommitDiff, VirtualOid};

use super::{CommitPos, FileId, FileSpan, HunkInfo, SpanCluster};
use std::path::Path;

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

const SOURCE: usize = 0;
const SINK: usize = 1;

/// The Span Propagation Graph for one file. Nodes are numbered in the order
/// they are first seen; equal nodes are one node.
struct Spg {
    nodes: Vec<SpgNode>,
    index: HashMap<SpgNode, usize>,
    succs: Vec<Vec<usize>>,
    downstream_from_active: Vec<bool>,
    /// Every node that has had an edge to SINK, possibly since replaced.
    frontier: Vec<usize>,
}

impl Spg {
    fn empty() -> Self {
        let mut spg = Spg {
            nodes: Vec::new(),
            index: HashMap::new(),
            succs: Vec::new(),
            downstream_from_active: Vec::new(),
            frontier: Vec::new(),
        };
        let source = spg.node(source_node());
        let sink = spg.node(sink_node());
        debug_assert_eq!((source, sink), (SOURCE, SINK));
        spg.register(SOURCE, SINK);
        spg
    }

    fn node(&mut self, node: SpgNode) -> usize {
        if let Some(&existing) = self.index.get(&node) {
            return existing;
        }
        let id = self.nodes.len();
        self.downstream_from_active.push(node.is_active);
        self.succs.push(Vec::new());
        self.index.insert(node.clone(), id);
        self.nodes.push(node);
        id
    }

    /// Register an edge from `from` to `to`, removing any existing SINK edge
    /// from `from`. This is the core SPG mutation: when a node gets a real
    /// successor, it no longer points directly to SINK.
    fn register(&mut self, from: usize, to: usize) {
        let succs = &mut self.succs[from];
        succs.retain(|&n| n != SINK);
        succs.push(to);
        if to == SINK {
            self.frontier.push(from);
        }
        self.downstream_from_active[to] |= self.downstream_from_active[from];
    }

    /// Find all nodes that have SINK as a direct successor (the current frontier).
    fn sink_connected_nodes(&mut self) -> Vec<usize> {
        self.frontier.sort_unstable();
        self.frontier.dedup();
        let succs = &self.succs;
        self.frontier.retain(|&n| succs[n].contains(&SINK));
        self.frontier.clone()
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
fn spg_add_on_top_of(spg: &mut Spg, prev_nodes: &[usize], node: SpgNode) {
    let cur_range = node.old_span;
    let node = spg.node(node);
    let mut registered = false;

    // Level 1: register ALL prev_nodes with INTERVAL_OVERLAP
    for &prev in prev_nodes {
        if cur_range.overlap(&spg.nodes[prev].new_span) == SpgOverlap::Interval {
            spg.register(prev, node);
            registered = true;
        }
    }

    // Level 2: any overlap, excluding point-on-border to downstream-from-active
    if !registered {
        for &prev in prev_nodes {
            let prev_span = spg.nodes[prev].new_span;
            let ov = cur_range.overlap(&prev_span);
            if ov != SpgOverlap::None {
                let on_border =
                    cur_range.start == prev_span.start || cur_range.end == prev_span.end;
                let is_dfa = spg.downstream_from_active[prev];
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
        for &prev in prev_nodes {
            let prev_node = &spg.nodes[prev];
            let ov = cur_range.overlap(&prev_node.new_span);
            if ov != SpgOverlap::None {
                let on_border = cur_range.start == prev_node.new_span.start
                    || cur_range.end == prev_node.new_span.end;
                if !(ov == SpgOverlap::Point && on_border && prev_node.is_active) {
                    spg.register(prev, node);
                    registered = true;
                    break;
                }
            }
        }
    }

    // Level 4: any overlap to inactive nodes only
    if !registered {
        for &prev in prev_nodes {
            let prev_node = &spg.nodes[prev];
            if cur_range.overlap(&prev_node.new_span) != SpgOverlap::None && !prev_node.is_active {
                spg.register(prev, node);
                registered = true;
                break;
            }
        }
    }

    // Level 5: any overlap at all
    if !registered {
        for &prev in prev_nodes {
            if cur_range.overlap(&spg.nodes[prev].new_span) != SpgOverlap::None {
                spg.register(prev, node);
                registered = true;
                break;
            }
        }
    }

    spg.register(node, SINK);
    debug_assert!(
        registered,
        "SPG: node {:?} has no overlap with any prev_node",
        spg.nodes[node]
    );
}

/// Handle prev_nodes that still point to SINK after all `add_on_top_of`
/// calls. Creates simple propagated copies so they remain reachable.
fn spg_update_dangling(spg: &mut Spg, prev_nodes: &[usize], generation: i32) {
    for &prev in prev_nodes {
        if spg.succs[prev].contains(&SINK) {
            let span = spg.nodes[prev].new_span;
            let propagated = spg.node(SpgNode {
                generation,
                is_active: false,
                old_span: span,
                new_span: span,
            });
            spg.register(prev, propagated);
            spg.register(propagated, SINK);
        }
    }
}

/// `node`'s successors in the order paths through them are listed.
fn sorted_succs(spg: &Spg, node: usize) -> Vec<usize> {
    let mut sorted = spg.succs[node].clone();
    sorted.sort_by_key(|&n| {
        let n = &spg.nodes[n];
        (
            n.new_span.start,
            n.old_span.start,
            n.new_span.end,
            n.old_span.end,
        )
    });
    sorted
}

/// Recursively enumerate all paths from `from` to SINK through the DAG.
fn spg_enumerate_paths(
    spg: &Spg,
    from: usize,
    poll: &mut impl FnMut() -> bool,
) -> Option<Vec<Vec<SpgNode>>> {
    if from == SINK {
        return Some(vec![vec![spg.nodes[SINK].clone()]]);
    }

    let mut paths = Vec::new();
    for succ in sorted_succs(spg, from) {
        if !poll() {
            return None;
        }
        for mut sub_path in spg_enumerate_paths(spg, succ, poll)? {
            sub_path.insert(0, spg.nodes[from].clone());
            paths.push(sub_path);
        }
    }

    Some(paths)
}

/// Enumerate all unique paths through an SPG, deduplicated by active-node
/// signature and filtered to exclude empty paths (no active nodes).
/// Output is sorted by earliest active node position for deterministic ordering.
fn spg_all_paths(spg: &Spg, poll: &mut impl FnMut() -> bool) -> Option<Vec<Vec<SpgNode>>> {
    let raw_paths = spg_enumerate_paths(spg, SOURCE, poll)?;

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

    Some(result)
}

const NIL: u32 = u32::MAX;

/// Singly linked lists in one arena, sharing tails, so that extending a path
/// by one node does not copy the rest of it.
struct ConsArena<T> {
    cells: Vec<(T, u32)>,
}

impl<T: Copy + Ord> ConsArena<T> {
    fn new() -> Self {
        ConsArena { cells: Vec::new() }
    }

    fn cons(&mut self, head: T, tail: u32) -> u32 {
        self.cells.push((head, tail));
        (self.cells.len() - 1) as u32
    }

    fn iter(&self, mut list: u32) -> impl Iterator<Item = T> + '_ {
        std::iter::from_fn(move || {
            let (head, tail) = *self.cells.get(list as usize)?;
            list = tail;
            Some(head)
        })
    }

    fn cmp(&self, mut a: u32, mut b: u32) -> Ordering {
        while a != b {
            match (a, b) {
                (NIL, _) => return Ordering::Less,
                (_, NIL) => return Ordering::Greater,
                _ => {}
            }
            let (head_a, tail_a) = self.cells[a as usize];
            let (head_b, tail_b) = self.cells[b as usize];
            match head_a.cmp(&head_b) {
                Ordering::Equal => (a, b) = (tail_a, tail_b),
                unequal => return unequal,
            }
        }
        Ordering::Equal
    }
}

/// The graph's nodes, each node's successors in the order
/// `spg_enumerate_paths` visits them, and the nodes in generation order —
/// a topological order, since every edge leads to a later generation.
struct IndexedSpg<'a> {
    nodes: &'a [SpgNode],
    succs: Vec<Vec<usize>>,
    by_generation: Vec<usize>,
    source: usize,
    sink: usize,
}

impl<'a> IndexedSpg<'a> {
    fn new(spg: &'a Spg) -> Self {
        let nodes = &spg.nodes[..];
        let succs: Vec<Vec<usize>> = (0..nodes.len()).map(|n| sorted_succs(spg, n)).collect();
        let mut by_generation: Vec<usize> = (0..nodes.len()).collect();
        by_generation.sort_by_key(|&i| nodes[i].generation);
        debug_assert!(succs.iter().enumerate().all(|(from, to)| {
            to.iter()
                .all(|&to| nodes[to].generation > nodes[from].generation)
        }));
        IndexedSpg {
            nodes,
            succs,
            by_generation,
            source: SOURCE,
            sink: SINK,
        }
    }
}

/// One column of a file: the generations of the commits touching it, and the
/// span of the last of them.
struct SpgColumn {
    generations: Vec<i32>,
    last_span: SpgSpan,
}

/// Every distinct set of commits a path through the graph touches, each with
/// the first such path in `spg_all_paths` order, listed in that order.
///
/// This is what the deduplicated matrix keeps of `spg_all_paths`, computed
/// without listing the paths: for each node, from the sink back, the best
/// suffix per commit set, keyed like `spg_all_paths` sorts. Two paths of one
/// set compare as their suffixes do when their prefixes agree, so the best
/// suffix per set is all a node needs. Successors are visited in enumeration
/// order and only a smaller key replaces the best, so of tied paths the one
/// `spg_all_paths` keeps wins; paths of different sets never tie.
fn spg_columns(spg: &Spg, poll: &mut impl FnMut() -> bool) -> Option<Vec<SpgColumn>> {
    let graph = IndexedSpg::new(spg);
    let mut keys: ConsArena<(i32, i64)> = ConsArena::new();
    let mut sets: ConsArena<i32> = ConsArena::new();
    let mut interned_sets: HashMap<(i32, u32), u32> = HashMap::new();

    let mut pending_preds = vec![0usize; graph.nodes.len()];
    for to in graph.succs.iter().flatten() {
        pending_preds[*to] += 1;
    }

    // Per commit set: the best suffix's key and its last active span.
    type Suffixes = HashMap<u32, (u32, Option<SpgSpan>)>;
    let mut suffixes: Vec<Option<Suffixes>> = (0..graph.nodes.len()).map(|_| None).collect();
    suffixes[graph.sink] = Some(HashMap::from([(NIL, (NIL, None))]));

    for &at in graph.by_generation.iter().rev() {
        if at == graph.sink {
            continue;
        }
        if !poll() {
            return None;
        }
        // Keyed by the successor's set and holding the successor's key: the
        // node itself is prepended to all of them alike once the best is known.
        let mut best: Suffixes = HashMap::new();
        for &next in &graph.succs[at] {
            let next_suffixes = suffixes[next].as_ref().expect("successors come first");
            for (&set, &(key, span)) in next_suffixes {
                let better = best
                    .get(&set)
                    .is_none_or(|&(current, _)| keys.cmp(key, current).is_lt());
                if better {
                    best.insert(set, (key, span));
                }
            }
        }

        let node = &graph.nodes[at];
        suffixes[at] = Some(if !node.is_active {
            best
        } else {
            best.into_iter()
                .map(|(set, (key, span))| {
                    let set = *interned_sets
                        .entry((node.generation, set))
                        .or_insert_with(|| sets.cons(node.generation, set));
                    let key = keys.cons((node.generation, node.new_span.start), key);
                    (set, (key, span.or(Some(node.new_span))))
                })
                .collect()
        });

        for &next in &graph.succs[at] {
            pending_preds[next] -= 1;
            if pending_preds[next] == 0 {
                suffixes[next] = None;
            }
        }
    }

    let mut columns: Vec<(u32, u32, SpgSpan)> = suffixes[graph.source]
        .take()
        .expect("source is never freed")
        .into_iter()
        .filter_map(|(set, (key, span))| Some((key, set, span?)))
        .collect();
    columns.sort_by(|(a, _, _), (b, _, _)| keys.cmp(*a, *b));
    Some(
        columns
            .into_iter()
            .map(|(_, set, last_span)| SpgColumn {
                generations: sets.iter(set).collect(),
                last_span,
            })
            .collect(),
    )
}

/// For each of `target`'s hunk spans — its active nodes — the generations of the
/// commits on the first path through it in `spg_all_paths` order, hunks
/// listed in the order of those paths. Hunks whose first paths tie come in
/// span order rather than enumeration order; they touch the same commits.
///
/// The first path through a node is the best prefix to it followed by the
/// best suffix from it: every generation after the node is later than every
/// one before it, so a prefix that ends early only sorts after the prefixes
/// that continue it. Paths that tie have the same commits, so which of them
/// wins does not matter.
fn spg_target_columns(
    spg: &Spg,
    target: i32,
    poll: &mut impl FnMut() -> bool,
) -> Option<Vec<(SpgSpan, Vec<i32>)>> {
    let graph = IndexedSpg::new(spg);
    let mut keys: ConsArena<(i32, i64)> = ConsArena::new();
    let key_of = |node: &SpgNode| (node.generation, node.new_span.start);

    // From each node to the sink, the node included.
    let mut suffix: Vec<Option<u32>> = vec![None; graph.nodes.len()];
    suffix[graph.sink] = Some(NIL);
    for &at in graph.by_generation.iter().rev() {
        if at == graph.sink {
            continue;
        }
        if !poll() {
            return None;
        }
        let mut best: Option<u32> = None;
        for &next in &graph.succs[at] {
            if let Some(key) = suffix[next]
                && best.is_none_or(|current| keys.cmp(key, current).is_lt())
            {
                best = Some(key);
            }
        }
        let node = &graph.nodes[at];
        suffix[at] = best.map(|key| match node.is_active {
            true => keys.cons(key_of(node), key),
            false => key,
        });
    }

    // From the source to each node, the node excluded, last element first.
    let mut reversed: ConsArena<(i32, i64)> = ConsArena::new();
    let in_order = |reversed: &ConsArena<(i32, i64)>, list: u32| {
        let mut elements: Vec<(i32, i64)> = reversed.iter(list).collect();
        elements.reverse();
        elements
    };
    let ends_first =
        |a: &[(i32, i64)], b: &[(i32, i64)]| match a.iter().zip(b).find(|(x, y)| x != y) {
            Some((x, y)) => x < y,
            None => a.len() > b.len(),
        };
    let mut prefix: Vec<Option<u32>> = vec![None; graph.nodes.len()];
    prefix[graph.source] = Some(NIL);
    for &at in &graph.by_generation {
        let Some(before) = prefix[at] else { continue };
        if !poll() {
            return None;
        }
        let node = &graph.nodes[at];
        let through = match node.is_active {
            true => reversed.cons(key_of(node), before),
            false => before,
        };
        let through_in_order = in_order(&reversed, through);
        for &next in &graph.succs[at] {
            let better = prefix[next]
                .is_none_or(|current| ends_first(&through_in_order, &in_order(&reversed, current)));
            if better {
                prefix[next] = Some(through);
            }
        }
    }

    let mut columns: Vec<(Vec<(i32, i64)>, SpgSpan)> = (0..graph.nodes.len())
        .filter(|&at| graph.nodes[at].is_active && graph.nodes[at].generation == target)
        .filter_map(|at| {
            let mut path = in_order(&reversed, prefix[at]?);
            path.extend(keys.iter(suffix[at]?));
            Some((path, graph.nodes[at].new_span))
        })
        .collect();
    columns.sort_by_key(|(path, span)| (path.clone(), span.end));
    // `spg_all_paths` tells nodes apart by their new span alone.
    let mut seen = HashSet::new();
    columns.retain(|(_, span)| seen.insert(*span));
    Some(
        columns
            .into_iter()
            .map(|(path, span)| {
                (
                    span,
                    path.iter().map(|&(generation, _)| generation).collect(),
                )
            })
            .collect(),
    )
}

/// Build the SPG for a single file from its commits and hunks.
///
/// `poll` is called after each commit generation. Return `false` from `poll`
/// to interrupt early; in that case the function returns `None`.
fn build_file_spg(
    commits: &[(CommitPos, Vec<HunkInfo>)],
    poll: &mut impl FnMut() -> bool,
) -> Option<Spg> {
    let mut spg = Spg::empty();
    let mut last_gen: Option<i32> = None;

    for (commit, hunks) in commits {
        let commit_gen = commit.0 as i32;

        // When commits that touch this file are non-consecutive (e.g.
        // generations 0 and 5, with 1–4 not touching the file), the
        // original fragmap calls update_unchanged_file() at every
        // intermediate generation.  That converts active frontier nodes
        // into inactive propagated copies.
        //
        // This matters because spg_add_on_top_of rejects a
        // point-on-border overlap with active nodes but accepts it for
        // inactive ones — so without this step, the wrong predecessor
        // gets chosen when the gap is followed by a commit whose hunk
        // starts or ends exactly at a surviving span boundary.
        //
        // One propagation step at (commit_gen - 1) is enough: spans
        // don't change across a gap (no hunks), so it is equivalent to
        // the full chain.
        let prev_gen = last_gen.unwrap_or(commit_gen - 1);
        if commit_gen > prev_gen + 1 {
            let gap_nodes = spg.sink_connected_nodes();
            let gap_gen = commit_gen - 1;
            for node in gap_nodes {
                let span = spg.nodes[node].new_span;
                if span.is_empty() {
                    continue;
                }
                let propagated = spg.node(SpgNode {
                    generation: gap_gen,
                    is_active: false,
                    old_span: span,
                    new_span: span,
                });
                spg.register(node, propagated);
                spg.register(propagated, SINK);
            }
        }
        last_gen = Some(commit_gen);

        let mut prev_nodes = spg.sink_connected_nodes();
        prev_nodes.retain(|&n| !spg.nodes[n].new_span.is_empty());
        prev_nodes.sort_by_key(|&n| {
            let n = &spg.nodes[n];
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
                generation: commit_gen,
                is_active: true,
                old_span: SpgSpan::from_old_hunk(h),
                new_span: SpgSpan::from_new_hunk(h),
            })
            .collect();

        // Propagate prev_nodes: split surviving parts around hunks
        let mut propagated_nodes: Vec<SpgNode> = Vec::new();
        for &prev in &prev_nodes {
            let prev_span = spg.nodes[prev].new_span;
            for m in spg_moved_span(&prev_span, hunks) {
                propagated_nodes.push(SpgNode {
                    generation: commit_gen,
                    is_active: false,
                    old_span: prev_span,
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

        for cur_node in all_new_nodes {
            spg_add_on_top_of(&mut spg, &prev_nodes, cur_node);
            if !poll() {
                return None;
            }
        }

        spg_update_dangling(&mut spg, &prev_nodes, commit_gen);

        if !poll() {
            return None;
        }
    }

    Some(spg)
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

    // =========================================================
    // spg_columns — against listing every path
    // =========================================================

    /// A layered graph whose nodes share starts often enough that paths tie on
    /// their active `(generation, start)` and only their enumeration order
    /// tells them apart, with duplicate edges among them.
    fn random_graph(rng: &mut super::super::tests::XorShift) -> Spg {
        let layers: Vec<Vec<SpgNode>> = (0..2 + rng.below(5) as i32)
            .map(|generation| {
                (0..1 + rng.below(4))
                    .map(|_| {
                        let span = |rng: &mut super::super::tests::XorShift| {
                            let start = rng.below(3) as i64;
                            SpgSpan {
                                start,
                                end: start + rng.below(3) as i64,
                            }
                        };
                        SpgNode {
                            generation,
                            is_active: rng.below(2) == 0,
                            old_span: span(rng),
                            new_span: span(rng),
                        }
                    })
                    .collect()
            })
            .collect();
        let mut spg = Spg::empty();
        spg.succs[SOURCE].clear();
        let mut froms = vec![SOURCE];
        for (depth, layer) in layers.iter().enumerate() {
            for &from in &froms {
                for to in layers[depth..].iter().flatten() {
                    for _ in 0..rng.below(4).saturating_sub(1) {
                        if to.generation > spg.nodes[from].generation {
                            let to = spg.node(to.clone());
                            spg.succs[from].push(to);
                        }
                    }
                }
                if rng.below(3) == 0 || spg.succs[from].is_empty() {
                    spg.succs[from].push(SINK);
                }
            }
            froms = layer.iter().map(|n| spg.node(n.clone())).collect();
        }
        for from in froms {
            spg.succs[from].push(SINK);
        }
        spg
    }

    #[test]
    fn columns_are_the_first_path_of_each_commit_set() {
        for seed in 0..3000 {
            let mut rng = super::super::tests::XorShift(seed * 2 + 1);
            let spg = random_graph(&mut rng);

            let mut seen = HashSet::new();
            let mut expected = Vec::new();
            for path in spg_all_paths(&spg, &mut || true).unwrap() {
                let active: Vec<&SpgNode> = path.iter().filter(|n| n.is_active).collect();
                let generations: Vec<i32> = active.iter().map(|n| n.generation).collect();
                if seen.insert(generations.clone()) {
                    expected.push((generations, active.last().unwrap().new_span));
                }
            }
            let columns: Vec<(Vec<i32>, SpgSpan)> = spg_columns(&spg, &mut || true)
                .unwrap()
                .into_iter()
                .map(|c| (c.generations, c.last_span))
                .collect();
            assert_eq!(columns, expected, "seed {seed}");
        }
    }

    /// What the first path through each of `target`'s nodes is, by listing
    /// every path.
    fn first_paths_through(spg: &Spg, target: i32) -> Vec<(SpgSpan, Vec<i32>)> {
        let mut seen = HashSet::new();
        let mut expected = Vec::new();
        for path in spg_all_paths(spg, &mut || true).unwrap() {
            let Some(node) = path.iter().find(|n| n.is_active && n.generation == target) else {
                continue;
            };
            if seen.insert(node.new_span) {
                let generations = path
                    .iter()
                    .filter(|n| n.is_active)
                    .map(|n| n.generation)
                    .collect();
                expected.push((node.new_span, generations));
            }
        }
        expected
    }

    /// Only which commits each hunk's column holds, and the order of columns
    /// that hold different commits, reach the caller.
    fn assert_same_columns(
        actual: Vec<(SpgSpan, Vec<i32>)>,
        expected: Vec<(SpgSpan, Vec<i32>)>,
        context: &str,
    ) {
        let commits = |columns: &[(SpgSpan, Vec<i32>)]| -> Vec<Vec<i32>> {
            columns.iter().map(|(_, c)| c.clone()).collect()
        };
        assert_eq!(commits(&actual), commits(&expected), "{context}");
        let sorted = |mut columns: Vec<(SpgSpan, Vec<i32>)>| {
            columns.sort_by_key(|(span, c)| (span.start, span.end, c.clone()));
            columns
        };
        assert_eq!(sorted(actual), sorted(expected), "{context}");
    }

    #[test]
    fn target_columns_follow_the_first_path_through_each_node() {
        for seed in 0..3000 {
            let mut rng = super::super::tests::XorShift(seed * 2 + 1);
            let spg = random_graph(&mut rng);
            for target in 0..6 {
                assert_same_columns(
                    spg_target_columns(&spg, target, &mut || true).unwrap(),
                    first_paths_through(&spg, target),
                    &format!("seed {seed}, target {target}"),
                );
            }
        }
    }

    #[test]
    fn target_columns_follow_the_first_path_through_each_hunk() {
        for seed in 0..1000 {
            let diffs = super::super::tests::random_history(seed);
            let lineages = super::super::FileLineages::new(&diffs);
            for commits in super::super::collect_file_commits(&diffs, &lineages).values() {
                let spg = build_file_spg(commits, &mut || true).unwrap();
                for &(target, _) in commits {
                    let target = target.0 as i32;
                    assert_same_columns(
                        spg_target_columns(&spg, target, &mut || true).unwrap(),
                        first_paths_through(&spg, target),
                        &format!("seed {seed}, target {target}"),
                    );
                }
            }
        }
    }
}
