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

// Static (non-TUI) fragmap renderer.
//
// Renders the fragmap matrix to a plain String, mimicking the output of the
// original fragmap tool. Each row shows: short SHA, commit title, then one
// character per cluster column.
//
// When `colors` is true the output uses ANSI escape codes (cyan SHA, grey
// title for fully-squashable commits, white/yellow/red background cells).
// When `colors` is false Unicode block characters are used instead, which
// produces clean readable output suitable for snapshot tests or plain-text
// piping.

use crate::{CommitDiff, fragmap as fm};

/// Symbol set used when rendering a row.
struct Symbols {
    /// ANSI prefix for the short SHA (e.g. cyan).
    sha_start: &'static str,
    /// ANSI reset / empty string.
    reset: &'static str,
    /// ANSI prefix for a fully-squashable commit's title (grey).
    grey_start: &'static str,
    /// String rendered when a commit directly touches a cluster.
    cell_touch: &'static str,
    /// String rendered for a squashable connector (between two touching commits with no conflict).
    cell_squashable: &'static str,
    /// String rendered for a conflicting connector.
    cell_conflicting: &'static str,
    /// String rendered for an empty cell.
    cell_empty: &'static str,
}

const COLORED: Symbols = Symbols {
    sha_start: "\x1b[36m",
    reset: "\x1b[0m",
    grey_start: "\x1b[90m",
    cell_touch: "\x1b[47m \x1b[0m",
    cell_squashable: "\x1b[43m \x1b[0m",
    cell_conflicting: "\x1b[41m \x1b[0m",
    cell_empty: ".",
};

const PLAIN: Symbols = Symbols {
    sha_start: "",
    reset: "",
    grey_start: "",
    cell_touch: "#",
    cell_squashable: "^",
    cell_conflicting: "|",
    cell_empty: ".",
};

/// Determine the connector kind between two touching commits in a cluster.
///
/// Returns `None` if only one side is present; `Some(true)` for squashable,
/// `Some(false)` for conflicting.
fn connector_is_squashable(
    fmap: &fm::FragMap,
    below_idx: usize,
    cluster_idx: usize,
) -> Option<bool> {
    let earlier = (0..below_idx).find(|&i| fmap.matrix[i][cluster_idx] != fm::TouchKind::None)?;
    Some(fmap.cluster_relation(earlier, below_idx, cluster_idx) == fm::SquashRelation::Squashable)
}

/// Render the fragmap matrix for all `commit_diffs` to a `String`.
///
/// `commit_diffs` contains all commits in display order (regular commits
/// followed by any synthetic rows such as staged/unstaged changes).
/// `full` controls whether identical cluster columns are deduplicated.
/// `colors` selects ANSI color output (matching the original fragmap tool)
/// versus plain Unicode output (suitable for tests and `--no-color` piping).
/// `reverse` prints rows oldest-first (highest matrix index first).
pub fn render(commit_diffs: &[CommitDiff], full: bool, colors: bool, reverse: bool) -> String {
    let s = if colors { &COLORED } else { &PLAIN };

    let fmap = fm::build_fragmap(commit_diffs, !full);
    let mut out = String::new();

    let indices: Box<dyn Iterator<Item = usize>> = if reverse {
        Box::new((0..commit_diffs.len()).rev())
    } else {
        Box::new(0..commit_diffs.len())
    };

    for commit_idx in indices {
        let diff = &commit_diffs[commit_idx];
        let commit = &diff.commit;
        let sha8 = &commit.oid[..8.min(commit.oid.len())];
        let title: String = commit.summary.chars().take(26).collect();
        let title_padded = format!("{title:<26}");

        out.push_str(s.sha_start);
        out.push_str(sha8);
        out.push_str(s.reset);
        out.push(' ');

        if fmap.is_fully_squashable(commit_idx) {
            out.push_str(s.grey_start);
            out.push_str(&title_padded);
            out.push_str(s.reset);
        } else {
            out.push_str(&title_padded);
        }

        for cluster_idx in 0..fmap.clusters.len() {
            if fmap.matrix[commit_idx][cluster_idx] != fm::TouchKind::None {
                out.push_str(s.cell_touch);
            } else {
                let has_above =
                    (0..commit_idx).any(|i| fmap.matrix[i][cluster_idx] != fm::TouchKind::None);
                let below = ((commit_idx + 1)..commit_diffs.len())
                    .find(|&i| fmap.matrix[i][cluster_idx] != fm::TouchKind::None);

                // In reverse display each gap row still uses the same matrix-based
                // connector logic: has_above checks lower indices and below checks
                // higher indices — the iteration order doesn't change the computation.
                if has_above && let Some(below_idx) = below {
                    match connector_is_squashable(&fmap, below_idx, cluster_idx) {
                        Some(true) => out.push_str(s.cell_squashable),
                        Some(false) => out.push_str(s.cell_conflicting),
                        None => out.push_str(s.cell_empty),
                    }
                } else {
                    out.push_str(s.cell_empty);
                }
            }
        }

        out.push('\n');
    }

    out
}
