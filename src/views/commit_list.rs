// Commit list view rendering

use crate::app::AppState;
use crate::fragmap::{self, TouchKind};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Cell, Paragraph, Row, Scrollbar, ScrollbarOrientation, ScrollbarState, Table},
    Frame,
};

/// Number of characters to display for short SHA.
const SHORT_SHA_LENGTH: usize = 8;

const HEADER_STYLE: Style = Style::new().fg(Color::White).bg(Color::Green);
const FOOTER_STYLE: Style = Style::new().fg(Color::White).bg(Color::Blue);
const SEPARATOR_STYLE: Style = Style::new().fg(Color::White).bg(Color::Blue);

// Fragmap visualization symbols
const CLUSTER_TOUCHED_CONFLICTING: &str = "█";
const CLUSTER_TOUCHED_SQUASHABLE: &str = "█";
const CLUSTER_CONNECTOR_CONFLICTING: &str = "│";
const CLUSTER_CONNECTOR_SQUASHABLE: &str = "│";

// Connector colors
const COLOR_CONFLICTING: Color = Color::Red;
const COLOR_SQUASHABLE: Color = Color::Yellow;

// Cell colors
const COLOR_TOUCHED_CONFLICTING: Color = Color::White;
const COLOR_TOUCHED_SQUASHABLE: Color = Color::DarkGray;

// Background applied to the fragmap matrix columns of the selected row.
// Kept separate from the text-cell highlight (which uses terminal Reversed)
// so that the vertical lines and filled squares keep their foreground colors.
const COLOR_SELECTED_FRAGMAP_BG: Color = Color::Rgb(60, 60, 80);

// Foreground color for synthetic working-tree rows (staged / unstaged).
// Applied when the row is not selected so they are visually distinct from commits.
const COLOR_SYNTHETIC_LABEL: Color = Color::Cyan;

/// Maximum width for the title column, keeping fragmap adjacent to titles.
const MAX_TITLE_WIDTH: u16 = 60;

/// Pre-computed layout information shared between rendering functions.
struct LayoutInfo {
    table_area: Rect,
    footer_area: Rect,
    h_scrollbar_area: Option<Rect>,
    available_height: usize,
    has_v_scrollbar: bool,
    visible_clusters: Vec<usize>,
    display_clusters: Vec<usize>,
    fragmap_col_width: u16,
    title_width: u16,
    fragmap_available_width: usize,
    h_scroll_offset: usize,
    visual_selection: usize,
    scroll_offset: usize,
}
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
    for earlier_idx in 0..commit_idx {
        if fragmap.matrix[earlier_idx][cluster_idx] != TouchKind::None {
            return Some(fragmap.cluster_relation(earlier_idx, commit_idx, cluster_idx));
        }
    }
    None
}

