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

//! The matrix's columns and a split commit's columns, computed from the
//! graph without listing every path.

use std::collections::{HashMap, HashSet};

use super::paths::sorted_succs;
use super::shared_tail_lists::{ListId, SharedTailLists};
use super::{NodeId, PerNode, SINK, SOURCE, Spg, SpgNode, SpgSpan};

/// The graph's nodes, each node's successors in the order
/// `spg_enumerate_paths` visits them, and the nodes in generation order —
/// a topological order, since every edge leads to a later generation.
struct IndexedSpg<'a> {
    nodes: &'a PerNode<SpgNode>,
    succs: PerNode<Vec<NodeId>>,
    by_generation: Vec<NodeId>,
}

impl<'a> IndexedSpg<'a> {
    fn new(spg: &'a Spg) -> Self {
        let nodes = &spg.nodes;
        let succs = PerNode(nodes.ids().map(|n| sorted_succs(spg, n)).collect());
        let mut by_generation: Vec<NodeId> = nodes.ids().collect();
        by_generation.sort_by_key(|&n| nodes[n].generation);
        debug_assert!(nodes.ids().all(|from| {
            succs[from]
                .iter()
                .all(|&to| nodes[to].generation > nodes[from].generation)
        }));
        IndexedSpg {
            nodes,
            succs,
            by_generation,
        }
    }
}

/// One column of a file: the generations of the commits touching it, and the
/// span of the last of them.
pub(super) struct SpgColumn {
    pub(super) generations: Vec<i32>,
    pub(super) last_span: SpgSpan,
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
pub(super) fn spg_columns(spg: &Spg, poll: &mut impl FnMut() -> bool) -> Option<Vec<SpgColumn>> {
    let graph = IndexedSpg::new(spg);
    let mut keys: SharedTailLists<(i32, i64)> = SharedTailLists::new();
    let mut sets: SharedTailLists<i32> = SharedTailLists::new();
    let mut interned_sets: HashMap<(i32, ListId), ListId> = HashMap::new();

    let mut pending_preds: PerNode<usize> = PerNode::like(graph.nodes, 0);
    for to in graph.succs.0.iter().flatten() {
        pending_preds[*to] += 1;
    }

    // Per commit set: the best suffix's key and its last active span.
    type Suffixes = HashMap<ListId, (ListId, Option<SpgSpan>)>;
    let mut suffixes: PerNode<Option<Suffixes>> = PerNode::like(graph.nodes, None);
    suffixes[SINK] = Some(HashMap::from([(ListId::EMPTY, (ListId::EMPTY, None))]));

    for &at in graph.by_generation.iter().rev() {
        if at == SINK {
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
                        .or_insert_with(|| sets.prepend(node.generation, set));
                    let key = keys.prepend((node.generation, node.new_span.start), key);
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

    let mut columns: Vec<(ListId, ListId, SpgSpan)> = suffixes[SOURCE]
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
pub(super) fn spg_target_columns(
    spg: &Spg,
    target: i32,
    poll: &mut impl FnMut() -> bool,
) -> Option<Vec<(SpgSpan, Vec<i32>)>> {
    let graph = IndexedSpg::new(spg);
    let mut keys: SharedTailLists<(i32, i64)> = SharedTailLists::new();
    let key_of = |node: &SpgNode| (node.generation, node.new_span.start);

    // From each node to the sink, the node included.
    let mut suffix: PerNode<Option<ListId>> = PerNode::like(graph.nodes, None);
    suffix[SINK] = Some(ListId::EMPTY);
    for &at in graph.by_generation.iter().rev() {
        if at == SINK {
            continue;
        }
        if !poll() {
            return None;
        }
        let mut best: Option<ListId> = None;
        for &next in &graph.succs[at] {
            if let Some(key) = suffix[next]
                && best.is_none_or(|current| keys.cmp(key, current).is_lt())
            {
                best = Some(key);
            }
        }
        let node = &graph.nodes[at];
        suffix[at] = best.map(|key| match node.is_active {
            true => keys.prepend(key_of(node), key),
            false => key,
        });
    }

    // From the source to each node, the node excluded, last element first.
    let mut reversed: SharedTailLists<(i32, i64)> = SharedTailLists::new();
    let in_order = |reversed: &SharedTailLists<(i32, i64)>, list: ListId| {
        let mut elements: Vec<(i32, i64)> = reversed.iter(list).collect();
        elements.reverse();
        elements
    };
    let ends_first =
        |a: &[(i32, i64)], b: &[(i32, i64)]| match a.iter().zip(b).find(|(x, y)| x != y) {
            Some((x, y)) => x < y,
            None => a.len() > b.len(),
        };
    let mut prefix: PerNode<Option<ListId>> = PerNode::like(graph.nodes, None);
    prefix[SOURCE] = Some(ListId::EMPTY);
    for &at in &graph.by_generation {
        let Some(before) = prefix[at] else { continue };
        if !poll() {
            return None;
        }
        let node = &graph.nodes[at];
        let through = match node.is_active {
            true => reversed.prepend(key_of(node), before),
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

    let mut columns: Vec<(Vec<(i32, i64)>, SpgSpan)> = graph
        .nodes
        .ids()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fragmap::spg::build::build_file_spg;
    use crate::fragmap::spg::paths::spg_all_paths;
    use crate::fragmap::tests::{XorShift, random_history};
    use crate::fragmap::{FileLineages, collect_file_commits};

    /// A layered graph whose nodes share starts often enough that paths tie on
    /// their active `(generation, start)` and only their enumeration order
    /// tells them apart, with duplicate edges among them.
    fn random_graph(rng: &mut XorShift) -> Spg {
        let layers: Vec<Vec<SpgNode>> = (0..2 + rng.below(5) as i32)
            .map(|generation| {
                (0..1 + rng.below(4))
                    .map(|_| {
                        let span = |rng: &mut XorShift| {
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
            let mut rng = XorShift(seed * 2 + 1);
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
            let mut rng = XorShift(seed * 2 + 1);
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
            let diffs = random_history(seed);
            let lineages = FileLineages::new(&diffs);
            for commits in collect_file_commits(&diffs, &lineages).values() {
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
