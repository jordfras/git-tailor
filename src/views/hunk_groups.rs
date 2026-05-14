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

// Fragmap (hunk-group) rendering helpers extracted from commit_list.
//
// These functions build the third column of the commit table — the
// cluster-matrix visualization — plus its horizontal scrollbar.

use crate::fragmap::{self, SquashableScope, TouchKind};
use crate::views::theme::{
    CommitRowRole, ConnectorRelation, ConnectorRole, FragmapTheme, SquareRelation, SquareRole,
};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Cell, Scrollbar, ScrollbarOrientation, ScrollbarState},
};

// Background applied to the fragmap matrix columns of the selected row.
const COLOR_SELECTED_FRAGMAP_BG: Color = Color::Rgb(60, 60, 80);

/// Determine a commit's relationship to the earliest earlier commit in a cluster.
///
/// Returns None if the commit doesn't touch the cluster or no earlier commit does.
fn cluster_relation(
    fragmap: &fragmap::FragMap,
    commit_idx: usize,
    cluster_idx: usize,
) -> Option<fragmap::SquashRelation> {
    if fragmap.matrix[commit_idx][cluster_idx] == TouchKind::None {
        return None;
    }
    for earlier_idx in (0..commit_idx).rev() {
        if fragmap.matrix[earlier_idx][cluster_idx] != TouchKind::None {
            return Some(fragmap.cluster_relation(earlier_idx, commit_idx, cluster_idx));
        }
    }
    None
}

/// Determine cell content and style for a commit-cluster intersection.
///
/// Returns None if the commit doesn't touch the cluster.
/// With `Group` scope a square is grey when that group pair is squashable.
/// With `Commit` scope a square is grey only when the entire commit is fully
/// squashable into a single target commit.
fn fragmap_cell_content(
    fragmap: &fragmap::FragMap,
    commit_idx: usize,
    cluster_idx: usize,
    scope: SquashableScope,
    role: SquareRole,
    theme: &dyn FragmapTheme,
) -> Option<(String, Style)> {
    if fragmap.matrix[commit_idx][cluster_idx] == TouchKind::None {
        return None;
    }

    let has_earlier = (0..commit_idx).any(|i| fragmap.matrix[i][cluster_idx] != TouchKind::None);

    let rel = if !has_earlier {
        SquareRelation::Origin
    } else {
        match scope {
            SquashableScope::Group => match cluster_relation(fragmap, commit_idx, cluster_idx) {
                Some(fragmap::SquashRelation::Squashable) => SquareRelation::Squashable,
                _ => SquareRelation::Conflict,
            },
            SquashableScope::Commit => {
                if fragmap.is_fully_squashable(commit_idx) {
                    SquareRelation::Squashable
                } else {
                    SquareRelation::Conflict
                }
            }
        }
    };
    Some((
        theme.square_symbol(role, rel).to_owned(),
        theme.square_style(role, rel),
    ))
}

/// Determine connector content for a cell where the commit does NOT touch the cluster.
///
/// If there are touching commits both above and below this row in the same
/// column, draw a vertical connector line colored by the relationship that
/// the lower square has with an earlier commit.
fn fragmap_connector_content(
    fragmap: &fragmap::FragMap,
    commit_idx: usize,
    cluster_idx: usize,
    scope: SquashableScope,
    role: ConnectorRole,
    theme: &dyn FragmapTheme,
) -> Option<(String, Style)> {
    let has_above = (0..commit_idx)
        .rev()
        .any(|i| fragmap.matrix[i][cluster_idx] != TouchKind::None);

    let below = ((commit_idx + 1)..fragmap.commits.len())
        .find(|&i| fragmap.matrix[i][cluster_idx] != TouchKind::None);

    match (has_above, below) {
        (true, Some(below_idx)) => {
            match fragmap.connector_squashable(below_idx, cluster_idx, scope) {
                Some(true) => Some((
                    theme
                        .connector_symbol(role, ConnectorRelation::Squashable)
                        .to_owned(),
                    theme.connector_style(role, ConnectorRelation::Squashable),
                )),
                Some(false) => Some((
                    theme
                        .connector_symbol(role, ConnectorRelation::Conflict)
                        .to_owned(),
                    theme.connector_style(role, ConnectorRelation::Conflict),
                )),
                None => None,
            }
        }
        _ => None,
    }
}

