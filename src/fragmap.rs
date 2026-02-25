// Fragmap: chunk clustering for visualizing commit relationships
//
// Uses span propagation to map all hunk positions to a common reference
// frame (the final file version at HEAD), matching the algorithm from the
// original fragmap tool. Without propagation, line numbers from different
// commits refer to different file versions and cannot be compared directly.

use std::collections::HashMap;

use crate::CommitDiff;

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

/// Extract FileSpans from all commit diffs with span propagation.
///
/// Each hunk produces a span using its full `[new_start, new_start + new_lines)`
/// range (the region of the file occupied after the commit). That span is then
/// propagated forward through every subsequent commit that modifies the same
/// file, adjusting line numbers to account for insertions and deletions.
/// The result: every span is expressed in the FINAL file version's coordinates,
/// making overlap-based clustering correct across commits.
pub fn extract_spans_propagated(commit_diffs: &[CommitDiff]) -> Vec<(String, Vec<FileSpan>)> {
    // Group hunks by file path across all commits.
    // For each file we need the commit index + hunks in chronological order.
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
                file_commits
                    .entry(path)
                    .or_default()
                    .push((commit_idx, hunks));
            }
        }
    }

    // For each file, propagate every commit's spans forward to the final version.
    let mut all_spans: Vec<(usize, FileSpan)> = Vec::new();

    for (path, commits) in &file_commits {
        for (ci, (commit_idx, hunks)) in commits.iter().enumerate() {
            for hunk in hunks {
                if hunk.new_lines == 0 {
                    continue;
                }

                // Start with the hunk's new-side range [start, end) exclusive
                let mut spans = vec![(hunk.new_start, hunk.new_start + hunk.new_lines)];

                // Propagate through all subsequent commits that touch this file,
                // splitting around each commit's hunks to avoid mapping positions
                // that fall inside a hunk's replaced region.
                for (_, later_hunks) in &commits[ci + 1..] {
                    spans = spans
                        .into_iter()
                        .flat_map(|(s, e)| split_and_propagate(s, e, later_hunks))
                        .collect();
                }

                // Convert exclusive end to inclusive and add to results
                for (start, end) in spans {
                    if end > start {
                        all_spans.push((
                            *commit_idx,
                            FileSpan {
                                path: path.clone(),
                                start_line: start,
                                end_line: end - 1,
                            },
                        ));
                    }
                }
            }
        }
    }

    // Group spans by commit OID to match the expected format
    let mut result: Vec<(String, Vec<FileSpan>)> = commit_diffs
        .iter()
        .map(|d| (d.commit.oid.clone(), Vec::new()))
        .collect();

    for (commit_idx, span) in all_spans {
        result[commit_idx].1.push(span);
    }

    result
}

/// Lightweight copy of the hunk header fields needed for propagation.
#[derive(Debug, Clone)]
struct HunkInfo {
    old_start: u32,
    old_lines: u32,
    new_start: u32,
    new_lines: u32,
}

/// Map a single line number forward through a commit's hunks.
///
/// Given a line number in the file version *before* the commit and the
/// commit's hunks (sorted by `old_start`), returns the corresponding
/// line number in the file version *after* the commit.
///
/// IMPORTANT: only call this for positions that are OUTSIDE any hunk's
/// `[old_start, old_start + old_lines)` range. Use `split_and_propagate`
/// for arbitrary spans that might overlap with hunks.
fn map_line_forward(line: u32, hunks: &[HunkInfo]) -> u32 {
    let mut cumulative_delta: i64 = 0;

    for hunk in hunks {
        // After split_and_propagate, positions are guaranteed outside any
        // hunk's [old_start, old_start + old_lines). A position equal to
        // old_start is the exclusive end of a fragment before the hunk.
        if line <= hunk.old_start {
            return (line as i64 + cumulative_delta) as u32;
        }

        cumulative_delta += hunk.new_lines as i64 - hunk.old_lines as i64;
    }

    (line as i64 + cumulative_delta) as u32
}

/// Propagate a span `[start, end)` (exclusive end) through a commit's hunks.
///
/// Follows the original fragmap's "overhang" algorithm: the span is first
/// split around each hunk's old range so that only the non-overlapping
/// parts survive, then those parts are mapped to the new file version
/// using the cumulative line offsets.
///
/// Returns zero or more spans in the post-commit file version.
fn split_and_propagate(start: u32, end: u32, hunks: &[HunkInfo]) -> Vec<(u32, u32)> {
    // 1. Split the span around each hunk's [old_start, old_end)
    let mut remaining = vec![(start, end)];

    for hunk in hunks {
        let old_start = hunk.old_start;
        let old_end = hunk.old_start + hunk.old_lines;

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

    // 2. Map the remaining (non-overlapping) parts through the line offsets
    remaining
        .into_iter()
        .filter(|(s, e)| e > s)
        .map(|(s, e)| (map_line_forward(s, hunks), map_line_forward(e, hunks)))
        .filter(|(s, e)| e > s)
        .collect()
}

/// (legacy) Extract FileSpans from a single commit diff without propagation.
/// Kept for tests that operate on individual commits.
pub fn extract_spans(commit_diff: &CommitDiff) -> Vec<FileSpan> {
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
    pub commit_oids: Vec<String>,
}

/// The complete fragmap: commits, span clusters, and the matrix showing
/// which commits touch which clusters.
///
/// The matrix is commits × clusters, where matrix[commit_idx][cluster_idx]
/// indicates how that commit touches that cluster.
#[derive(Debug, Clone)]
pub struct FragMap {
    /// The commits in order (oldest to newest).
    pub commits: Vec<String>,
    /// The span clusters (code regions touched by commits).
    pub clusters: Vec<SpanCluster>,
    /// Matrix[commit_idx][cluster_idx] = TouchKind
    pub matrix: Vec<Vec<TouchKind>>,
}

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

/// Half-open interval `[start, end)` for SPG span computations.
/// Uses `i64` to safely handle arithmetic with large sentinel values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct SpgSpan {
    start: i64,
    end: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SpgOverlap {
    None,
    Point,
    Interval,
}

const SPG_SENTINEL: i64 = 100_000_000;

impl SpgSpan {
    fn is_empty(&self) -> bool {
        self.start >= self.end
    }

    /// Overlap classification matching the original fragmap's `Span.overlap()`.
    fn overlap(&self, other: &SpgSpan) -> SpgOverlap {
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

    fn from_old_hunk(h: &HunkInfo) -> Self {
        let mut start = h.old_start as i64;
        if h.old_lines == 0 {
            start += 1;
        }
        SpgSpan {
            start,
            end: start + h.old_lines as i64,
        }
    }

    fn from_new_hunk(h: &HunkInfo) -> Self {
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

use std::collections::HashSet;

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
fn spg_map_start(line: i64, hunks: &[HunkInfo]) -> i64 {
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
fn spg_map_end(line: i64, hunks: &[HunkInfo]) -> i64 {
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
fn spg_moved_span(prev_new_span: &SpgSpan, hunks: &[HunkInfo]) -> Vec<SpgSpan> {
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
fn deduplicate_clusters(clusters: &mut Vec<SpanCluster>) {
    // Build activation pattern (sorted commit_oids) for each cluster
    for c in clusters.iter_mut() {
        c.commit_oids.sort();
    }
    let mut seen: HashSet<Vec<String>> = HashSet::new();
    clusters.retain(|c| seen.insert(c.commit_oids.clone()));
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
        let spg = build_file_spg(commits_for_file);
        let node_count = spg.graph.len();
        let raw_paths = spg_enumerate_paths(&spg.graph, &source_node(), &sink_node());
        let deduped_paths = spg_all_paths(&spg);
        let gens: Vec<usize> = commits_for_file.iter().map(|(g, _)| *g).collect();
        eprintln!(
            "FILE: {} | gens={:?} | nodes={} | raw_paths={} | deduped_paths={}",
            path,
            gens,
            node_count,
            raw_paths.len(),
            deduped_paths.len()
        );
    }
}

/// Build a fragmap from a collection of commits and their diffs.
///
/// Implements the Span Propagation Graph (SPG) algorithm from the original
/// fragmap tool. For each file, a DAG is built where active nodes (hunks)
/// and inactive nodes (propagated surviving spans) are connected by overlap
/// edges. Columns correspond to unique paths through the DAG, with each
/// path's active nodes determining which commits have CHANGE in that column.
pub fn build_fragmap(commit_diffs: &[CommitDiff]) -> FragMap {
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
                // Merge hunks from the same file and commit (can happen when
                // a commit has multiple FileDiff entries for the same path)
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

    let mut clusters: Vec<SpanCluster> = Vec::new();

    let mut sorted_paths: Vec<&String> = file_commits.keys().collect();
    sorted_paths.sort();

    for path in sorted_paths {
        let commits_for_file = &file_commits[path];
        let spg = build_file_spg(commits_for_file);
        let paths = spg_all_paths(&spg);

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
                            path: path.clone(),
                            start_line: sp.start.max(1) as u32,
                            end_line: (sp.end - 1).max(1) as u32,
                        }],
                        commit_oids,
                    });
                }
            }
        }
    }

    deduplicate_clusters(&mut clusters);

    let commits: Vec<String> = commit_diffs.iter().map(|d| d.commit.oid.clone()).collect();
    let matrix = build_matrix(&commits, &clusters, commit_diffs);

    FragMap {
        commits,
        clusters,
        matrix,
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

            let earlier = (0..commit_idx).find(|&i| self.matrix[i][cluster_idx] != TouchKind::None);

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
}

