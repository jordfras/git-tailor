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

//! Hunk-group assignment: routing each hunk of the commit being split — or
//! fragments of it — to a deduplicated fragmap group (matrix column).
//!
//! A hunk can lie on several SPG paths, and therefore in several columns: each
//! path claims the sub-region of the hunk it runs through (recorded as
//! [`GroupClaim`] overlaps).  When those claims separate cleanly, the hunk is
//! partitioned into [`HunkFragment`]s so every column can receive its own
//! part.  A covering pass then assigns each unit (whole hunk or fragment) to
//! one group so that every group the commit touches gets at least one unit
//! where possible.

use std::collections::{BTreeSet, HashMap, HashSet};

/// Half-open `[start, end)` range of 1-based file line numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineRange {
    pub start: u32,
    pub end: u32,
}

impl LineRange {
    pub fn len(&self) -> u32 {
        self.end.saturating_sub(self.start)
    }

    pub fn is_empty(&self) -> bool {
        self.start >= self.end
    }
}

/// A contiguous piece of one hunk, addressed on both sides of the diff.
///
/// `old_lines` is a sub-range of the hunk's removed lines in old-file
/// coordinates; `new_lines` a sub-range of its added lines in new-file
/// coordinates.  Either side may be empty (pure-insertion / pure-deletion
/// fragments).  The fragments of one hunk tile both sides in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HunkFragment {
    pub old_lines: LineRange,
    pub new_lines: LineRange,
}

/// One fragment routed to one output group (fragmap column).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FragmentAssignment {
    pub fragment: HunkFragment,
    pub group: usize,
}

/// How one hunk of the split commit maps to output groups.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HunkAssignment {
    /// The entire hunk goes to a single group.
    Whole { group: usize },
    /// The hunk is split into fragments that may go to different groups.
    Fragmented { fragments: Vec<FragmentAssignment> },
}