/// Determine cell content and style for a commit-cluster intersection.
///
/// Returns None if the commit doesn't touch the cluster.
fn fragmap_cell_content(
    fragmap: &fragmap::FragMap,
    commit_idx: usize,
    cluster_idx: usize,
) -> Option<(&'static str, Style)> {
    if fragmap.matrix[commit_idx][cluster_idx] == TouchKind::None {
        return None;
    }

    match cluster_relation(fragmap, commit_idx, cluster_idx) {
        Some(fragmap::SquashRelation::Squashable) => Some((
            CLUSTER_TOUCHED_SQUASHABLE,
            Style::new().fg(COLOR_TOUCHED_SQUASHABLE),
        )),
        Some(fragmap::SquashRelation::Conflicting) => Some((
            CLUSTER_TOUCHED_CONFLICTING,
            Style::new().fg(COLOR_TOUCHED_CONFLICTING),
        )),
        _ => Some((
            CLUSTER_TOUCHED_CONFLICTING,
            Style::new().fg(COLOR_TOUCHED_CONFLICTING),
        )),
    }
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
) -> Option<(&'static str, Style)> {
    let has_above = (0..commit_idx)
        .rev()
        .any(|i| fragmap.matrix[i][cluster_idx] != TouchKind::None);

    let below = ((commit_idx + 1)..fragmap.commits.len())
        .find(|&i| fragmap.matrix[i][cluster_idx] != TouchKind::None);

    match (has_above, below) {
        (true, Some(below_idx)) => {
            // Color connector by the lower square's relationship
            match cluster_relation(fragmap, below_idx, cluster_idx) {
                Some(fragmap::SquashRelation::Conflicting) => Some((
                    CLUSTER_CONNECTOR_CONFLICTING,
                    Style::new().fg(COLOR_CONFLICTING),
                )),
                Some(fragmap::SquashRelation::Squashable) => Some((
                    CLUSTER_CONNECTOR_SQUASHABLE,
                    Style::new().fg(COLOR_SQUASHABLE),
                )),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Render the commit list view.
///
/// Takes application state and renders the commit list to the terminal frame.
/// If `area` is provided, uses that instead of the full frame area.
pub fn render(app: &mut AppState, frame: &mut Frame) {
    render_in_area(app, frame, frame.area());
}

/// Render the commit list view in a specific area.
pub fn render_in_area(app: &mut AppState, frame: &mut Frame, area: Rect) {
    let layout = compute_layout(app, area);

    // Store visible height for page scrolling
    app.commit_list_visible_height = layout.available_height;

    let header = build_header(&layout);
    let rows = build_rows(app, &layout);

    let constraints = build_constraints(&layout);

    let (scrollbar_area, content_area) = if layout.has_v_scrollbar {
        let [sb, content] = Layout::horizontal([Constraint::Length(1), Constraint::Min(0)])
            .areas(layout.table_area);
        (Some(sb), content)
    } else {
        (None, layout.table_area)
    };

    let table = Table::new(rows, constraints).header(header);
    frame.render_widget(table, content_area);

    if layout.fragmap_col_width > 0 {
        let sep_x = content_area.x + 10 + 1 + layout.title_width;
        let sep_height = if layout.h_scrollbar_area.is_some() {
            content_area.height + 1
        } else {
            content_area.height
        };
        let sep_area = Rect {
            x: sep_x,
            y: content_area.y,
            width: 1,
            height: sep_height,
        };
        let sep_lines: Vec<Line> = (0..sep_height)
            .map(|_| Line::from(Span::styled("│", SEPARATOR_STYLE)))
            .collect();
        frame.render_widget(Paragraph::new(sep_lines), sep_area);
    }

    if let Some(sb_area) = scrollbar_area {
        render_vertical_scrollbar(frame, sb_area, &layout, app.commits.len());
    }

    render_footer(frame, app, layout.footer_area);

    if let Some(hs_area) = layout.h_scrollbar_area {
        render_horizontal_scrollbar(frame, hs_area, content_area, &layout);
    }
}

/// Compute all layout dimensions, scroll offsets, and visible cluster indices.
fn compute_layout(app: &mut AppState, frame_area: Rect) -> LayoutInfo {
    let visible_cluster_count = if let Some(ref fragmap) = app.fragmap {
        (0..fragmap.clusters.len())
            .filter(|&ci| fragmap.matrix.iter().any(|row| row[ci] != TouchKind::None))
            .count()
    } else {
        0
    };

    let preliminary_fragmap_width = frame_area.width.saturating_sub(10 + 1 + 20 + 1 + 1) as usize;
    let needs_h_scrollbar =
        visible_cluster_count > 0 && visible_cluster_count > preliminary_fragmap_width;

    let (table_area, h_scrollbar_area, footer_area) = if needs_h_scrollbar {
        let [t, hs, f] = Layout::vertical([
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(frame_area);
        (t, Some(hs), f)
    } else {
        let [t, f] =
            Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(frame_area);
        (t, None, f)
    };

    let available_height = table_area.height.saturating_sub(1) as usize;
    let has_v_scrollbar = !app.commits.is_empty() && app.commits.len() > available_height;
    let effective_width = if has_v_scrollbar {
        table_area.width.saturating_sub(1)
    } else {
        table_area.width
    };

    let visible_clusters: Vec<usize> = if let Some(ref fragmap) = app.fragmap {
        (0..fragmap.clusters.len())
            .filter(|&ci| fragmap.matrix.iter().any(|row| row[ci] != TouchKind::None))
            .collect()
    } else {
        vec![]
    };

    let fragmap_available_width = effective_width.saturating_sub(10 + 1 + 20 + 1) as usize;
    let h_scroll_offset = app.fragmap_scroll_offset.min(
        visible_clusters
            .len()
            .saturating_sub(fragmap_available_width.max(1)),
    );
    app.fragmap_scroll_offset = h_scroll_offset;

    let display_clusters: Vec<usize> = if visible_clusters.is_empty() {
        vec![]
    } else {
        let end = (h_scroll_offset + fragmap_available_width).min(visible_clusters.len());
        visible_clusters[h_scroll_offset..end].to_vec()
    };

    let fragmap_col_width = display_clusters.len() as u16;
    let title_width = if fragmap_col_width > 0 {
        effective_width
            .saturating_sub(10 + 2 + fragmap_col_width)
            .min(MAX_TITLE_WIDTH)
    } else {
        0
    };

    let visual_selection = if app.reverse {
        app.commits
            .len()
            .saturating_sub(1)
            .saturating_sub(app.selection_index)
    } else {
        app.selection_index
    };

    let scroll_offset =
        if app.commits.is_empty() || available_height == 0 || visual_selection < available_height {
            0
        } else {
            visual_selection.saturating_sub(available_height - 1)
        };

    LayoutInfo {
        table_area,
        footer_area,
        h_scrollbar_area,
        available_height,
        has_v_scrollbar,
        visible_clusters,
        display_clusters,
        fragmap_col_width,
        title_width,
        fragmap_available_width,
        h_scroll_offset,
        visual_selection,
        scroll_offset,
    }
}

fn build_header(layout: &LayoutInfo) -> Row<'static> {
    let cells = if layout.fragmap_col_width > 0 {
        vec![
            Cell::from("SHA"),
            Cell::from("Title"),
            Cell::from("Hunk groups"),
        ]
    } else {
        vec![Cell::from("SHA"), Cell::from("Title")]
    };
    Row::new(cells).style(HEADER_STYLE)
}

fn build_constraints(layout: &LayoutInfo) -> Vec<Constraint> {
    if layout.fragmap_col_width > 0 {
        vec![
            Constraint::Length(10),
            Constraint::Length(layout.title_width),
            Constraint::Length(layout.fragmap_col_width),
        ]
    } else {
        vec![Constraint::Length(10), Constraint::Min(20)]
    }
}

/// Determine the text style for a non-selected commit row.
///
/// Yellow: squash partner — either the selected commit can squash into this
/// commit, or this commit can squash into the selected commit.
/// Red: shares a cluster but not a squash partner.
/// DarkGray: this commit is itself fully squashable (intrinsic property).
fn commit_text_style(fragmap: &fragmap::FragMap, selection_idx: usize, commit_idx: usize) -> Style {
    let is_squash_partner = fragmap
        .squash_target(selection_idx)
        .is_some_and(|t| t == commit_idx)
        || fragmap
            .squash_target(commit_idx)
            .is_some_and(|t| t == selection_idx);

    if is_squash_partner {
        Style::new().fg(COLOR_SQUASHABLE)
    } else if fragmap.shares_cluster_with(selection_idx, commit_idx) {
        Style::new().fg(COLOR_CONFLICTING)
    } else if fragmap.is_fully_squashable(commit_idx) {
        Style::new().fg(COLOR_TOUCHED_SQUASHABLE)
    } else {
        Style::default()
    }
}

/// Build a single fragmap cell from the visible cluster columns.
///
/// When `is_selected` is true, adds `COLOR_SELECTED_FRAGMAP_BG` as the
/// background of every span so the row is visually highlighted without
/// inverting the foreground colors of the symbols.
fn build_fragmap_cell<'a>(
    fragmap: &fragmap::FragMap,
    commit_idx: usize,
    display_clusters: &[usize],
    is_selected: bool,
) -> Cell<'a> {
    let spans: Vec<Span> = display_clusters
        .iter()
        .map(|&cluster_idx| {
            let base_style = if is_selected {
                Style::new().bg(COLOR_SELECTED_FRAGMAP_BG)
            } else {
                Style::new()
            };
            if let Some((symbol, style)) = fragmap_cell_content(fragmap, commit_idx, cluster_idx) {
                Span::styled(symbol, base_style.patch(style))
            } else if let Some((symbol, style)) =
                fragmap_connector_content(fragmap, commit_idx, cluster_idx)
            {
                Span::styled(symbol, base_style.patch(style))
            } else {
                Span::styled(" ", base_style)
            }
        })
        .collect();
    Cell::from(Line::from(spans))
}

/// Build all visible table rows.
fn build_rows<'a>(app: &AppState, layout: &LayoutInfo) -> Vec<Row<'a>> {
    let display_commits: Vec<&crate::CommitInfo> = if app.reverse {
        app.commits.iter().rev().collect()
    } else {
        app.commits.iter().collect()
    };

    let visible_commits = if display_commits.is_empty() {
        &display_commits[..]
    } else {
        let end = (layout.scroll_offset + layout.available_height).min(display_commits.len());
        &display_commits[layout.scroll_offset..end]
    };

    visible_commits
        .iter()
        .enumerate()
        .map(|(visible_index, commit)| {
            let visual_index = layout.scroll_offset + visible_index;

            let commit_idx_in_fragmap = if app.reverse {
                app.commits
                    .len()
                    .saturating_sub(1)
                    .saturating_sub(visual_index)
            } else {
                visual_index
            };

            let short_sha: String = commit.oid.chars().take(SHORT_SHA_LENGTH).collect();

            // Synthetic working-tree rows (staged/unstaged) use a fixed label
            // color rather than the commit-relationship coloring.
            let is_synthetic = commit.oid == "staged" || commit.oid == "unstaged";

            let text_style = if visual_index != layout.visual_selection {
                if is_synthetic {
                    Style::new().fg(COLOR_SYNTHETIC_LABEL)
                } else if let Some(ref fm) = app.fragmap {
                    commit_text_style(fm, app.selection_index, commit_idx_in_fragmap)
                } else {
                    Style::default()
                }
            } else {
                Style::default()
            };

            let is_selected = visual_index == layout.visual_selection;
            let text_cell_style = if is_selected {
                text_style.reversed()
            } else {
                text_style
            };

            let mut cells = vec![
                Cell::from(Span::styled(short_sha, text_cell_style)),
                Cell::from(Span::styled(commit.summary.clone(), text_cell_style)),
            ];

            if let Some(ref fragmap) = app.fragmap {
                if !layout.display_clusters.is_empty() {
                    cells.push(build_fragmap_cell(
                        fragmap,
                        commit_idx_in_fragmap,
                        &layout.display_clusters,
                        is_selected,
                    ));
                }
            }

            Row::new(cells)
        })
        .collect()
}

fn render_footer(frame: &mut Frame, app: &AppState, area: Rect) {
    let text = if app.commits.is_empty() {
        String::from("No commits")
    } else {
        let commit = &app.commits[app.selection_index];
        let position = app.commits.len() - app.selection_index;
        format!(" {} {}/{}", commit.oid, position, app.commits.len())
    };

    let footer = Paragraph::new(Span::styled(text, FOOTER_STYLE)).style(FOOTER_STYLE);
    frame.render_widget(footer, area);
}

fn render_vertical_scrollbar(
    frame: &mut Frame,
    sb_area: Rect,
    layout: &LayoutInfo,
    commit_count: usize,
) {
    let mut state =
        ScrollbarState::new(commit_count.saturating_sub(1)).position(layout.visual_selection);

    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalLeft)
        .begin_symbol(None)
        .end_symbol(None)
        .track_symbol(Some("│"));

    let data_area = Rect {
        y: sb_area.y + 1,
        height: layout.available_height as u16,
        ..sb_area
    };
    frame.render_stateful_widget(scrollbar, data_area, &mut state);
}

fn render_horizontal_scrollbar(
    frame: &mut Frame,
    hs_area: Rect,
    content_area: Rect,
    layout: &LayoutInfo,
) {
    let fragmap_x = content_area.x + 10 + 1 + layout.title_width + 1;
    let area = Rect {
        x: fragmap_x,
        width: layout.fragmap_col_width,
        ..hs_area
    };

    let mut state = ScrollbarState::new(
        layout
            .visible_clusters
            .len()
            .saturating_sub(layout.fragmap_available_width),
    )
    .position(layout.h_scroll_offset);

    let scrollbar = Scrollbar::new(ScrollbarOrientation::HorizontalBottom)
        .begin_symbol(None)
        .end_symbol(None)
        .track_symbol(Some("─"));

    frame.render_stateful_widget(scrollbar, area, &mut state);
}
