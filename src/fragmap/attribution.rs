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

//! Exact line attribution for the commit being split.
//!
//! For each hunk of the target commit K, determine which other commits relate
//! to which of its lines — earlier commits whose output K's removed lines
//! rewrote, and later commits that reworked K's added lines.  This is computed
//! directly from the diffs by composing line-number maps between commits, so
//! every line of K gets exactly one relation set.  The hunk is then divided at
//! the points where the relation set changes; each resulting fragment belongs
//! wholly to one set.
//!
//! Splitting along these boundaries keeps the post-split matrix coherent: all
//! lines of a piece share one relation pattern, so a piece cannot end up
//! related to its sibling pieces, and regions that were merged into one column
//! stay merged.

use super::HunkInfo;
use super::assignment::{HunkFragment, LineRange};

/// Half-open `[start, end)` interval used for line-map arithmetic.
type Interval = (i64, i64);

/// One fragment of a target-commit hunk together with the commits related to
/// it.  The fragments of a hunk tile its old and new sides in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AttributedFragment {
    pub(super) fragment: HunkFragment,
    /// Sorted, deduplicated indices (into the commit-diff list) of the other
    /// commits relating to this fragment's lines.  Empty when only the target
    /// commit itself touches them.
    pub(super) related: Vec<usize>,
}

/// The span of lines a hunk removes, in old-file coordinates (empty for pure
/// insertions, positioned after the insertion point — matching the SPG's
/// convention so both engines agree on coordinates).
fn hunk_old_span(h: &HunkInfo) -> Interval {
    let mut start = h.old_start as i64;
    if h.old_lines == 0 {
        start += 1;
    }
    (start, start + h.old_lines as i64)
}

/// The span of lines a hunk adds, in new-file coordinates (empty for pure
/// deletions).
fn hunk_new_span(h: &HunkInfo) -> Interval {
    let mut start = h.new_start as i64;
    if h.new_lines == 0 {
        start += 1;
    }
    (start, start + h.new_lines as i64)
}

/// Map a position through a diff: positions past a hunk shift by that hunk's
/// size delta.  `check` is the position to compare against hunk boundaries
/// (for an exclusive end pass `p - 1`).
fn shift_position(p: i64, check: i64, hunks: &[HunkInfo], backward: bool) -> i64 {
    let mut ref_from: i64 = 0;
    let mut ref_to: i64 = 0;
    for hunk in hunks {
        let (from, to) = if backward {
            (hunk_new_span(hunk), hunk_old_span(hunk))
        } else {
            (hunk_old_span(hunk), hunk_new_span(hunk))
        };
        if check < from.1 {
            break;
        }
        ref_from = from.1;
        ref_to = to.1;
    }
    p - ref_from + ref_to
}

/// Map intervals through one diff, from its old frame to its new frame (or
/// the reverse with `backward`).
///
/// Parts of an interval that the diff does not touch shift by the size delta
/// of the hunks above them.  Parts that fall inside a hunk's source range are
/// replaced by that hunk's entire target range: the diff rewrote those lines,
/// so anything related to them is related to the whole replacement.  This is
/// what carries chain relations — a commit editing a replacement relates to
/// the lines the replacement consumed.
fn map_intervals(intervals: &[Interval], hunks: &[HunkInfo], backward: bool) -> Vec<Interval> {
    let mut result: Vec<Interval> = Vec::new();

    for &(start, end) in intervals {
        if start >= end {
            continue;
        }

        // Untouched parts survive; consumed parts map to the replacement.
        let mut remaining = vec![(start, end)];
        for hunk in hunks {
            let (from, to) = if backward {
                (hunk_new_span(hunk), hunk_old_span(hunk))
            } else {
                (hunk_old_span(hunk), hunk_new_span(hunk))
            };
            let mut next = Vec::new();
            for (s, e) in remaining {
                if e <= from.0 || s >= from.1 {
                    next.push((s, e));
                    continue;
                }
                if s < from.0 {
                    next.push((s, from.0));
                }
                if e > from.1 {
                    next.push((from.1, e));
                }
                if to.0 < to.1 {
                    result.push(to);
                }
            }
            remaining = next;
        }

        for (s, e) in remaining {
            let mapped_start = shift_position(s, s, hunks, backward);
            let mapped_end = shift_position(e, e - 1, hunks, backward);
            if mapped_start < mapped_end {
                result.push((mapped_start, mapped_end));
            }
        }
    }

    normalize(result)
}