/// Compute the text style for a commit row based on its fragmap relationship
/// to the currently selected commit.
///
/// Yellow: squash partner — either the selected commit can squash into this
/// commit, or this commit can squash into the selected commit.
/// Red: shares a cluster but not a squash partner.
/// DarkGray: this commit is itself fully squashable (intrinsic property).
pub fn commit_text_style(
    fragmap: &fragmap::FragMap,
    selection_idx: usize,
    commit_idx: usize,
    theme: &dyn FragmapTheme,
) -> Style {
    let is_squash_partner = fragmap
        .squash_target(selection_idx)
        .is_some_and(|t| t == commit_idx)
        || fragmap
            .squash_target(commit_idx)
            .is_some_and(|t| t == selection_idx);

    let role = if is_squash_partner {
        CommitRowRole::SquashPartner
    } else if fragmap.shares_cluster_with(selection_idx, commit_idx) {
        CommitRowRole::Conflict
    } else if fragmap.is_fully_squashable(commit_idx) {
        CommitRowRole::Squashable
    } else {
        CommitRowRole::Unrelated
    };
    theme.commit_row_style(role)
}

/// Build a single fragmap cell from the visible cluster columns.
///
/// `focus_idx` is the index of the focus commit (selected commit or action
/// source) used to compute `SquareRole` and `ConnectorRole` per cluster for
/// themes that distinguish focus-related columns from unrelated ones.
///
/// When `is_selected` is true, adds `COLOR_SELECTED_FRAGMAP_BG` as the
/// background of every span so the row is visually highlighted without
/// inverting the foreground colors of the symbols.
pub fn build_fragmap_cell<'a>(
    fragmap: &fragmap::FragMap,
    commit_idx: usize,
    focus_idx: usize,
    display_clusters: &[usize],
    is_selected: bool,
    scope: SquashableScope,
    theme: &dyn FragmapTheme,
) -> Cell<'a> {
    let spans: Vec<Span> = display_clusters
        .iter()
        .map(|&cluster_idx| {
            let focus_touches = fragmap.matrix[focus_idx][cluster_idx] != TouchKind::None;
            let square_role = if commit_idx == focus_idx {
                SquareRole::Current
            } else if focus_touches {
                SquareRole::Related
            } else {
                SquareRole::Unrelated
            };
            let connector_role = if focus_touches {
                ConnectorRole::Related
            } else {
                ConnectorRole::Unrelated
            };

            let base_style = if is_selected {
                Style::new().bg(COLOR_SELECTED_FRAGMAP_BG)
            } else {
                Style::new()
            };
            if let Some((symbol, style)) =
                fragmap_cell_content(fragmap, commit_idx, cluster_idx, scope, square_role, theme)
            {
                Span::styled(symbol, base_style.patch(style))
            } else if let Some((symbol, style)) = fragmap_connector_content(
                fragmap,
                commit_idx,
                cluster_idx,
                scope,
                connector_role,
                theme,
            ) {
                Span::styled(symbol, base_style.patch(style))
            } else {
                Span::styled(" ", base_style)
            }
        })
        .collect();
    let cell = Cell::from(Line::from(spans));
    if is_selected {
        cell.style(Style::new().bg(COLOR_SELECTED_FRAGMAP_BG))
    } else {
        cell
    }
}

/// Render the horizontal scrollbar for the fragmap columns.
pub fn render_horizontal_scrollbar(
    frame: &mut Frame,
    hs_area: Rect,
    title_width: u16,
    fragmap_col_width: u16,
    total_clusters: usize,
    fragmap_available_width: usize,
    h_scroll_offset: usize,
) {
    let fragmap_x = hs_area.x + 10 + 1 + title_width + 1;
    let area = Rect {
        x: fragmap_x,
        width: fragmap_col_width,
        ..hs_area
    };

    let mut state = ScrollbarState::new(total_clusters.saturating_sub(fragmap_available_width))
        .position(h_scroll_offset);

    let scrollbar = Scrollbar::new(ScrollbarOrientation::HorizontalBottom)
        .begin_symbol(None)
        .end_symbol(None)
        .track_symbol(Some("─"));

    frame.render_stateful_widget(scrollbar, area, &mut state);
}
