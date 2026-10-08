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

//! Building a file's span propagation graph from its commits' hunks.

use super::{SINK, Spg, SpgNode, SpgOverlap, SpgSpan};
use crate::fragmap::{CommitPos, HunkInfo};

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

/// Build the SPG for a single file from its commits and hunks.
///
/// `poll` is called after each commit generation. Return `false` from `poll`
/// to interrupt early; in that case the function returns `None`.
pub(super) fn build_file_spg(
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

#[cfg(test)]
mod tests {
    use super::*;

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
