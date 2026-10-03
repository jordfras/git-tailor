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

//! The hunk-group assignment types for splitting a commit per hunk group.
//!
//! Each hunk of the commit being split — or a fragment of it, when its lines
//! relate to different commits — is routed to one group.  A group is a column
//! of the matrix paired with a relation pattern: the set of other commits
//! whose changes a fragment's lines concern, computed exactly by the
//! [`attribution`](super::attribution) module.  Splitting along these groups
//! keeps every resulting commit related only to the commits its lines actually
//! concern, never to sibling pieces.
//!
//! The relation pattern alone would not do, because lines that only add
//! rewrite nobody's output and so relate to nothing — a commit made entirely
//! of insertions would collapse into one group however many columns it
//! occupies.

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

/// One fragment routed to one output group.
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
    /// The hunk is split into fragments that go to different groups.
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
    /// Number of distinct groups (column and relation pattern).
    pub group_count: usize,
    /// Per change of the split commit, one entry per hunk; see [`Self::hunk`].
    pub(super) by_change: Vec<Vec<HunkAssignment>>,
}

impl HunkGroupAssignment {
    /// The assignment of hunk `hunk` of change `change`, both numbered as in
    /// the split commit's 0-context, rename-detecting diff — the one
    /// `GitRepo::commit_diff_for_fragmap` produces. `None` for a change with
    /// no hunks.
    pub fn hunk(&self, change: usize, hunk: usize) -> Option<&HunkAssignment> {
        self.by_change.get(change)?.get(hunk)
    }
}