/// Sort intervals and merge overlapping or adjacent ones.
fn normalize(mut intervals: Vec<Interval>) -> Vec<Interval> {
    intervals.sort();
    let mut merged: Vec<Interval> = Vec::new();
    for (s, e) in intervals {
        match merged.last_mut() {
            Some(last) if s <= last.1 => last.1 = last.1.max(e),
            _ => merged.push((s, e)),
        }
    }
    merged
}

/// A relation of one other commit to the target commit's lines: intervals in
/// the target's old frame (earlier commits) or new frame (later commits).
struct CommitRelation {
    commit_idx: usize,
    /// Intervals in the target hunks' old-file coordinates.
    old_intervals: Vec<Interval>,
    /// Intervals in the target hunks' new-file coordinates.
    new_intervals: Vec<Interval>,
}

/// Attribute the target commit's hunks in one file.
///
/// `commits_for_file` lists, in commit order, every commit touching the file
/// with its hunks (as produced by `collect_file_commits`); commits absent from
/// the file are identity maps and need no entry.  Returns one fragment list
/// per target hunk, or `None` when the target does not touch this file.
pub(super) fn attribute_target_hunks(
    commits_for_file: &[(usize, Vec<HunkInfo>)],
    target_commit_idx: usize,
) -> Option<Vec<Vec<AttributedFragment>>> {
    let target_pos = commits_for_file
        .iter()
        .position(|(idx, _)| *idx == target_commit_idx)?;
    let target_hunks = &commits_for_file[target_pos].1;

    let mut relations: Vec<CommitRelation> = Vec::new();

    // Earlier commits: map the lines they produced forward, through every
    // intermediate diff, into the target's old frame.
    for earlier_pos in 0..target_pos {
        let (commit_idx, hunks) = &commits_for_file[earlier_pos];
        let mut intervals: Vec<Interval> = hunks.iter().map(hunk_new_span).collect();
        for (_, between) in &commits_for_file[earlier_pos + 1..target_pos] {
            intervals = map_intervals(&intervals, between, false);
        }
        relations.push(CommitRelation {
            commit_idx: *commit_idx,
            old_intervals: normalize(intervals),
            new_intervals: Vec::new(),
        });
    }

    // Later commits: map the lines they consumed backward, through every
    // intermediate diff, into the target's new frame.
    for later_pos in target_pos + 1..commits_for_file.len() {
        let (commit_idx, hunks) = &commits_for_file[later_pos];
        let mut intervals: Vec<Interval> = hunks.iter().map(hunk_old_span).collect();
        for (_, between) in commits_for_file[target_pos + 1..later_pos].iter().rev() {
            intervals = map_intervals(&intervals, between, true);
        }
        relations.push(CommitRelation {
            commit_idx: *commit_idx,
            old_intervals: Vec::new(),
            new_intervals: normalize(intervals),
        });
    }

    Some(
        target_hunks
            .iter()
            .map(|hunk| attribute_hunk(hunk, &relations))
            .collect(),
    )
}