impl HunkAssignment {
    /// All groups this hunk contributes changes to.
    pub fn groups(&self) -> Vec<usize> {
        match self {
            HunkAssignment::Whole { group } => vec![*group],
            HunkAssignment::Fragmented { fragments } => fragments.iter().map(|f| f.group).collect(),
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

/// One deduplicated group's claim on a sub-region of a hunk, derived from the
/// SPG path(s) that put the hunk in that group's column.
#[derive(Debug, Clone, Copy)]
pub(super) struct GroupClaim {
    pub(super) group: usize,
    /// The part of the hunk's old span this group relates to (the lines the
    /// hunk rewrote from an earlier commit on the path).
    pub(super) old_overlap: Option<LineRange>,
    /// The part of the hunk's new span this group relates to (the lines a
    /// later commit on the path consumed).
    pub(super) new_overlap: Option<LineRange>,
}

/// One hunk of the split commit together with every group claim on it.
#[derive(Debug, Clone)]
pub(super) struct HunkClaims {
    pub(super) path: String,
    pub(super) hunk_idx: usize,
    /// The hunk's removed-lines extent (old-file coordinates).
    pub(super) old_span: LineRange,
    /// The hunk's added-lines extent (new-file coordinates).
    pub(super) new_span: LineRange,
    /// May contain several claims for the same group (one per SPG path).
    pub(super) claims: Vec<GroupClaim>,
}

/// A unit the covering pass assigns to one group: a whole hunk, or one
/// fragment of a partitioned hunk.
#[derive(Debug)]
struct AssignmentUnit {
    /// Index into the `HunkClaims` list this unit belongs to.
    claims_idx: usize,
    /// `None` for a whole hunk; `Some` for one fragment of it.
    fragment: Option<HunkFragment>,
    /// Sorted, deduplicated, non-empty list of possible group indices.
    possible_groups: Vec<usize>,
}

/// Compute the final assignment for all hunks of the split commit.
///
/// Each hunk is first partitioned into fragments where its group claims
/// separate cleanly (see [`partition_hunk`]); a global covering pass then
/// fixes one group per unit so that every group any unit can reach receives
/// at least one unit where possible.
pub(super) fn assign_hunks(all_claims: &[HunkClaims], group_count: usize) -> HunkGroupAssignment {
    let mut units: Vec<AssignmentUnit> = Vec::new();
    for (claims_idx, hunk) in all_claims.iter().enumerate() {
        units.extend(partition_hunk(claims_idx, hunk));
    }

    let chosen = cover_units(&units);

    // Exactly one HunkClaims exists per hunk, so per-path hunk counts can be
    // derived by counting them.
    let mut hunk_counts: HashMap<&str, usize> = HashMap::new();
    for hunk in all_claims {
        *hunk_counts.entry(hunk.path.as_str()).or_default() += 1;
    }

    let mut by_file: HashMap<String, Vec<HunkAssignment>> = HashMap::new();
    for (claims_idx, hunk) in all_claims.iter().enumerate() {
        let fragments: Vec<FragmentAssignment> = units
            .iter()
            .zip(&chosen)
            .filter(|(u, _)| u.claims_idx == claims_idx)
            .map(|(u, &group)| FragmentAssignment {
                // A whole-hunk unit is a single fragment covering everything.
                fragment: u.fragment.unwrap_or(HunkFragment {
                    old_lines: hunk.old_span,
                    new_lines: hunk.new_span,
                }),
                group,
            })
            .collect();

        let all_same_group = fragments.iter().all(|f| f.group == fragments[0].group);
        let assignment = if fragments.len() <= 1 || all_same_group {
            HunkAssignment::Whole {
                group: fragments.first().map(|f| f.group).unwrap_or(0),
            }
        } else {
            HunkAssignment::Fragmented { fragments }
        };

        let entry = by_file.entry(hunk.path.clone()).or_insert_with(|| {
            vec![HunkAssignment::Whole { group: 0 }; hunk_counts[hunk.path.as_str()]]
        });
        entry[hunk.hunk_idx] = assignment;
    }

    HunkGroupAssignment {
        group_count,
        by_file,
    }
}

/// Partition one hunk into assignment units according to its group claims.
///
/// The two sides of the hunk are projected onto a common unified axis
/// `[0, L)` (`L` = the longer side), old-side claims via the old projection
/// and new-side claims via the new one.  Claim boundaries become cut points;
/// the segments between cuts become fragments, each with the groups whose
/// claims cover it as candidates.  Unclaimed slack merges into the preceding
/// fragment.  When no interior cut exists (a single group, or claims that all
/// span the whole hunk) the hunk stays whole with all claimed groups as
/// candidates — the covering pass then behaves exactly as before this
/// partitioning existed.
fn partition_hunk(claims_idx: usize, hunk: &HunkClaims) -> Vec<AssignmentUnit> {
    let mut groups: Vec<usize> = hunk.claims.iter().map(|c| c.group).collect();
    groups.sort();
    groups.dedup();
    if groups.is_empty() {
        groups.push(0);
    }

    let whole = |groups: Vec<usize>| {
        vec![AssignmentUnit {
            claims_idx,
            fragment: None,
            possible_groups: groups,
        }]
    };

    let old_len = hunk.old_span.len() as i64;
    let new_len = hunk.new_span.len() as i64;
    let unified_len = old_len.max(new_len);
    if groups.len() < 2 || unified_len == 0 {
        return whole(groups);
    }

    // Project a range from one side onto the unified axis (floor division is
    // monotone, and 0 / side_len map to 0 / unified_len exactly, so projected
    // ranges keep tiling).
    let project = |range: &LineRange, side_start: u32, side_len: i64| -> Option<(i64, i64)> {
        if side_len == 0 {
            return None;
        }
        let a = (range.start.saturating_sub(side_start)) as i64;
        let b = (range.end.saturating_sub(side_start)) as i64;
        Some((a * unified_len / side_len, b * unified_len / side_len))
    };

    // Each claim contributes up to two intervals on the unified axis.
    let mut claim_intervals: Vec<(usize, i64, i64)> = Vec::new();
    for claim in &hunk.claims {
        if let Some(range) = &claim.old_overlap
            && let Some((a, b)) = project(range, hunk.old_span.start, old_len)
            && a < b
        {
            claim_intervals.push((claim.group, a, b));
        }
        if let Some(range) = &claim.new_overlap
            && let Some((a, b)) = project(range, hunk.new_span.start, new_len)
            && a < b
        {
            claim_intervals.push((claim.group, a, b));
        }
    }

    let mut cuts: Vec<i64> = claim_intervals
        .iter()
        .flat_map(|&(_, a, b)| [a, b])
        .filter(|&c| c > 0 && c < unified_len)
        .collect();
    cuts.sort();
    cuts.dedup();
    if cuts.is_empty() {
        return whole(groups);
    }

    // Build segments between cuts, with the groups whose claims cover each.
    let bounds: Vec<i64> = std::iter::once(0)
        .chain(cuts)
        .chain(std::iter::once(unified_len))
        .collect();
    let mut segments: Vec<(i64, i64, Vec<usize>)> = Vec::new();
    for pair in bounds.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        let mut covering: Vec<usize> = claim_intervals
            .iter()
            .filter(|&&(_, a, b)| a < end && start < b)
            .map(|&(group, _, _)| group)
            .collect();
        covering.sort();
        covering.dedup();
        segments.push((start, end, covering));
    }

    // Unclaimed slack merges into the preceding segment (or the following one
    // for leading slack); adjacent segments with identical candidates merge.
    let mut merged: Vec<(i64, i64, Vec<usize>)> = Vec::new();
    for segment in segments {
        match merged.last_mut() {
            Some(prev) if segment.2.is_empty() || prev.2 == segment.2 => prev.1 = segment.1,
            Some(prev) if prev.2.is_empty() => {
                prev.1 = segment.1;
                prev.2 = segment.2;
            }
            _ => merged.push(segment),
        }
    }
    if merged.len() < 2 {
        return whole(groups);
    }

    // Map each unified segment back to concrete old/new sub-ranges.
    let unproject = |u: i64, side_start: u32, side_len: i64| -> u32 {
        side_start + (u * side_len / unified_len) as u32
    };
    merged
        .into_iter()
        .map(|(start, end, covering)| AssignmentUnit {
            claims_idx,
            fragment: Some(HunkFragment {
                old_lines: LineRange {
                    start: unproject(start, hunk.old_span.start, old_len),
                    end: unproject(end, hunk.old_span.start, old_len),
                },
                new_lines: LineRange {
                    start: unproject(start, hunk.new_span.start, new_len),
                    end: unproject(end, hunk.new_span.start, new_len),
                },
            }),
            possible_groups: if covering.is_empty() {
                groups.clone()
            } else {
                covering
            },
        })
        .collect()
}

/// Choose one group per unit so that every group any unit can reach receives
/// at least one unit where possible.
///
/// Strategy:
///  1. Units with only one possible group are assigned immediately.
///  2. For uncovered groups, pick the unassigned unit with the fewest
///     alternatives and assign it there.
///  3. Remaining unassigned units go to their lowest possible group.
fn cover_units(units: &[AssignmentUnit]) -> Vec<usize> {
    let all_touched: BTreeSet<usize> = units
        .iter()
        .flat_map(|u| u.possible_groups.iter().copied())
        .collect();

    let mut assigned: Vec<Option<usize>> = vec![None; units.len()];

    // Pass 1: units with only one possible group.
    for (i, unit) in units.iter().enumerate() {
        if unit.possible_groups.len() == 1 {
            assigned[i] = Some(unit.possible_groups[0]);
        }
    }

    // Pass 2: for each uncovered group, assign the best remaining unit.
    let mut covered: HashSet<usize> = assigned.iter().filter_map(|a| *a).collect();
    for &group in &all_touched {
        if covered.contains(&group) {
            continue;
        }
        let best = units
            .iter()
            .enumerate()
            .filter(|(i, u)| assigned[*i].is_none() && u.possible_groups.contains(&group))
            .min_by_key(|(_, u)| u.possible_groups.len())
            .map(|(i, _)| i);
        if let Some(i) = best {
            assigned[i] = Some(group);
            covered.insert(group);
        }
    }

    // Pass 3: remaining units go to their lowest possible group.
    for (i, unit) in units.iter().enumerate() {
        if assigned[i].is_none() {
            assigned[i] = Some(unit.possible_groups[0]);
        }
    }

    assigned.into_iter().map(|a| a.unwrap_or(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(start: u32, end: u32) -> LineRange {
        LineRange { start, end }
    }

    fn claims(old_span: LineRange, new_span: LineRange, claims: Vec<GroupClaim>) -> HunkClaims {
        HunkClaims {
            path: "a.txt".to_string(),
            hunk_idx: 0,
            old_span,
            new_span,
            claims,
        }
    }

    fn old_claim(group: usize, start: u32, end: u32) -> GroupClaim {
        GroupClaim {
            group,
            old_overlap: Some(range(start, end)),
            new_overlap: None,
        }
    }

    fn new_claim(group: usize, start: u32, end: u32) -> GroupClaim {
        GroupClaim {
            group,
            old_overlap: None,
            new_overlap: Some(range(start, end)),
        }
    }

    #[test]
    fn single_group_stays_whole() {
        let hunk = claims(range(1, 3), range(1, 3), vec![old_claim(1, 1, 2)]);
        let result = assign_hunks(&[hunk], 2);
        assert_eq!(
            result.by_file["a.txt"][0],
            HunkAssignment::Whole { group: 1 }
        );
    }

    #[test]
    fn two_old_side_claims_split_the_hunk() {
        // Old lines 1–2 claimed by group 0, line 3 by group 1; equal-length
        // replacement so new side mirrors the old offsets.
        let hunk = claims(
            range(1, 4),
            range(1, 4),
            vec![old_claim(0, 1, 3), old_claim(1, 3, 4)],
        );
        let result = assign_hunks(&[hunk], 2);
        assert_eq!(
            result.by_file["a.txt"][0],
            HunkAssignment::Fragmented {
                fragments: vec![
                    FragmentAssignment {
                        fragment: HunkFragment {
                            old_lines: range(1, 3),
                            new_lines: range(1, 3),
                        },
                        group: 0,
                    },
                    FragmentAssignment {
                        fragment: HunkFragment {
                            old_lines: range(3, 4),
                            new_lines: range(3, 4),
                        },
                        group: 1,
                    },
                ]
            }
        );
        assert_eq!(result.touched_groups(), vec![0, 1]);
    }

    #[test]
    fn new_side_claims_split_a_pure_insertion() {
        // Pure insertion (empty old span): only new-side claims can exist.
        let hunk = claims(
            range(5, 5),
            range(5, 9),
            vec![new_claim(0, 5, 7), new_claim(1, 7, 9)],
        );
        let result = assign_hunks(&[hunk], 2);
        match &result.by_file["a.txt"][0] {
            HunkAssignment::Fragmented { fragments } => {
                assert_eq!(fragments.len(), 2);
                assert!(fragments.iter().all(|f| f.fragment.old_lines.is_empty()));
                assert_eq!(fragments[0].fragment.new_lines, range(5, 7));
                assert_eq!(fragments[1].fragment.new_lines, range(7, 9));
                assert_eq!(fragments[0].group, 0);
                assert_eq!(fragments[1].group, 1);
            }
            other => panic!("expected Fragmented, got {:?}", other),
        }
    }

    #[test]
    fn full_span_claims_cannot_split_and_fall_back_to_covering() {
        // Both groups claim the entire old span — no interior cut exists, so
        // the hunk stays whole and the covering pass picks one group.
        let hunk = claims(
            range(1, 4),
            range(1, 4),
            vec![old_claim(0, 1, 4), old_claim(1, 1, 4)],
        );
        let result = assign_hunks(&[hunk], 2);
        assert!(matches!(
            result.by_file["a.txt"][0],
            HunkAssignment::Whole { .. }
        ));
        assert_eq!(result.touched_groups().len(), 1);
    }

    #[test]
    fn unclaimed_middle_slack_merges_into_preceding_fragment() {
        // Groups claim lines 1–2 and 8–9; the middle 3–7 is unclaimed and must
        // ride along with the first fragment, not become its own unit.
        let hunk = claims(
            range(1, 10),
            range(1, 10),
            vec![old_claim(0, 1, 3), old_claim(1, 8, 10)],
        );
        let result = assign_hunks(&[hunk], 2);
        match &result.by_file["a.txt"][0] {
            HunkAssignment::Fragmented { fragments } => {
                assert_eq!(fragments.len(), 2);
                assert_eq!(fragments[0].fragment.old_lines, range(1, 8));
                assert_eq!(fragments[1].fragment.old_lines, range(8, 10));
            }
            other => panic!("expected Fragmented, got {:?}", other),
        }
    }

    #[test]
    fn covering_still_fills_groups_across_whole_hunks() {
        // Two hunks; the first can serve groups 0 or 1, the second only 0.
        // Covering must send the first to group 1 so both groups get a hunk.
        let ambiguous = HunkClaims {
            path: "a.txt".to_string(),
            hunk_idx: 0,
            old_span: range(1, 2),
            new_span: range(1, 2),
            claims: vec![old_claim(0, 1, 2), old_claim(1, 1, 2)],
        };
        let fixed = HunkClaims {
            path: "a.txt".to_string(),
            hunk_idx: 1,
            old_span: range(10, 11),
            new_span: range(10, 11),
            claims: vec![old_claim(0, 10, 11)],
        };
        let result = assign_hunks(&[ambiguous, fixed], 2);
        assert_eq!(result.touched_groups(), vec![0, 1]);
        assert_eq!(
            result.by_file["a.txt"][0],
            HunkAssignment::Whole { group: 1 }
        );
        assert_eq!(
            result.by_file["a.txt"][1],
            HunkAssignment::Whole { group: 0 }
        );
    }

    #[test]
    fn uneven_side_lengths_project_proportionally() {
        // Old side 2 lines, new side 4 lines; an old-side cut after line 1
        // maps to the middle of the new side.
        let hunk = claims(
            range(1, 3),
            range(1, 5),
            vec![old_claim(0, 1, 2), old_claim(1, 2, 3)],
        );
        let result = assign_hunks(&[hunk], 2);
        match &result.by_file["a.txt"][0] {
            HunkAssignment::Fragmented { fragments } => {
                assert_eq!(fragments[0].fragment.old_lines, range(1, 2));
                assert_eq!(fragments[0].fragment.new_lines, range(1, 3));
                assert_eq!(fragments[1].fragment.old_lines, range(2, 3));
                assert_eq!(fragments[1].fragment.new_lines, range(3, 5));
            }
            other => panic!("expected Fragmented, got {:?}", other),
        }
    }
}
