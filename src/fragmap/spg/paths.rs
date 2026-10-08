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

//! Listing every path through the graph: what `--full` shows, and what
//! the faster column computations are checked against.

use std::collections::HashSet;

use super::{NodeId, SINK, SOURCE, Spg, SpgNode, SpgSpan};

/// `node`'s successors in the order paths through them are listed.
pub(super) fn sorted_succs(spg: &Spg, node: NodeId) -> Vec<NodeId> {
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
pub(super) fn spg_enumerate_paths(
    spg: &Spg,
    from: NodeId,
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
pub(super) fn spg_all_paths(
    spg: &Spg,
    poll: &mut impl FnMut() -> bool,
) -> Option<Vec<Vec<SpgNode>>> {
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