/// Divide one hunk at the points where its relation set changes.
fn attribute_hunk(hunk: &HunkInfo, relations: &[CommitRelation]) -> Vec<AttributedFragment> {
    let old_span = hunk_old_span(hunk);
    let new_span = hunk_new_span(hunk);
    let old_len = old_span.1 - old_span.0;
    let new_len = new_span.1 - new_span.0;
    let unified_len = old_len.max(new_len);

    // Project both sides onto a unified axis [0, unified_len) so old-side and
    // new-side relations can be segmented together (floor division is
    // monotone, so projected intervals keep tiling).
    let project = |interval: Interval, side: Interval, side_len: i64| -> Interval {
        if side_len == 0 {
            return (0, 0);
        }
        let a = (interval.0.max(side.0) - side.0) * unified_len / side_len;
        let b = (interval.1.min(side.1) - side.0) * unified_len / side_len;
        (a, b)
    };

    // Each relation's claim on this hunk, on the unified axis.
    let mut claims: Vec<(usize, Interval)> = Vec::new();
    for relation in relations {
        for &interval in &relation.old_intervals {
            let (a, b) = project(interval, old_span, old_len);
            if a < b {
                claims.push((relation.commit_idx, (a, b)));
            }
        }
        for &interval in &relation.new_intervals {
            let (a, b) = project(interval, new_span, new_len);
            if a < b {
                claims.push((relation.commit_idx, (a, b)));
            }
        }
    }

    // Segment at every claim boundary; each segment's pattern is the set of
    // commits claiming it.
    let mut cuts: Vec<i64> = claims
        .iter()
        .flat_map(|&(_, (a, b))| [a, b])
        .filter(|&c| c > 0 && c < unified_len)
        .collect();
    cuts.sort();
    cuts.dedup();

    let bounds: Vec<i64> = std::iter::once(0)
        .chain(cuts)
        .chain(std::iter::once(unified_len))
        .collect();
    let mut segments: Vec<(i64, i64, Vec<usize>)> = Vec::new();
    for pair in bounds.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        if start >= end {
            continue;
        }
        let mut related: Vec<usize> = claims
            .iter()
            .filter(|&&(_, (a, b))| a < end && start < b)
            .map(|&(commit_idx, _)| commit_idx)
            .collect();
        related.sort();
        related.dedup();
        // Merge with the previous segment when the pattern is unchanged.
        match segments.last_mut() {
            Some(prev) if prev.2 == related => prev.1 = end,
            _ => segments.push((start, end, related)),
        }
    }

    let unproject = |u: i64, side: Interval, side_len: i64| -> u32 {
        (side.0 + u * side_len / unified_len).max(0) as u32
    };
    segments
        .into_iter()
        .map(|(start, end, related)| AttributedFragment {
            fragment: HunkFragment {
                old_lines: LineRange {
                    start: unproject(start, old_span, old_len),
                    end: unproject(end, old_span, old_len),
                },
                new_lines: LineRange {
                    start: unproject(start, new_span, new_len),
                    end: unproject(end, new_span, new_len),
                },
            },
            related,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hunk(old_start: u32, old_lines: u32, new_start: u32, new_lines: u32) -> HunkInfo {
        HunkInfo {
            old_start,
            old_lines,
            new_start,
            new_lines,
        }
    }

    fn range(start: u32, end: u32) -> LineRange {
        LineRange { start, end }
    }

    #[test]
    fn map_forward_shifts_past_an_insertion() {
        // 3 lines inserted at line 5: positions after shift by +3.
        let hunks = vec![hunk(5, 0, 6, 3)];
        assert_eq!(map_intervals(&[(10, 12)], &hunks, false), vec![(13, 15)]);
        // Positions before are untouched.
        assert_eq!(map_intervals(&[(1, 4)], &hunks, false), vec![(1, 4)]);
    }

    #[test]
    fn map_forward_replaces_consumed_lines_with_the_whole_replacement() {
        // Lines 10-12 replaced by lines 10-15.
        let hunks = vec![hunk(10, 3, 10, 6)];
        // An interval inside the consumed range maps to the whole replacement.
        assert_eq!(map_intervals(&[(11, 12)], &hunks, false), vec![(10, 16)]);
        // An interval spanning the hunk keeps its outside parts too (adjacent
        // results merge).
        assert_eq!(map_intervals(&[(8, 15)], &hunks, false), vec![(8, 18)]);
    }

    #[test]
    fn map_backward_inverts_the_direction() {
        // Forward: lines 10-12 became 10-15.  Backward maps the replacement
        // range back to the consumed range.
        let hunks = vec![hunk(10, 3, 10, 6)];
        assert_eq!(map_intervals(&[(12, 14)], &hunks, true), vec![(10, 13)]);
        assert_eq!(map_intervals(&[(20, 22)], &hunks, true), vec![(17, 19)]);
    }

    #[test]
    fn later_commit_consuming_part_of_a_hunk_splits_it() {
        // K (idx 0) rewrites lines 1-2; C (idx 1) edits line 2 of K's output.
        let commits = vec![(0, vec![hunk(1, 2, 1, 2)]), (1, vec![hunk(2, 1, 2, 1)])];
        let fragments = attribute_target_hunks(&commits, 0).unwrap();
        assert_eq!(
            fragments[0],
            vec![
                AttributedFragment {
                    fragment: HunkFragment {
                        old_lines: range(1, 2),
                        new_lines: range(1, 2),
                    },
                    related: vec![],
                },
                AttributedFragment {
                    fragment: HunkFragment {
                        old_lines: range(2, 3),
                        new_lines: range(2, 3),
                    },
                    related: vec![1],
                },
            ]
        );
    }

    #[test]
    fn earlier_commits_split_a_hunk_by_their_regions() {
        // A (idx 0) wrote line 1, B (idx 1) wrote line 2; K (idx 2) rewrites
        // both lines in one hunk.
        let commits = vec![
            (0, vec![hunk(1, 1, 1, 1)]),
            (1, vec![hunk(2, 1, 2, 1)]),
            (2, vec![hunk(1, 2, 1, 2)]),
        ];
        let fragments = attribute_target_hunks(&commits, 2).unwrap();
        let related: Vec<&Vec<usize>> = fragments[0].iter().map(|f| &f.related).collect();
        assert_eq!(related, vec![&vec![0], &vec![1]]);
    }

    #[test]
    fn chain_relation_reaches_through_an_intermediate_commit() {
        // K (idx 0) creates lines 1-2; M (idx 1) rewrites line 2 into lines
        // 2-4; C (idx 2) edits line 3 — a line M created in place of K's line
        // 2.  C's relation must reach K's line 2 through M's replacement.
        let commits = vec![
            (0, vec![hunk(1, 0, 1, 2)]),
            (1, vec![hunk(2, 1, 2, 3)]),
            (2, vec![hunk(3, 1, 3, 1)]),
        ];
        let fragments = attribute_target_hunks(&commits, 0).unwrap();
        let with_c: Vec<_> = fragments[0]
            .iter()
            .filter(|f| f.related.contains(&2))
            .collect();
        assert_eq!(with_c.len(), 1);
        // The fragment C relates to is K's second added line — the one M's
        // replacement consumed — and M relates to it too.
        assert_eq!(with_c[0].fragment.new_lines, range(2, 3));
        assert!(with_c[0].related.contains(&1));
    }

    #[test]
    fn chain_relation_survives_multiple_intermediate_hops() {
        // K (idx 0) writes line 3.  Four intermediate commits perturb the
        // file around it — insert above (shifts K forward), delete that same
        // insertion (shifts K back), insert below (no effect), and an
        // unrelated same-size modify far above (no effect) — before C (idx 5)
        // finally touches K's line at its current position.  The relation
        // must still be found after composing all four hops' shifts.
        let commits = vec![
            (0, vec![hunk(3, 1, 3, 1)]), // K writes line 3
            (1, vec![hunk(1, 0, 2, 1)]), // hop1: insert above -> K shifts to 4
            (2, vec![hunk(2, 1, 1, 0)]), // hop2: delete that insertion -> K shifts back to 3
            (3, vec![hunk(5, 0, 6, 1)]), // hop3: insert below -> no effect
            (4, vec![hunk(1, 1, 1, 1)]), // hop4: unrelated same-size modify -> no effect
            (5, vec![hunk(3, 1, 3, 1)]), // C touches K's line at its current position
        ];
        let fragments = attribute_target_hunks(&commits, 0).unwrap();
        assert_eq!(fragments[0][0].related, vec![5]);
    }

    #[test]
    fn deletion_severs_a_chain_relation() {
        // A (idx 0) writes a line at position 2; DEL (idx 1) fully deletes
        // it; K (idx 2) later touches whatever now sits at that position —
        // content A never produced.  The relation must not leak through: an
        // earlier commit's contribution that gets entirely deleted
        // contributes nothing to what replaces it.
        let commits = vec![
            (0, vec![hunk(2, 1, 2, 1)]), // A writes line 2
            (1, vec![hunk(2, 1, 1, 0)]), // DEL deletes it (pure deletion)
            (2, vec![hunk(2, 1, 2, 1)]), // K touches the same position, post-deletion
        ];
        let fragments = attribute_target_hunks(&commits, 2).unwrap();
        assert_eq!(fragments[0][0].related, Vec::<usize>::new());
    }

    #[test]
    fn gap_commits_do_not_blur_attribution() {
        // K (idx 0) rewrites lines 1-2; idx 1 does not touch this file (no
        // entry); C (idx 2) edits line 2.  Attribution must be exact despite
        // the gap.
        let commits = vec![(0, vec![hunk(1, 2, 1, 2)]), (2, vec![hunk(2, 1, 2, 1)])];
        let fragments = attribute_target_hunks(&commits, 0).unwrap();
        let related: Vec<&Vec<usize>> = fragments[0].iter().map(|f| &f.related).collect();
        assert_eq!(related, vec![&vec![], &vec![2]]);
    }

    #[test]
    fn unrelated_middle_region_becomes_its_own_fragment() {
        // A (idx 0) wrote lines 1-2, B (idx 1) wrote lines 8-9; K (idx 2)
        // rewrites lines 1-9.  The middle nobody else touched is its own
        // relation set (empty), exactly as the matrix shows it as a K-only
        // column.
        let commits = vec![
            (0, vec![hunk(1, 2, 1, 2)]),
            (1, vec![hunk(8, 2, 8, 2)]),
            (2, vec![hunk(1, 9, 1, 9)]),
        ];
        let fragments = attribute_target_hunks(&commits, 2).unwrap();
        let related: Vec<&Vec<usize>> = fragments[0].iter().map(|f| &f.related).collect();
        assert_eq!(related, vec![&vec![0], &vec![], &vec![1]]);
    }

    #[test]
    fn target_absent_from_file_yields_none() {
        let commits = vec![(0, vec![hunk(1, 1, 1, 1)])];
        assert!(attribute_target_hunks(&commits, 5).is_none());
    }
}
