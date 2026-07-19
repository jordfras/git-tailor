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

//! Hunk-group assignment types and the covering heuristic.
//!
//! [`assign_hunk_groups`](super::assign_hunk_groups) clusters the branch and
//! must then decide which deduplicated group (fragmap column) each hunk of the
//! commit being split belongs to.  A hunk can appear in several clusters (via
//! different SPG paths), so the assignment is a covering problem: every group
//! the commit touches should receive at least one hunk.

use std::collections::{BTreeSet, HashMap, HashSet};

/// How one hunk of the split commit maps to output groups.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HunkAssignment {
    /// The entire hunk goes to a single group.
    Whole { group: usize },
}

impl HunkAssignment {
    /// All groups this hunk contributes changes to.
    pub fn groups(&self) -> Vec<usize> {
        match self {
            HunkAssignment::Whole { group } => vec![*group],
        }
    }

    /// The group of the whole hunk, if it is not fragmented.
    pub fn whole_group(&self) -> Option<usize> {
        match self {
            HunkAssignment::Whole { group } => Some(*group),
        }
    }
}

/// The hunk-group assignment for a commit being split per hunk group.
#[derive(Debug, Clone)]
pub struct HunkGroupAssignment {
    /// Number of distinct groups (fragmap columns) after deduplication.
    pub group_count: usize,
    /// Per file path, one entry per hunk of the split commit — indexed the
    /// same way as the 0-context full diff produced by
    /// `GitRepo::commit_diff_for_fragmap`.
    pub by_file: HashMap<String, Vec<HunkAssignment>>,
}

impl HunkGroupAssignment {
    /// The distinct groups that receive at least one hunk, in ascending order.
    pub fn touched_groups(&self) -> Vec<usize> {
        let touched: BTreeSet<usize> = self
            .by_file
            .values()
            .flatten()
            .flat_map(|a| a.groups())
            .collect();
        touched.into_iter().collect()
    }
}

/// One hunk of the split commit together with the groups it could belong to.
#[derive(Debug)]
pub(super) struct GroupCandidate {
    pub(super) path: String,
    pub(super) hunk_idx: usize,
    /// Sorted, deduplicated, non-empty list of possible group indices.
    pub(super) possible_groups: Vec<usize>,
}

/// Assign each candidate hunk to exactly one group so that every group any
/// candidate can reach receives at least one hunk where possible.
///
/// Strategy:
///  1. Hunks with only one possible group are assigned immediately.
///  2. For uncovered groups, pick the unassigned hunk with the fewest
///     alternatives and assign it there.
///  3. Remaining unassigned hunks go to their lowest possible group.
pub(super) fn assign_by_covering(
    candidates: &[GroupCandidate],
    group_count: usize,
) -> HunkGroupAssignment {
    // All groups any candidate can reach.
    let all_touched: BTreeSet<usize> = candidates
        .iter()
        .flat_map(|c| c.possible_groups.iter().copied())
        .collect();

    let mut assigned: Vec<Option<usize>> = vec![None; candidates.len()];

    // Pass 1: hunks with only one possible group.
    for (i, candidate) in candidates.iter().enumerate() {
        if candidate.possible_groups.len() == 1 {
            assigned[i] = Some(candidate.possible_groups[0]);
        }
    }

    // Pass 2: for each uncovered group, assign the best remaining hunk.
    let mut covered: HashSet<usize> = assigned.iter().filter_map(|a| *a).collect();
    for &group in &all_touched {
        if covered.contains(&group) {
            continue;
        }
        let best = candidates
            .iter()
            .enumerate()
            .filter(|(i, c)| assigned[*i].is_none() && c.possible_groups.contains(&group))
            .min_by_key(|(_, c)| c.possible_groups.len())
            .map(|(i, _)| i);
        if let Some(i) = best {
            assigned[i] = Some(group);
            covered.insert(group);
        }
    }

    // Pass 3: remaining hunks go to their lowest possible group.
    for (i, candidate) in candidates.iter().enumerate() {
        if assigned[i].is_none() {
            assigned[i] = Some(candidate.possible_groups[0]);
        }
    }

    // Exactly one candidate exists per hunk, so per-path hunk counts can be
    // derived by counting candidates.
    let mut hunk_counts: HashMap<&str, usize> = HashMap::new();
    for candidate in candidates {
        *hunk_counts.entry(candidate.path.as_str()).or_default() += 1;
    }

    let mut by_file: HashMap<String, Vec<HunkAssignment>> = HashMap::new();
    for (i, candidate) in candidates.iter().enumerate() {
        let entry = by_file.entry(candidate.path.clone()).or_insert_with(|| {
            vec![HunkAssignment::Whole { group: 0 }; hunk_counts[candidate.path.as_str()]]
        });
        entry[candidate.hunk_idx] = HunkAssignment::Whole {
            group: assigned[i].unwrap_or(0),
        };
    }

    HunkGroupAssignment {
        group_count,
        by_file,
    }
}