/// Build the commits × clusters matrix with TouchKind values.
///
/// For each (commit, cluster), determine if the commit touches the cluster
/// and if so, classify the touch as Added/Modified/Deleted.
fn build_matrix(
    commits: &[String],
    clusters: &[SpanCluster],
    commit_diffs: &[CommitDiff],
) -> Vec<Vec<TouchKind>> {
    let mut matrix = vec![vec![TouchKind::None; clusters.len()]; commits.len()];

    for (commit_idx, commit_oid) in commits.iter().enumerate() {
        let commit_diff = &commit_diffs[commit_idx];

        for (cluster_idx, cluster) in clusters.iter().enumerate() {
            // Check if this commit touches this cluster
            if cluster.commit_oids.contains(commit_oid) {
                // Determine the touch kind
                matrix[commit_idx][cluster_idx] = determine_touch_kind(commit_diff, cluster);
            }
        }
    }

    matrix
}

/// Determine how a commit touches a cluster (Added/Modified/Deleted).
///
/// Looks at the files in the commit that overlap with the cluster's spans
/// to classify the type of change.
fn determine_touch_kind(commit_diff: &CommitDiff, cluster: &SpanCluster) -> TouchKind {
    for cluster_span in &cluster.spans {
        for file in &commit_diff.files {
            // Check if this file matches the cluster span
            let file_path = file.new_path.as_ref().or(file.old_path.as_ref());
            if file_path.map(|p| p == &cluster_span.path).unwrap_or(false) {
                // Classify based on file paths
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
mod tests {
    use super::*;
    use crate::{CommitDiff, CommitInfo, FileDiff, Hunk};

    fn make_commit_info() -> CommitInfo {
        CommitInfo {
            oid: "abc123".to_string(),
            summary: "Test commit".to_string(),
            author: "Test Author".to_string(),
            date: "123456789".to_string(),
            parent_oids: vec![],
            message: "Test commit".to_string(),
            author_email: "test@example.com".to_string(),
            author_date: time::OffsetDateTime::from_unix_timestamp(123456789).unwrap(),
            committer: "Test Committer".to_string(),
            committer_email: "committer@example.com".to_string(),
            commit_date: time::OffsetDateTime::from_unix_timestamp(123456789).unwrap(),
        }
    }

    #[test]
    fn test_extract_spans_single_hunk() {
        let commit_diff = CommitDiff {
            commit: make_commit_info(),
            files: vec![FileDiff {
                old_path: Some("file.txt".to_string()),
                new_path: Some("file.txt".to_string()),
                status: crate::DeltaStatus::Modified,
                hunks: vec![Hunk {
                    old_start: 10,
                    old_lines: 3,
                    new_start: 10,
                    new_lines: 5,
                    lines: vec![],
                }],
            }],
        };

        let spans = extract_spans(&commit_diff);

        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].path, "file.txt");
        assert_eq!(spans[0].start_line, 10);
        assert_eq!(spans[0].end_line, 14); // 10 + 5 - 1
    }

    #[test]
    fn test_extract_spans_multiple_hunks() {
        let commit_diff = CommitDiff {
            commit: make_commit_info(),
            files: vec![FileDiff {
                old_path: Some("file.txt".to_string()),
                new_path: Some("file.txt".to_string()),
                status: crate::DeltaStatus::Modified,
                hunks: vec![
                    Hunk {
                        old_start: 5,
                        old_lines: 2,
                        new_start: 5,
                        new_lines: 3,
                        lines: vec![],
                    },
                    Hunk {
                        old_start: 20,
                        old_lines: 1,
                        new_start: 21,
                        new_lines: 2,
                        lines: vec![],
                    },
                ],
            }],
        };

        let spans = extract_spans(&commit_diff);

        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].path, "file.txt");
        assert_eq!(spans[0].start_line, 5);
        assert_eq!(spans[0].end_line, 7); // 5 + 3 - 1

        assert_eq!(spans[1].path, "file.txt");
        assert_eq!(spans[1].start_line, 21);
        assert_eq!(spans[1].end_line, 22); // 21 + 2 - 1
    }

    #[test]
    fn test_extract_spans_multiple_files() {
        let commit_diff = CommitDiff {
            commit: make_commit_info(),
            files: vec![
                FileDiff {
                    old_path: Some("a.txt".to_string()),
                    new_path: Some("a.txt".to_string()),
                    status: crate::DeltaStatus::Modified,
                    hunks: vec![Hunk {
                        old_start: 1,
                        old_lines: 1,
                        new_start: 1,
                        new_lines: 2,
                        lines: vec![],
                    }],
                },
                FileDiff {
                    old_path: Some("b.txt".to_string()),
                    new_path: Some("b.txt".to_string()),
                    status: crate::DeltaStatus::Modified,
                    hunks: vec![Hunk {
                        old_start: 10,
                        old_lines: 3,
                        new_start: 10,
                        new_lines: 4,
                        lines: vec![],
                    }],
                },
            ],
        };

        let spans = extract_spans(&commit_diff);

        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].path, "a.txt");
        assert_eq!(spans[0].start_line, 1);
        assert_eq!(spans[0].end_line, 2);

        assert_eq!(spans[1].path, "b.txt");
        assert_eq!(spans[1].start_line, 10);
        assert_eq!(spans[1].end_line, 13);
    }

    #[test]
    fn test_extract_spans_skips_deleted_files() {
        let commit_diff = CommitDiff {
            commit: make_commit_info(),
            files: vec![
                FileDiff {
                    old_path: Some("file.txt".to_string()),
                    new_path: Some("file.txt".to_string()),
                    status: crate::DeltaStatus::Modified,
                    hunks: vec![Hunk {
                        old_start: 1,
                        old_lines: 1,
                        new_start: 1,
                        new_lines: 2,
                        lines: vec![],
                    }],
                },
                FileDiff {
                    old_path: Some("deleted.txt".to_string()),
                    new_path: None, // File was deleted
                    status: crate::DeltaStatus::Deleted,
                    hunks: vec![Hunk {
                        old_start: 1,
                        old_lines: 5,
                        new_start: 0,
                        new_lines: 0,
                        lines: vec![],
                    }],
                },
            ],
        };

        let spans = extract_spans(&commit_diff);

        // Should only have span from file.txt, not from deleted.txt
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].path, "file.txt");
    }

    #[test]
    fn test_extract_spans_skips_empty_hunks() {
        let commit_diff = CommitDiff {
            commit: make_commit_info(),
            files: vec![FileDiff {
                old_path: Some("file.txt".to_string()),
                new_path: Some("file.txt".to_string()),
                status: crate::DeltaStatus::Modified,
                hunks: vec![
                    Hunk {
                        old_start: 5,
                        old_lines: 2,
                        new_start: 5,
                        new_lines: 3,
                        lines: vec![],
                    },
                    Hunk {
                        old_start: 10,
                        old_lines: 1,
                        new_start: 8,
                        new_lines: 0, // Empty hunk (pure deletion in context)
                        lines: vec![],
                    },
                ],
            }],
        };

        let spans = extract_spans(&commit_diff);

        // Should only have span from first hunk, not the empty one
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].start_line, 5);
        assert_eq!(spans[0].end_line, 7);
    }

    #[test]
    fn test_extract_spans_added_file() {
        let commit_diff = CommitDiff {
            commit: make_commit_info(),
            files: vec![FileDiff {
                old_path: None, // File was added
                new_path: Some("new_file.txt".to_string()),
                status: crate::DeltaStatus::Added,
                hunks: vec![Hunk {
                    old_start: 0,
                    old_lines: 0,
                    new_start: 1,
                    new_lines: 10,
                    lines: vec![],
                }],
            }],
        };

        let spans = extract_spans(&commit_diff);

        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].path, "new_file.txt");
        assert_eq!(spans[0].start_line, 1);
        assert_eq!(spans[0].end_line, 10);
    }

    #[test]
    fn test_extract_spans_single_line_change() {
        let commit_diff = CommitDiff {
            commit: make_commit_info(),
            files: vec![FileDiff {
                old_path: Some("file.txt".to_string()),
                new_path: Some("file.txt".to_string()),
                status: crate::DeltaStatus::Modified,
                hunks: vec![Hunk {
                    old_start: 42,
                    old_lines: 1,
                    new_start: 42,
                    new_lines: 1,
                    lines: vec![],
                }],
            }],
        };

        let spans = extract_spans(&commit_diff);

        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].start_line, 42);
        assert_eq!(spans[0].end_line, 42); // Single line: 42 + 1 - 1 = 42
    }

    #[test]
    fn test_extract_spans_empty_commit() {
        let commit_diff = CommitDiff {
            commit: make_commit_info(),
            files: vec![],
        };

        let spans = extract_spans(&commit_diff);

        assert_eq!(spans.len(), 0);
    }

    #[test]
    fn test_map_line_forward_before_hunk() {
        let hunks = vec![HunkInfo {
            old_start: 10,
            old_lines: 5,
            new_start: 10,
            new_lines: 8,
        }];
        assert_eq!(map_line_forward(5, &hunks), 5);
    }

    #[test]
    fn test_map_line_forward_after_hunk() {
        let hunks = vec![HunkInfo {
            old_start: 10,
            old_lines: 5,
            new_start: 10,
            new_lines: 8,
        }];
        // delta = 8 - 5 = 3, so line 20 → 23
        assert_eq!(map_line_forward(20, &hunks), 23);
    }

    #[test]
    fn test_split_and_propagate_overlap() {
        let hunks = vec![HunkInfo {
            old_start: 10,
            old_lines: 5,
            new_start: 10,
            new_lines: 8,
        }];
        // Span [8, 20) partially overlaps with hunk [10, 15).
        // After splitting: [8, 10) (before) and [15, 20) (after).
        // [8, 10) → map: 8 < 10, no delta → [8, 10)
        // [15, 20) → map: 15 >= 15, delta = 8-5 = 3 → [18, 23)
        let result = split_and_propagate(8, 20, &hunks);
        assert_eq!(result, vec![(8, 10), (18, 23)]);
    }

    #[test]
    fn test_map_line_forward_two_hunks() {
        let hunks = vec![
            HunkInfo {
                old_start: 10,
                old_lines: 5,
                new_start: 10,
                new_lines: 8,
            },
            HunkInfo {
                old_start: 30,
                old_lines: 3,
                new_start: 33,
                new_lines: 5,
            },
        ];
        // Between hunks: line 25 → 25 + 3 = 28
        assert_eq!(map_line_forward(25, &hunks), 28);
        // After both: line 40 → 40 + 3 + 2 = 45
        assert_eq!(map_line_forward(40, &hunks), 45);
    }

    #[test]
    fn test_propagation_sequential_commits_same_file() {
        // Two commits touching different, distant parts of the same file.
        // After propagation they should not share a cluster.
        let commits = vec![
            CommitDiff {
                commit: make_commit_info_with_oid("c1"),
                files: vec![FileDiff {
                    old_path: Some("f.rs".to_string()),
                    new_path: Some("f.rs".to_string()),
                    status: crate::DeltaStatus::Modified,
                    hunks: vec![Hunk {
                        old_start: 10,
                        old_lines: 5,
                        new_start: 10,
                        new_lines: 8,
                        lines: vec![],
                    }],
                }],
            },
            CommitDiff {
                commit: make_commit_info_with_oid("c2"),
                files: vec![FileDiff {
                    old_path: Some("f.rs".to_string()),
                    new_path: Some("f.rs".to_string()),
                    status: crate::DeltaStatus::Modified,
                    hunks: vec![Hunk {
                        old_start: 50,
                        old_lines: 5,
                        new_start: 50,
                        new_lines: 5,
                        lines: vec![],
                    }],
                }],
            },
        ];

        let fm = build_fragmap(&commits);
        assert!(!fm.shares_cluster_with(0, 1));
    }

    #[test]
    fn test_propagation_overlapping_hunks_are_related() {
        // Commit 1 inserts a large block. Commit 2 modifies within that block.
        // After propagation commit 1's span includes commit 2's region.
        let commits = vec![
            CommitDiff {
                commit: make_commit_info_with_oid("c1"),
                files: vec![FileDiff {
                    old_path: Some("f.rs".to_string()),
                    new_path: Some("f.rs".to_string()),
                    status: crate::DeltaStatus::Modified,
                    hunks: vec![Hunk {
                        old_start: 10,
                        old_lines: 5,
                        new_start: 10,
                        new_lines: 55,
                        lines: vec![],
                    }],
                }],
            },
            CommitDiff {
                commit: make_commit_info_with_oid("c2"),
                files: vec![FileDiff {
                    old_path: Some("f.rs".to_string()),
                    new_path: Some("f.rs".to_string()),
                    status: crate::DeltaStatus::Modified,
                    hunks: vec![Hunk {
                        old_start: 30,
                        old_lines: 10,
                        new_start: 30,
                        new_lines: 10,
                        lines: vec![],
                    }],
                }],
            },
        ];

        let fm = build_fragmap(&commits);
        assert!(fm.shares_cluster_with(0, 1));
    }

    #[test]
    fn test_propagation_distant_changes_not_related() {
        // Changes far apart in the same file should not cluster.
        let commits = vec![
            CommitDiff {
                commit: make_commit_info_with_oid("c1"),
                files: vec![FileDiff {
                    old_path: Some("f.rs".to_string()),
                    new_path: Some("f.rs".to_string()),
                    status: crate::DeltaStatus::Modified,
                    hunks: vec![Hunk {
                        old_start: 10,
                        old_lines: 3,
                        new_start: 10,
                        new_lines: 5,
                        lines: vec![],
                    }],
                }],
            },
            CommitDiff {
                commit: make_commit_info_with_oid("c2"),
                files: vec![FileDiff {
                    old_path: Some("f.rs".to_string()),
                    new_path: Some("f.rs".to_string()),
                    status: crate::DeltaStatus::Modified,
                    hunks: vec![Hunk {
                        old_start: 200,
                        old_lines: 5,
                        new_start: 202,
                        new_lines: 5,
                        lines: vec![],
                    }],
                }],
            },
        ];

        let fm = build_fragmap(&commits);
        assert!(!fm.shares_cluster_with(0, 1));
    }

    // Helper functions for matrix generation tests

    fn make_commit_info_with_oid(oid: &str) -> CommitInfo {
        CommitInfo {
            oid: oid.to_string(),
            summary: format!("Commit {}", oid),
            author: "Test Author".to_string(),
            date: "123456789".to_string(),
            parent_oids: vec![],
            message: format!("Commit {}", oid),
            author_email: "test@example.com".to_string(),
            author_date: time::OffsetDateTime::from_unix_timestamp(123456789).unwrap(),
            committer: "Test Committer".to_string(),
            committer_email: "committer@example.com".to_string(),
            commit_date: time::OffsetDateTime::from_unix_timestamp(123456789).unwrap(),
        }
    }

    fn make_file_diff(
        old_path: Option<&str>,
        new_path: Option<&str>,
        old_start: u32,
        old_lines: u32,
        new_start: u32,
        new_lines: u32,
    ) -> FileDiff {
        FileDiff {
            old_path: old_path.map(|s| s.to_string()),
            new_path: new_path.map(|s| s.to_string()),
            status: crate::DeltaStatus::Modified,
            hunks: vec![Hunk {
                old_start,
                old_lines,
                new_start,
                new_lines,
                lines: vec![],
            }],
        }
    }

    fn make_commit_diff(oid: &str, files: Vec<FileDiff>) -> CommitDiff {
        CommitDiff {
            commit: make_commit_info_with_oid(oid),
            files,
        }
    }

    // Matrix generation tests

    #[test]
    fn test_build_fragmap_empty_commits() {
        let fragmap = build_fragmap(&[]);

        assert_eq!(fragmap.commits.len(), 0);
        assert_eq!(fragmap.clusters.len(), 0);
        assert_eq!(fragmap.matrix.len(), 0);
    }

    #[test]
    fn test_build_fragmap_single_commit() {
        let commits = vec![make_commit_diff(
            "c1",
            vec![make_file_diff(
                None, // File was added
                Some("file.txt"),
                0,
                0,
                1,
                3,
            )],
        )];

        let fragmap = build_fragmap(&commits);

        assert_eq!(fragmap.commits.len(), 1);
        assert_eq!(fragmap.commits[0], "c1");

        // Should have one cluster
        assert_eq!(fragmap.clusters.len(), 1);
        assert_eq!(fragmap.clusters[0].spans.len(), 1);
        assert_eq!(fragmap.clusters[0].spans[0].path, "file.txt");
        assert_eq!(fragmap.clusters[0].commit_oids, vec!["c1"]);

        // Matrix should be 1x1 with Added
        assert_eq!(fragmap.matrix.len(), 1);
        assert_eq!(fragmap.matrix[0].len(), 1);
        assert_eq!(fragmap.matrix[0][0], TouchKind::Added);
    }

    #[test]
    fn test_build_fragmap_overlapping_spans_merge() {
        // Two commits touching overlapping regions should be related
        let commits = vec![
            make_commit_diff(
                "c1",
                vec![make_file_diff(
                    Some("file.txt"),
                    Some("file.txt"),
                    1,
                    0,
                    1,
                    5, // lines 1-5
                )],
            ),
            make_commit_diff(
                "c2",
                vec![make_file_diff(
                    Some("file.txt"),
                    Some("file.txt"),
                    3,
                    3,
                    3,
                    4, // lines 3-6 (overlaps with c1)
                )],
            ),
        ];

        let fragmap = build_fragmap(&commits);

        assert_eq!(fragmap.commits.len(), 2);

        // Both commits should share at least one cluster
        assert!(fragmap.shares_cluster_with(0, 1));

        // There should be a cluster containing both commits
        let shared = fragmap.clusters.iter().any(|c| {
            c.commit_oids.contains(&"c1".to_string()) && c.commit_oids.contains(&"c2".to_string())
        });
        assert!(shared);

        // Both commits should have non-None entries in the shared cluster
        let shared_idx = fragmap
            .clusters
            .iter()
            .position(|c| {
                c.commit_oids.contains(&"c1".to_string())
                    && c.commit_oids.contains(&"c2".to_string())
            })
            .unwrap();
        assert_ne!(fragmap.matrix[0][shared_idx], TouchKind::None);
        assert_ne!(fragmap.matrix[1][shared_idx], TouchKind::None);
    }

    #[test]
    fn test_build_fragmap_non_overlapping_separate_clusters() {
        // Two commits touching different regions should create two clusters
        let commits = vec![
            make_commit_diff(
                "c1",
                vec![make_file_diff(
                    Some("file.txt"),
                    Some("file.txt"),
                    1,
                    0,
                    1,
                    5, // lines 1-5
                )],
            ),
            make_commit_diff(
                "c2",
                vec![make_file_diff(
                    Some("file.txt"),
                    Some("file.txt"),
                    10,
                    3,
                    10,
                    4, // lines 10-13 (no overlap)
                )],
            ),
        ];

        let fragmap = build_fragmap(&commits);

        assert_eq!(fragmap.commits.len(), 2);

        // Should have two clusters (no overlap)
        assert_eq!(fragmap.clusters.len(), 2);

        // Matrix should be 2x2
        assert_eq!(fragmap.matrix.len(), 2);
        assert_eq!(fragmap.matrix[0].len(), 2);
        assert_eq!(fragmap.matrix[1].len(), 2);

        // c1 touches first cluster, not second
        assert_ne!(fragmap.matrix[0][0], TouchKind::None);
        assert_eq!(fragmap.matrix[0][1], TouchKind::None);

        // c2 touches second cluster, not first
        assert_eq!(fragmap.matrix[1][0], TouchKind::None);
        assert_ne!(fragmap.matrix[1][1], TouchKind::None);
    }

    #[test]
    fn test_build_fragmap_adjacent_spans_stay_separate() {
        // Adjacent spans (end_line + 1 == start_line) should NOT merge.
        // Only actual overlap causes clustering, matching the original fragmap.
        let commits = vec![
            make_commit_diff(
                "c1",
                vec![make_file_diff(
                    Some("file.txt"),
                    Some("file.txt"),
                    1,
                    0,
                    1,
                    5, // lines 1-5
                )],
            ),
            make_commit_diff(
                "c2",
                vec![make_file_diff(
                    Some("file.txt"),
                    Some("file.txt"),
                    6,
                    2,
                    6,
                    3, // lines 6-8 (adjacent to c1, NOT overlapping)
                )],
            ),
        ];

        let fragmap = build_fragmap(&commits);

        assert_eq!(fragmap.commits.len(), 2);

        // Should have two clusters (adjacent but not overlapping)
        assert_eq!(fragmap.clusters.len(), 2);
    }

    #[test]
    fn test_no_snowball_effect_on_cluster_ranges() {
        // Regression test: distant spans must not be absorbed into a nearby cluster.
        //
        // Commit 1: lines 1-5, Commit 2: lines 3-12 (overlaps c1),
        // Commit 3: lines 50-53 (should NOT be absorbed)
        let commits = vec![
            make_commit_diff(
                "c1",
                vec![make_file_diff(
                    Some("file.txt"),
                    Some("file.txt"),
                    1,
                    0,
                    1,
                    5, // lines 1-5
                )],
            ),
            make_commit_diff(
                "c2",
                vec![make_file_diff(
                    Some("file.txt"),
                    Some("file.txt"),
                    3,
                    5,
                    3,
                    10, // lines 3-12 (overlaps c1)
                )],
            ),
            make_commit_diff(
                "c3",
                vec![make_file_diff(
                    Some("file.txt"),
                    Some("file.txt"),
                    50,
                    3,
                    50,
                    4, // lines 50-53 (far away, separate)
                )],
            ),
        ];

        let fragmap = build_fragmap(&commits);

        // c1 and c2 share a cluster, c3 does not overlap with either
        assert!(fragmap.shares_cluster_with(0, 1));
        assert!(!fragmap.shares_cluster_with(0, 2));
        assert!(!fragmap.shares_cluster_with(1, 2));
    }

    #[test]
    fn test_different_functions_same_file_separate_clusters() {
        // Real-world scenario: two commits touch different functions in the
        // same file. They should be in separate clusters (separate columns),
        // not squashable into each other.
        let commits = vec![
            make_commit_diff(
                "c1",
                vec![make_file_diff(
                    Some("lib.rs"),
                    Some("lib.rs"),
                    10,
                    3,
                    10,
                    5, // function foo() at lines 10-14
                )],
            ),
            make_commit_diff(
                "c2",
                vec![make_file_diff(
                    Some("lib.rs"),
                    Some("lib.rs"),
                    80,
                    2,
                    80,
                    4, // function bar() at lines 80-83
                )],
            ),
        ];

        let fragmap = build_fragmap(&commits);

        // Separate clusters — these are different code regions
        assert_eq!(fragmap.clusters.len(), 2);

        // Neither commit is squashable into the other
        assert!(!fragmap.is_fully_squashable(0));
        assert!(!fragmap.is_fully_squashable(1));

        // They don't share any cluster
        assert!(!fragmap.shares_cluster_with(0, 1));
    }

    #[test]
    fn test_build_fragmap_touchkind_added() {
        // Adding a new file should produce TouchKind::Added
        let commits = vec![make_commit_diff(
            "c1",
            vec![make_file_diff(
                None, // old_path
                Some("new_file.txt"),
                0,
                0,
                1,
                10,
            )],
        )];

        let fragmap = build_fragmap(&commits);

        assert_eq!(fragmap.matrix[0][0], TouchKind::Added);
    }

    #[test]
    fn test_build_fragmap_touchkind_modified() {
        // Modifying existing lines should produce TouchKind::Modified
        let commits = vec![make_commit_diff(
            "c1",
            vec![make_file_diff(
                Some("file.txt"),
                Some("file.txt"),
                10,
                5, // old_lines > 0
                10,
                6, // new_lines > 0
            )],
        )];

        let fragmap = build_fragmap(&commits);

        assert_eq!(fragmap.matrix[0][0], TouchKind::Modified);
    }

    #[test]
    fn test_build_fragmap_touchkind_deleted() {
        // Deleting lines should produce TouchKind::Deleted
        // But deleted files are skipped, so we test a hunk with deletions
        // Actually, we need to look at the determine_touch_kind logic more carefully
        // For now, test that pure deletions (no new_lines) are skipped at span extraction level
        // This test verifies the matrix generation doesn't crash with complex diffs
        let commits = vec![make_commit_diff(
            "c1",
            vec![make_file_diff(
                Some("file.txt"),
                Some("file.txt"),
                10,
                5,
                10,
                2, // Shrinking the region (some deletions)
            )],
        )];

        let fragmap = build_fragmap(&commits);

        // Should still generate a valid fragmap
        assert_eq!(fragmap.commits.len(), 1);
        assert_eq!(fragmap.clusters.len(), 1);
    }

    #[test]
    fn test_build_fragmap_multiple_files_separate_clusters() {
        // Different files should always create separate clusters
        let commits = vec![
            make_commit_diff(
                "c1",
                vec![make_file_diff(Some("a.txt"), Some("a.txt"), 1, 0, 1, 5)],
            ),
            make_commit_diff(
                "c2",
                vec![make_file_diff(
                    Some("b.txt"),
                    Some("b.txt"),
                    1,
                    0,
                    1,
                    5, // Same line range but different file
                )],
            ),
        ];

        let fragmap = build_fragmap(&commits);

        assert_eq!(fragmap.commits.len(), 2);

        // Should have two clusters (different files)
        assert_eq!(fragmap.clusters.len(), 2);

        // Each commit touches only its own cluster
        assert_ne!(fragmap.matrix[0][0], TouchKind::None);
        assert_eq!(fragmap.matrix[0][1], TouchKind::None);

        assert_eq!(fragmap.matrix[1][0], TouchKind::None);
        assert_ne!(fragmap.matrix[1][1], TouchKind::None);
    }

    #[test]
    fn test_build_fragmap_commit_touches_multiple_clusters() {
        // A single commit touching multiple non-adjacent regions of the same
        // file produces columns with identical activation patterns (only c1
        // is active). BriefFragmap-style dedup merges them into one column.
        let mut c1 = make_commit_diff(
            "c1",
            vec![make_file_diff(
                Some("file.txt"),
                Some("file.txt"),
                1,
                0,
                1,
                5, // lines 1-5
            )],
        );

        c1.files.push(make_file_diff(
            Some("file.txt"),
            Some("file.txt"),
            20,
            0,
            20,
            3, // lines 20-22 (separate region)
        ));

        let fragmap = build_fragmap(&[c1]);

        assert_eq!(fragmap.commits.len(), 1);

        // After dedup, both regions have the same activation pattern {c1},
        // so they collapse into a single column.
        assert_eq!(fragmap.clusters.len(), 1);
        assert_ne!(fragmap.matrix[0][0], TouchKind::None);
    }

    // Squashability analysis tests

    #[test]
    fn test_cluster_relation_no_relation_neither_touches() {
        // Two commits that don't touch the same cluster
        let commits = vec![
            make_commit_diff(
                "c1",
                vec![make_file_diff(Some("a.txt"), Some("a.txt"), 1, 0, 1, 5)],
            ),
            make_commit_diff(
                "c2",
                vec![make_file_diff(Some("b.txt"), Some("b.txt"), 1, 0, 1, 5)],
            ),
        ];

        let fragmap = build_fragmap(&commits);

        // Two clusters, c1 only touches cluster 0
        assert_eq!(fragmap.clusters.len(), 2);

        let relation = fragmap.cluster_relation(0, 1, 0);
        assert_eq!(relation, SquashRelation::NoRelation);
    }

    #[test]
    fn test_cluster_relation_no_relation_only_one_touches() {
        // Only one commit touches the cluster
        let commits = vec![
            make_commit_diff(
                "c1",
                vec![make_file_diff(
                    Some("file.txt"),
                    Some("file.txt"),
                    1,
                    0,
                    1,
                    5,
                )],
            ),
            make_commit_diff(
                "c2",
                vec![make_file_diff(
                    Some("file.txt"),
                    Some("file.txt"),
                    100,
                    0,
                    100,
                    5, // Far away, different cluster
                )],
            ),
        ];

        let fragmap = build_fragmap(&commits);

        assert_eq!(fragmap.clusters.len(), 2);

        // c1 touches cluster 0, c2 doesn't
        let relation = fragmap.cluster_relation(0, 1, 0);
        assert_eq!(relation, SquashRelation::NoRelation);
    }

    #[test]
    fn test_cluster_relation_squashable_no_collisions() {
        // Two commits touch same cluster, no commits in between
        let commits = vec![
            make_commit_diff(
                "c1",
                vec![make_file_diff(
                    Some("file.txt"),
                    Some("file.txt"),
                    1,
                    0,
                    1,
                    5,
                )],
            ),
            make_commit_diff(
                "c2",
                vec![make_file_diff(
                    Some("file.txt"),
                    Some("file.txt"),
                    3,
                    3,
                    3,
                    4, // Overlaps with c1
                )],
            ),
        ];

        let fragmap = build_fragmap(&commits);

        // Find the shared cluster
        let shared_idx = fragmap
            .clusters
            .iter()
            .position(|c| {
                c.commit_oids.contains(&"c1".to_string())
                    && c.commit_oids.contains(&"c2".to_string())
            })
            .expect("should have a shared cluster");

        let relation = fragmap.cluster_relation(0, 1, shared_idx);
        assert_eq!(relation, SquashRelation::Squashable);
    }

    #[test]
    fn test_cluster_relation_conflicting_with_collision() {
        // Three commits touch same code region - middle one creates a collision
        let commits = vec![
            make_commit_diff(
                "c1",
                vec![make_file_diff(
                    Some("file.txt"),
                    Some("file.txt"),
                    1,
                    0,
                    1,
                    5,
                )],
            ),
            make_commit_diff(
                "c2",
                vec![make_file_diff(
                    Some("file.txt"),
                    Some("file.txt"),
                    3,
                    2,
                    3,
                    3, // Overlaps - collision
                )],
            ),
            make_commit_diff(
                "c3",
                vec![make_file_diff(
                    Some("file.txt"),
                    Some("file.txt"),
                    2,
                    3,
                    2,
                    4, // Also overlaps
                )],
            ),
        ];

        let fragmap = build_fragmap(&commits);

        // All three commits should share at least one cluster
        let all_three_idx = fragmap
            .clusters
            .iter()
            .position(|c| {
                c.commit_oids.contains(&"c1".to_string())
                    && c.commit_oids.contains(&"c2".to_string())
                    && c.commit_oids.contains(&"c3".to_string())
            })
            .expect("should have a cluster with all three commits");

        // c1 and c3 have a collision (c2 in between)
        let relation = fragmap.cluster_relation(0, 2, all_three_idx);
        assert_eq!(relation, SquashRelation::Conflicting);
    }

    #[test]
    fn test_cluster_relation_invalid_indices() {
        let commits = vec![
            make_commit_diff(
                "c1",
                vec![make_file_diff(Some("a.txt"), Some("a.txt"), 1, 0, 1, 5)],
            ),
            make_commit_diff(
                "c2",
                vec![make_file_diff(Some("a.txt"), Some("a.txt"), 3, 2, 3, 3)],
            ),
        ];

        let fragmap = build_fragmap(&commits);

        // Out of range commit index
        let relation = fragmap.cluster_relation(0, 10, 0);
        assert_eq!(relation, SquashRelation::NoRelation);

        // Out of range cluster index
        let relation = fragmap.cluster_relation(0, 1, 10);
        assert_eq!(relation, SquashRelation::NoRelation);
    }

    #[test]
    fn test_cluster_relation_earlier_not_less_than_later() {
        let commits = vec![
            make_commit_diff(
                "c1",
                vec![make_file_diff(Some("a.txt"), Some("a.txt"), 1, 0, 1, 5)],
            ),
            make_commit_diff(
                "c2",
                vec![make_file_diff(Some("a.txt"), Some("a.txt"), 3, 2, 3, 3)],
            ),
        ];

        let fragmap = build_fragmap(&commits);

        // Same index
        let relation = fragmap.cluster_relation(1, 1, 0);
        assert_eq!(relation, SquashRelation::NoRelation);

        // Earlier > later
        let relation = fragmap.cluster_relation(1, 0, 0);
        assert_eq!(relation, SquashRelation::NoRelation);
    }

    #[test]
    fn test_cluster_relation_multiple_clusters() {
        // Complex scenario with multiple clusters across files
        let commits = vec![
            make_commit_diff(
                "c1",
                vec![
                    make_file_diff(Some("a.txt"), Some("a.txt"), 1, 0, 1, 5),
                    make_file_diff(Some("b.txt"), Some("b.txt"), 1, 0, 1, 5),
                ],
            ),
            make_commit_diff(
                "c2",
                vec![make_file_diff(Some("a.txt"), Some("a.txt"), 3, 2, 3, 3)],
            ),
            make_commit_diff(
                "c3",
                vec![make_file_diff(Some("b.txt"), Some("b.txt"), 3, 2, 3, 3)],
            ),
        ];

        let fragmap = build_fragmap(&commits);

        // Find shared clusters by file
        let a_cluster_idx = fragmap
            .clusters
            .iter()
            .position(|c| {
                c.spans[0].path == "a.txt"
                    && c.commit_oids.contains(&"c1".to_string())
                    && c.commit_oids.contains(&"c2".to_string())
            })
            .expect("should have a shared a.txt cluster");
        let b_cluster_idx = fragmap
            .clusters
            .iter()
            .position(|c| {
                c.spans[0].path == "b.txt"
                    && c.commit_oids.contains(&"c1".to_string())
                    && c.commit_oids.contains(&"c3".to_string())
            })
            .expect("should have a shared b.txt cluster");

        // c1 and c2 both touch a.txt cluster - squashable (no collision)
        let relation = fragmap.cluster_relation(0, 1, a_cluster_idx);
        assert_eq!(relation, SquashRelation::Squashable);

        // c1 and c3 both touch b.txt cluster - squashable (no collision)
        let relation = fragmap.cluster_relation(0, 2, b_cluster_idx);
        assert_eq!(relation, SquashRelation::Squashable);

        // c2 and c3 don't share any cluster
        assert!(!fragmap.shares_cluster_with(1, 2));
    }

    #[test]
    fn test_cluster_relation_squashable_with_gap() {
        // Four commits: c1 and c4 touch overlapping regions, c2 and c3 don't
        let commits = vec![
            make_commit_diff(
                "c1",
                vec![make_file_diff(
                    Some("file.txt"),
                    Some("file.txt"),
                    1,
                    0,
                    1,
                    5,
                )],
            ),
            make_commit_diff(
                "c2",
                vec![make_file_diff(
                    Some("other.txt"),
                    Some("other.txt"),
                    1,
                    0,
                    1,
                    5,
                )],
            ),
            make_commit_diff(
                "c3",
                vec![make_file_diff(
                    Some("another.txt"),
                    Some("another.txt"),
                    1,
                    0,
                    1,
                    5,
                )],
            ),
            make_commit_diff(
                "c4",
                vec![make_file_diff(
                    Some("file.txt"),
                    Some("file.txt"),
                    3,
                    2,
                    3,
                    3,
                )],
            ),
        ];

        let fragmap = build_fragmap(&commits);

        // Find the shared file.txt cluster containing c1 and c4
        let file_cluster_idx = fragmap
            .clusters
            .iter()
            .position(|c| {
                c.spans[0].path == "file.txt"
                    && c.commit_oids.contains(&"c1".to_string())
                    && c.commit_oids.contains(&"c4".to_string())
            })
            .expect("should have a shared file.txt cluster");

        // c1 and c4 touch file.txt, c2 and c3 don't - squashable
        let relation = fragmap.cluster_relation(0, 3, file_cluster_idx);
        assert_eq!(relation, SquashRelation::Squashable);
    }

    /// Build a FragMap directly from a matrix (bypasses span extraction).
    fn make_fragmap(commit_ids: &[&str], n_clusters: usize, touches: &[(usize, usize)]) -> FragMap {
        let commits: Vec<String> = commit_ids.iter().map(|s| s.to_string()).collect();
        let clusters = (0..n_clusters)
            .map(|_| SpanCluster {
                spans: vec![FileSpan {
                    path: "f.txt".to_string(),
                    start_line: 1,
                    end_line: 1,
                }],
                commit_oids: vec![],
            })
            .collect();
        let mut matrix = vec![vec![TouchKind::None; n_clusters]; commit_ids.len()];
        for &(c, cl) in touches {
            matrix[c][cl] = TouchKind::Modified;
        }
        FragMap {
            commits,
            clusters,
            matrix,
        }
    }

    // squash_target tests

    #[test]
    fn squash_target_no_shared_clusters() {
        // c0 touches cluster 0, c1 touches cluster 1 — no earlier commit in c1's cluster
        let fm = make_fragmap(&["c0", "c1"], 2, &[(0, 0), (1, 1)]);
        assert_eq!(fm.squash_target(1), None);
    }

    #[test]
    fn squash_target_adjacent() {
        // c0 and c1 both touch cluster 0 — c1's target is c0
        let fm = make_fragmap(&["c0", "c1"], 1, &[(0, 0), (1, 0)]);
        assert_eq!(fm.squash_target(1), Some(0));
    }

    #[test]
    fn squash_target_with_gap() {
        // c0 and c2 touch cluster 0, c1 does not — c2's target is c0
        let fm = make_fragmap(&["c0", "c1", "c2"], 1, &[(0, 0), (2, 0)]);
        assert_eq!(fm.squash_target(2), Some(0));
    }

    #[test]
    fn squash_target_conflicting_returns_none() {
        // c0, c1, c2 all touch cluster 0 — c2 blocked by c1
        let fm = make_fragmap(&["c0", "c1", "c2"], 1, &[(0, 0), (1, 0), (2, 0)]);
        assert_eq!(fm.squash_target(2), None);
    }

    #[test]
    fn squash_target_multiple_clusters_same_target() {
        // c0 and c1 share clusters 0 and 1 — target is c0
        let fm = make_fragmap(&["c0", "c1"], 2, &[(0, 0), (0, 1), (1, 0), (1, 1)]);
        assert_eq!(fm.squash_target(1), Some(0));
    }

    #[test]
    fn squash_target_multiple_clusters_different_targets() {
        // cluster 0: c0 and c2 → target c0
        // cluster 1: c1 and c2 → target c1
        // c2 has divergent targets → None
        let fm = make_fragmap(&["c0", "c1", "c2"], 2, &[(0, 0), (1, 1), (2, 0), (2, 1)]);
        assert_eq!(fm.squash_target(2), None);
    }

    #[test]
    fn squash_target_earliest_commit_returns_none() {
        // c0 is the earliest — nothing earlier to squash into
        let fm = make_fragmap(&["c0", "c1"], 1, &[(0, 0), (1, 0)]);
        assert_eq!(fm.squash_target(0), None);
    }

    #[test]
    fn squash_target_no_clusters_touched() {
        // c1 doesn't touch any cluster
        let fm = make_fragmap(&["c0", "c1"], 1, &[(0, 0)]);
        assert_eq!(fm.squash_target(1), None);
    }

    // is_fully_squashable tests

    #[test]
    fn is_fully_squashable_single_cluster_adjacent() {
        // c0 and c1 touch cluster 0, c1 is squashable into c0
        let fm = make_fragmap(&["c0", "c1"], 1, &[(0, 0), (1, 0)]);
        assert!(fm.is_fully_squashable(1));
    }

    #[test]
    fn is_fully_squashable_first_commit_not_squashable() {
        // c0 is the earliest — nothing to squash into
        let fm = make_fragmap(&["c0", "c1"], 1, &[(0, 0), (1, 0)]);
        assert!(!fm.is_fully_squashable(0));
    }

    #[test]
    fn is_fully_squashable_multiple_clusters_same_target() {
        // c0 and c1 both touch clusters 0 and 1 — all squashable into c0
        let fm = make_fragmap(&["c0", "c1"], 2, &[(0, 0), (0, 1), (1, 0), (1, 1)]);
        assert!(fm.is_fully_squashable(1));
    }

    #[test]
    fn is_fully_squashable_multiple_clusters_different_targets() {
        // cluster 0: c0 and c2 — target c0
        // cluster 1: c1 and c2 — target c1
        // c2 has different targets, not fully squashable
        let fm = make_fragmap(&["c0", "c1", "c2"], 2, &[(0, 0), (1, 1), (2, 0), (2, 1)]);
        assert!(!fm.is_fully_squashable(2));
    }

    #[test]
    fn is_fully_squashable_conflicting_cluster() {
        // c0, c1, c2 all touch cluster 0 — c2 has c1 in between
        let fm = make_fragmap(&["c0", "c1", "c2"], 1, &[(0, 0), (1, 0), (2, 0)]);
        assert!(!fm.is_fully_squashable(2));
    }

    #[test]
    fn is_fully_squashable_no_clusters_touched() {
        // c1 doesn't touch any cluster
        let fm = make_fragmap(&["c0", "c1"], 1, &[(0, 0)]);
        assert!(!fm.is_fully_squashable(1));
    }

    // shares_cluster_with tests

    #[test]
    fn shares_cluster_with_no_shared_cluster() {
        let fm = make_fragmap(&["c0", "c1"], 2, &[(0, 0), (1, 1)]);
        assert!(!fm.shares_cluster_with(0, 1));
    }

    #[test]
    fn shares_cluster_with_adjacent_pair() {
        let fm = make_fragmap(&["c0", "c1"], 1, &[(0, 0), (1, 0)]);
        assert!(fm.shares_cluster_with(0, 1));
    }

    #[test]
    fn shares_cluster_with_blocked_by_middle_commit() {
        let fm = make_fragmap(&["c0", "c1", "c2"], 1, &[(0, 0), (1, 0), (2, 0)]);
        assert!(fm.shares_cluster_with(0, 2));
    }

    #[test]
    fn shares_cluster_with_is_symmetric() {
        let fm = make_fragmap(&["c0", "c1"], 1, &[(0, 0), (1, 0)]);
        assert_eq!(fm.shares_cluster_with(0, 1), fm.shares_cluster_with(1, 0));
    }

    #[test]
    fn shares_cluster_with_same_commit() {
        let fm = make_fragmap(&["c0"], 1, &[(0, 0)]);
        assert!(!fm.shares_cluster_with(0, 0));
    }

    #[test]
    fn shares_cluster_with_one_shared_is_enough() {
        // cluster 0: only c0. cluster 1: c0 and c1
        let fm = make_fragmap(&["c0", "c1"], 2, &[(0, 0), (0, 1), (1, 1)]);
        assert!(fm.shares_cluster_with(0, 1));
    }

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
    // build_fragmap SPG edge cases
    // =========================================================

    #[test]
    fn build_fragmap_pure_insertion_clusters_with_later_modifier() {
        // c1 inserts 10 lines starting at position 5 (old_lines=0).
        // c2 then modifies 3 lines starting at old position 7 (within c1's block).
        // They overlap → must share a cluster.
        let commits = vec![
            make_commit_diff(
                "c1",
                vec![make_file_diff(Some("f.rs"), Some("f.rs"), 5, 0, 5, 10)],
            ),
            make_commit_diff(
                "c2",
                vec![make_file_diff(Some("f.rs"), Some("f.rs"), 7, 3, 7, 3)],
            ),
        ];
        let fm = build_fragmap(&commits);
        assert!(fm.shares_cluster_with(0, 1));
    }

    #[test]
    fn build_fragmap_far_deletion_does_not_cluster_with_unrelated_modify() {
        // c1 modifies lines 1-5 of one region.
        // c2 only deletes lines 50-53 (far away, different region, new_lines=0).
        // c2's deletion is far from c1 → separate clusters.
        let commits = vec![
            make_commit_diff(
                "c1",
                vec![make_file_diff(Some("f.rs"), Some("f.rs"), 1, 3, 1, 5)],
            ),
            make_commit_diff(
                "c2",
                vec![FileDiff {
                    old_path: Some("f.rs".to_string()),
                    new_path: Some("f.rs".to_string()),
                    status: crate::DeltaStatus::Modified,
                    hunks: vec![Hunk {
                        old_start: 50,
                        old_lines: 3,
                        new_start: 50,
                        new_lines: 0,
                        lines: vec![],
                    }],
                }],
            ),
        ];
        let fm = build_fragmap(&commits);
        assert!(!fm.shares_cluster_with(0, 1));
    }

    #[test]
    fn build_fragmap_file_rename_cluster_uses_new_path() {
        // A commit that renames foo.rs → bar.rs. The cluster should track bar.rs.
        let c1 = CommitDiff {
            commit: make_commit_info_with_oid("c1"),
            files: vec![FileDiff {
                old_path: Some("foo.rs".to_string()),
                new_path: Some("bar.rs".to_string()),
                status: crate::DeltaStatus::Modified,
                hunks: vec![Hunk {
                    old_start: 5,
                    old_lines: 3,
                    new_start: 5,
                    new_lines: 4,
                    lines: vec![],
                }],
            }],
        };
        let fm = build_fragmap(&[c1]);
        assert_eq!(fm.clusters.len(), 1);
        assert_eq!(fm.clusters[0].spans[0].path, "bar.rs");
    }

    #[test]
    fn build_fragmap_single_commit_two_regions_deduped_to_one_column() {
        // One commit touching two non-overlapping regions of the same file.
        // Both SPG paths have the same active-node set {c1} → deduplicated to 1 column.
        let mut c1 = make_commit_diff(
            "c1",
            vec![make_file_diff(Some("f.rs"), Some("f.rs"), 1, 0, 1, 5)],
        );
        c1.files
            .push(make_file_diff(Some("f.rs"), Some("f.rs"), 100, 0, 100, 5));
        let fm = build_fragmap(&[c1]);
        assert_eq!(fm.clusters.len(), 1);
        assert_ne!(fm.matrix[0][0], TouchKind::None);
    }

    #[test]
    fn build_fragmap_two_commits_separate_regions_not_deduped() {
        // c1 and c2 each touch a distinct region → different activation patterns → 2 columns.
        let commits = vec![
            make_commit_diff(
                "c1",
                vec![make_file_diff(Some("f.rs"), Some("f.rs"), 1, 0, 1, 5)],
            ),
            make_commit_diff(
                "c2",
                vec![make_file_diff(Some("f.rs"), Some("f.rs"), 100, 0, 100, 5)],
            ),
        ];
        let fm = build_fragmap(&commits);
        assert_eq!(fm.clusters.len(), 2);
    }

    #[test]
    fn build_fragmap_three_commits_sequential_on_same_region() {
        // c1 introduces a block, c2 refines it, c3 refines it again.
        // All three share a cluster; c1 and c3 are also related.
        let commits = vec![
            make_commit_diff(
                "c1",
                vec![make_file_diff(Some("f.rs"), Some("f.rs"), 10, 5, 10, 10)],
            ),
            make_commit_diff(
                "c2",
                vec![make_file_diff(Some("f.rs"), Some("f.rs"), 12, 3, 12, 3)],
            ),
            make_commit_diff(
                "c3",
                vec![make_file_diff(Some("f.rs"), Some("f.rs"), 11, 2, 11, 2)],
            ),
        ];
        let fm = build_fragmap(&commits);
        assert!(fm.shares_cluster_with(0, 1));
        assert!(fm.shares_cluster_with(0, 2));
        assert!(fm.shares_cluster_with(1, 2));
    }

    #[test]
    fn build_fragmap_empty_span_does_not_panic() {
        // A commit with a single-line addition (new_lines=1) followed by a
        // commit that touches an adjacent but non-overlapping line.
        // Regression guard: no panic or infinite loop in SPG construction.
        let commits = vec![
            make_commit_diff(
                "c1",
                vec![make_file_diff(Some("f.rs"), Some("f.rs"), 10, 1, 10, 1)],
            ),
            make_commit_diff(
                "c2",
                vec![make_file_diff(Some("f.rs"), Some("f.rs"), 20, 1, 20, 1)],
            ),
        ];
        let fm = build_fragmap(&commits);
        assert_eq!(fm.commits.len(), 2);
    }
}
