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
    /// How many edges lead into each node, counting duplicates.
    fn predecessor_counts(&self) -> PerNode<usize> {
        let mut counts = PerNode::like(self.nodes, 0);
        for &to in self.succs.0.iter().flatten() {
            counts[to] += 1;
        }
        counts
    }
}

/// One column of a file: the generations of the commits touching it, and the
/// span of the last of them.
pub(super) struct SpgColumn {
    pub(super) generations: Vec<i32>,
    pub(super) last_span: SpgSpan,
}

/// Per commit set: the best suffix's key and its last active span.
type Suffixes = HashMap<ListId, (ListId, Option<SpgSpan>)>;

/// The lists the column program builds, and commit sets interned so that
/// equal sets are one id.
struct ColumnLists {
    keys: SharedTailLists<(i32, i64)>,
    sets: SharedTailLists<i32>,
    interned_sets: HashMap<(i32, ListId), ListId>,
}

impl ColumnLists {
    fn new() -> Self {
        ColumnLists {
            keys: SharedTailLists::new(),
            sets: SharedTailLists::new(),
            interned_sets: HashMap::new(),
        }
    }

    /// Merges `from` into `best`, keeping per commit set the smaller key.
    /// Only a smaller key replaces the best, so when successors are merged
    /// in enumeration order, of tied paths the one `spg_all_paths` keeps
    /// wins.
    fn keep_best(&self, best: &mut Suffixes, from: &Suffixes) {
        for (&set, &(key, span)) in from {
            let better = best
                .get(&set)
                .is_none_or(|&(current, _)| self.keys.cmp(key, current).is_lt());
            if better {
                best.insert(set, (key, span));
            }
        }
    }

    /// The suffixes through `node`: `best` with the node prepended when it is
    /// active.
    fn through(&mut self, node: &SpgNode, best: Suffixes) -> Suffixes {
        if !node.is_active {
            return best;
        }
        best.into_iter()
            .map(|(set, (key, span))| {
                let set = *self
                    .interned_sets
                    .entry((node.generation, set))
                    .or_insert_with(|| self.sets.prepend(node.generation, set));
                let key = self.keys.prepend(path_key(node), key);
                (set, (key, span.or(Some(node.new_span))))
            })
            .collect()
    }

    /// The source's suffixes as columns, in path order. A suffix without an
    /// active node touches no commit and is no column.
    fn into_columns(self, at_source: Suffixes) -> Vec<SpgColumn> {
        let mut columns: Vec<(ListId, ListId, SpgSpan)> = at_source
            .into_iter()
            .filter_map(|(set, (key, span))| Some((key, set, span?)))
            .collect();
        columns.sort_by(|(a, _, _), (b, _, _)| self.keys.cmp(*a, *b));
        columns
            .into_iter()
            .map(|(_, set, last_span)| SpgColumn {
                generations: self.sets.iter(set).collect(),
                last_span,
            })
            .collect()
    }
}

/// Every distinct set of commits a path through the graph touches, each with
/// the first such path in `spg_all_paths` order, listed in that order.
///
/// This is what the deduplicated matrix keeps of `spg_all_paths`, computed
/// without listing the paths: for each node, from the sink back, the best
/// suffix per commit set, keyed like `spg_all_paths` sorts. Two paths of one
/// set compare as their suffixes do when their prefixes agree, so the best
/// suffix per set is all a node needs. Paths of different sets never tie.
pub(super) fn spg_columns(spg: &Spg, poll: &mut impl FnMut() -> bool) -> Option<Vec<SpgColumn>> {
    let graph = IndexedSpg::new(spg);
    let mut lists = ColumnLists::new();
    let mut pending_preds = graph.predecessor_counts();
    let mut suffixes: PerNode<Option<Suffixes>> = PerNode::like(graph.nodes, None);
    suffixes[SINK] = Some(HashMap::from([(ListId::EMPTY, (ListId::EMPTY, None))]));

    for &at in graph.by_generation.iter().rev() {
        if at == SINK {
            continue;
        }
        if !poll() {
            return None;
        }
        let mut best = Suffixes::new();
        for &next in &graph.succs[at] {
            let next_suffixes = suffixes[next].as_ref().expect("successors come first");
            lists.keep_best(&mut best, next_suffixes);
        }
        suffixes[at] = Some(lists.through(&graph.nodes[at], best));
        release_successors(&graph, at, &mut pending_preds, &mut suffixes);
    }

    let at_source = suffixes[SOURCE].take().expect("source is never freed");
    Some(lists.into_columns(at_source))
}

/// Frees the suffixes of `at`'s successors that no other predecessor still
/// needs.
fn release_successors(
    graph: &IndexedSpg,
    at: NodeId,
    pending_preds: &mut PerNode<usize>,
    suffixes: &mut PerNode<Option<Suffixes>>,
) {
    for &next in &graph.succs[at] {
        pending_preds[next] -= 1;
        if pending_preds[next] == 0 {
            suffixes[next] = None;
        }
    }
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
    let mut keys = SharedTailLists::new();
    let suffix = best_suffixes(&graph, &mut keys, poll)?;
    let mut reversed = SharedTailLists::new();
    let prefix = best_prefixes(&graph, &mut reversed, poll)?;

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

/// What a node adds to the key `spg_all_paths` sorts paths by.
fn path_key(node: &SpgNode) -> (i32, i64) {
    (node.generation, node.new_span.start)
}

/// From each node to the sink, the node included: the key of the first such
/// path, or `None` where no path reaches the sink.
fn best_suffixes(
    graph: &IndexedSpg,
    keys: &mut SharedTailLists<(i32, i64)>,
    poll: &mut impl FnMut() -> bool,
) -> Option<PerNode<Option<ListId>>> {
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
            true => keys.prepend(path_key(node), key),
            false => key,
        });
    }
    Some(suffix)
}

/// From the source to each node, the node excluded: the key of the first such
/// path, stored last element first, or `None` where the source does not reach
/// the node.
fn best_prefixes(
    graph: &IndexedSpg,
    reversed: &mut SharedTailLists<(i32, i64)>,
    poll: &mut impl FnMut() -> bool,
) -> Option<PerNode<Option<ListId>>> {
    let mut prefix: PerNode<Option<ListId>> = PerNode::like(graph.nodes, None);
    prefix[SOURCE] = Some(ListId::EMPTY);
    for &at in &graph.by_generation {
        let Some(before) = prefix[at] else { continue };
        if !poll() {
            return None;
        }
        let node = &graph.nodes[at];
        let through = match node.is_active {
            true => reversed.prepend(path_key(node), before),
            false => before,
        };
        let through_in_order = in_order(reversed, through);
        for &next in &graph.succs[at] {
            let better = prefix[next].is_none_or(|current| {
                prefix_sorts_first(&through_in_order, &in_order(reversed, current))
            });
            if better {
                prefix[next] = Some(through);
            }
        }
    }
    Some(prefix)
}

/// A list stored last element first, in order.
fn in_order(reversed: &SharedTailLists<(i32, i64)>, list: ListId) -> Vec<(i32, i64)> {
    let mut elements: Vec<(i32, i64)> = reversed.iter(list).collect();
    elements.reverse();
    elements
}

/// Whether prefix `a` sorts before `b` given that the same suffix follows
/// both: a prefix that stops early sorts after one that continues it.
fn prefix_sorts_first(a: &[(i32, i64)], b: &[(i32, i64)]) -> bool {
    match a.iter().zip(b).find(|(x, y)| x != y) {
        Some((x, y)) => x < y,
        None => a.len() > b.len(),
    }
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
