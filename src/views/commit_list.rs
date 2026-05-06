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

// Commit list view rendering

use super::hunk_groups;
use crate::app::{AppAction, AppMode, AppState, KeyCommand};
use crate::fragmap::TouchKind;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Cell, Paragraph, Row, Scrollbar, ScrollbarOrientation, ScrollbarState, Table},
};

/// Handle an action while in CommitList mode.
pub fn handle_key(action: KeyCommand, app: &mut AppState) -> AppAction {
    match action {
        KeyCommand::MoveUp => {
            if app.reverse {
                app.move_down();
            } else {
                app.move_up();
            }
            AppAction::Handled
        }
        KeyCommand::MoveDown => {
            if app.reverse {
                app.move_up();
            } else {
                app.move_down();
            }
            AppAction::Handled
        }
        KeyCommand::PageUp => {
            let h = app.commit_list_visible_height;
            if app.reverse {
                app.page_down(h);
            } else {
                app.page_up(h);
            }
            AppAction::Handled
        }
        KeyCommand::PageDown => {
            let h = app.commit_list_visible_height;
            if app.reverse {
                app.page_up(h);
            } else {
                app.page_down(h);
            }
            AppAction::Handled
        }
        KeyCommand::ScrollLeft => {
            app.scroll_fragmap_left();
            AppAction::Handled
        }
        KeyCommand::ScrollRight => {
            app.scroll_fragmap_right();
            AppAction::Handled
        }
        KeyCommand::ToggleDetail | KeyCommand::Confirm => {
            app.toggle_detail_view();
            AppAction::Handled
        }
        KeyCommand::ShowHelp => {
            app.toggle_help();
            AppAction::Handled
        }
        KeyCommand::Split => {
            app.enter_split_select();
            AppAction::Handled
        }
        KeyCommand::Squash => {
            app.enter_squash_select();
            AppAction::Handled
        }
        KeyCommand::Fixup => {
            app.enter_fixup_select();
            AppAction::Handled
        }
        KeyCommand::Drop => {
            let commit = &app.commits[app.selection_index];
            if commit.oid.is_synthetic() {
                app.set_error_message("Cannot drop staged/unstaged changes");
                AppAction::Handled
            } else {
                AppAction::PrepareDropConfirm {
                    commit_oid: commit.oid.expect_real_oid(),
                    commit_summary: commit.summary.clone(),
                }
            }
        }
        KeyCommand::Reword => {
            let commit = &app.commits[app.selection_index];
            if commit.oid.is_synthetic() {
                app.set_error_message("Cannot reword staged/unstaged changes");
                AppAction::Handled
            } else {
                AppAction::PrepareReword {
                    commit_oid: commit.oid.expect_real_oid(),
                    current_message: commit.message.clone(),
                }
            }
        }
        KeyCommand::Update => AppAction::ReloadCommits,
        KeyCommand::Quit => AppAction::Quit,
        KeyCommand::Move => {
            app.enter_move_select();
            AppAction::Handled
        }
        KeyCommand::Mergetool
        | KeyCommand::OpenEditor
        | KeyCommand::None
        | KeyCommand::ForceQuit
        | KeyCommand::Suspend
        | KeyCommand::Search
        | KeyCommand::SearchNext
        | KeyCommand::SearchPrev => AppAction::Handled,
        KeyCommand::SeparatorLeft => {
            app.separator_offset = app.separator_offset.saturating_sub(4);
            AppAction::Handled
        }
        KeyCommand::SeparatorRight => {
            app.separator_offset = app.separator_offset.saturating_add(4);
            AppAction::Handled
        }
    }
}

const HEADER_STYLE: Style = Style::new().fg(Color::White).bg(Color::Green);
const FOOTER_STYLE: Style = Style::new().fg(Color::White).bg(Color::Blue);
const SEPARATOR_STYLE: Style = Style::new().fg(Color::White).bg(Color::Blue);

// Foreground color for synthetic working-tree rows (staged / unstaged).
// Applied when the row is not selected so they are visually distinct from commits.
const COLOR_SYNTHETIC_LABEL: Color = Color::Cyan;

// Colors shared across action modes (squash, move, …) for source, target, and insert-point rows.
const COLOR_ACTION_SOURCE_BG: Color = Color::Rgb(0, 120, 120);
const COLOR_ACTION_TARGET_BG: Color = Color::Rgb(0, 40, 50);
const COLOR_ACTION_INSERT_BG: Color = Color::Rgb(40, 40, 100);

/// Width of the SHA column in the commit table.
const SHA_COL_WIDTH: u16 = 10;

/// Minimum title column width reserved when computing natural fragmap space.
const MIN_TITLE_WIDTH: u16 = 20;

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
/// Render the commit list view.
///
/// Takes application state and renders the commit list to the terminal frame.
/// If `area` is provided, uses that instead of the full frame area.
pub fn render(app: &mut AppState, frame: &mut Frame) {
    render_in_area(app, frame, frame.area());
}

/// Render the commit list view in a specific area, without showing fragmap columns.
/// The fragmap is still used for row coloring; only the column display is suppressed.
/// Used by the detail-panel split view where fragmap columns would be out of place.
pub fn render_in_area_without_fragmap_cols(app: &mut AppState, frame: &mut Frame, area: Rect) {
    // Hide fragmap only for layout (column sizing) so no fragmap columns appear;
    // restore it before build_rows so commit_text_style still gets fragmap colors.
    let saved_fragmap = app.fragmap.take();
    let layout = compute_layout(app, area);
    app.fragmap = saved_fragmap;
    render_in_area_with_layout(app, frame, layout);
}

/// Returns the x-coordinate of the fragmap "│" separator as it would be
/// rendered when the commit list occupies `area`, or `None` when no fragmap
/// clusters are visible (no data to split on).
///
/// As a side effect, updates `app.separator_offset` to the clamped value for
/// this area — the same value that `render_in_area` would produce.
pub fn compute_fragmap_sep_x(app: &mut AppState, area: Rect) -> Option<u16> {
    app.fragmap.as_ref()?;
    let layout = compute_layout(app, area);
    if layout.fragmap_col_width == 0 {
        return None;
    }
    let content_x = if layout.has_v_scrollbar {
        area.x + 1
    } else {
        area.x
    };
    Some(content_x + SHA_COL_WIDTH + 1 + layout.title_width)
}

/// Render the commit list view in a specific area.
pub fn render_in_area(app: &mut AppState, frame: &mut Frame, area: Rect) {
    let layout = compute_layout(app, area);
    render_in_area_with_layout(app, frame, layout);
}

fn render_in_area_with_layout(app: &mut AppState, frame: &mut Frame, layout: LayoutInfo) {
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
        let sep_x = content_area.x + SHA_COL_WIDTH + 1 + layout.title_width;
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
        hunk_groups::render_horizontal_scrollbar(
            frame,
            hs_area,
            layout.title_width,
            layout.fragmap_col_width,
            layout.visible_clusters.len(),
            layout.fragmap_available_width,
            layout.h_scroll_offset,
        );
    }
}

/// Compute all layout dimensions, scroll offsets, and visible cluster indices.
fn compute_layout(app: &mut AppState, frame_area: Rect) -> LayoutInfo {
    let visible_clusters: Vec<usize> = if let Some(ref fragmap) = app.fragmap {
        (0..fragmap.clusters.len())
            .filter(|&ci| fragmap.matrix.iter().any(|row| row[ci] != TouchKind::None))
            .collect()
    } else {
        vec![]
    };

    let preliminary_fragmap_width = frame_area
        .width
        .saturating_sub(SHA_COL_WIDTH + 1 + MIN_TITLE_WIDTH + 1 + 1)
        as usize;
    let needs_h_scrollbar =
        !visible_clusters.is_empty() && visible_clusters.len() > preliminary_fragmap_width;

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

    // Establish the natural (separator_offset=0) baseline using the same formula as
    // before T117: title is whatever fits after allocating maximum fragmap space.
    let natural_fragmap_w =
        effective_width.saturating_sub(SHA_COL_WIDTH + 2 + MIN_TITLE_WIDTH) as usize;
    let natural_h_scroll = app.fragmap_scroll_offset.min(
        visible_clusters
            .len()
            .saturating_sub(natural_fragmap_w.max(1)),
    );
    let natural_frag_col_w: u16 = if visible_clusters.is_empty() {
        0
    } else {
        let end = (natural_h_scroll + natural_fragmap_w).min(visible_clusters.len());
        end.saturating_sub(natural_h_scroll) as u16
    };
    let natural_title = effective_width
        .saturating_sub(SHA_COL_WIDTH + 2 + natural_frag_col_w)
        .min(MAX_TITLE_WIDTH);

    // Apply separator_offset on top of the natural baseline. Clamp and write back
    // immediately so reversing direction takes effect without delay.
    let max_title = effective_width.saturating_sub(SHA_COL_WIDTH + 2 + 1) as i32;
    let min_title: i32 = 10;
    let title_width: u16 = if max_title >= min_title {
        let w = (natural_title as i32 + app.separator_offset as i32).clamp(min_title, max_title);
        app.separator_offset = (w - natural_title as i32) as i16;
        w as u16
    } else {
        app.separator_offset = 0;
        0
    };

    // Derive fragmap available width from the final title_width so that the
    // table constraints (SHA + title + fragmap) always sum to effective_width.
    let fragmap_available_width =
        effective_width.saturating_sub(SHA_COL_WIDTH + 2 + title_width) as usize;

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

    let visual_selection = fragmap_index(app, app.selection_index);

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
            Constraint::Length(SHA_COL_WIDTH),
            Constraint::Length(layout.title_width),
            Constraint::Min(layout.fragmap_col_width),
        ]
    } else {
        // Min(0) for title: SHA's Length(10) always wins over the title column
        // so the SHA is never squished when the panel is narrow (e.g. the left
        // sub-panel in CommitDetail view).
        vec![Constraint::Length(SHA_COL_WIDTH), Constraint::Min(0)]
    }
}

/// Convert a visual row index (0 = top of list) to the fragmap matrix index.
/// When the list is in reverse order the index is mirrored.
fn fragmap_index(app: &AppState, visual_idx: usize) -> usize {
    if app.reverse {
        app.commits
            .len()
            .saturating_sub(1)
            .saturating_sub(visual_idx)
    } else {
        visual_idx
    }
}

/// Describes which action mode is active, carrying the source commit index.
/// Used by [`row_text_style`] to pick the foreground style for each row.
enum FocusContext {
    Squash { source_idx: usize },
    Move { source_idx: usize },
    Normal,
}

/// Compute the foreground/text style for a single commit row.
fn row_text_style(
    app: &AppState,
    focus_ctx: &FocusContext,
    commit_idx: usize,
    is_selected: bool,
    is_synthetic: bool,
) -> Style {
    match focus_ctx {
        FocusContext::Squash { source_idx } => {
            if commit_idx == *source_idx {
                Style::new().fg(Color::White)
            } else if commit_idx > *source_idx {
                Style::new().fg(Color::DarkGray)
            } else if is_synthetic {
                Style::new().fg(COLOR_SYNTHETIC_LABEL)
            } else if let Some(ref fm) = app.fragmap {
                hunk_groups::commit_text_style(fm, *source_idx, commit_idx, app.theme.as_theme())
            } else {
                Style::default()
            }
        }
        FocusContext::Move { source_idx } => {
            if commit_idx == *source_idx {
                Style::new().fg(Color::DarkGray)
            } else if is_synthetic {
                Style::new().fg(COLOR_SYNTHETIC_LABEL)
            } else if let Some(ref fm) = app.fragmap {
                hunk_groups::commit_text_style(fm, *source_idx, commit_idx, app.theme.as_theme())
            } else {
                Style::default()
            }
        }
        FocusContext::Normal => {
            if is_selected {
                Style::default()
            } else if is_synthetic {
                Style::new().fg(COLOR_SYNTHETIC_LABEL)
            } else if let Some(ref fm) = app.fragmap {
                hunk_groups::commit_text_style(
                    fm,
                    app.selection_index,
                    commit_idx,
                    app.theme.as_theme(),
                )
            } else {
                Style::default()
            }
        }
    }
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
        // When a move separator row is visible it occupies one table row,
        // so we show one fewer commit to keep everything within bounds.
        let separator_visible = match app.mode {
            AppMode::MoveSelect {
                source_index,
                insert_before,
            } if insert_before != source_index && insert_before != source_index + 1 => {
                let vis_start = layout.scroll_offset;
                let vis_end = (vis_start + layout.available_height).min(display_commits.len());
                insert_before >= vis_start && insert_before <= vis_end
            }
            _ => false,
        };
        let height = if separator_visible {
            layout.available_height.saturating_sub(1)
        } else {
            layout.available_height
        };
        let end = (layout.scroll_offset + height).min(display_commits.len());
        &display_commits[layout.scroll_offset..end]
    };

    // In SquashSelect mode, the source commit index drives candidate coloring.
    let squash_source_idx = match app.mode {
        AppMode::SquashSelect { source_index, .. } => Some(source_index),
        _ => None,
    };

    // In MoveSelect mode, extract source and insertion point.
    let move_info = match app.mode {
        AppMode::MoveSelect {
            source_index,
            insert_before,
        } => Some((source_index, insert_before)),
        _ => None,
    };

    let focus_ctx = match (squash_source_idx, move_info) {
        (Some(source_idx), _) => FocusContext::Squash { source_idx },
        (_, Some((source_idx, _))) => FocusContext::Move { source_idx },
        _ => FocusContext::Normal,
    };

    // Focus fragmap index for cluster-column highlight classification.
    let focus_source = squash_source_idx
        .or_else(|| move_info.map(|(si, _)| si))
        .unwrap_or(app.selection_index);
    let focus_idx_in_fragmap = fragmap_index(app, focus_source);

    let mut rows: Vec<Row<'a>> = Vec::new();

    for (visible_index, commit) in visible_commits.iter().enumerate() {
        let visual_index = layout.scroll_offset + visible_index;
        let commit_idx_in_fragmap = fragmap_index(app, visual_index);

        // Insert separator row before this commit if this is the insertion point.
        if let Some((source_index, insert_before)) = move_info
            && commit_idx_in_fragmap == insert_before
            && insert_before != source_index
            && insert_before != source_index + 1
        {
            rows.push(build_move_separator_row(app, layout, source_index));
        }

        let short_sha = commit.oid.short().to_string();

        let is_synthetic = commit.oid.is_synthetic();
        let is_selected = visual_index == layout.visual_selection;
        let is_squash_source = squash_source_idx.is_some_and(|si| commit_idx_in_fragmap == si);
        let is_move_source = move_info.is_some_and(|(si, _)| commit_idx_in_fragmap == si);

        let text_style = row_text_style(
            app,
            &focus_ctx,
            commit_idx_in_fragmap,
            is_selected,
            is_synthetic,
        );

        // Apply highlight: source gets teal bg, selection in an action mode gets
        // a subtle target-bg tint plus reversed; plain selection gets reversed.
        let text_cell_style = if is_squash_source || is_move_source {
            text_style.fg(Color::White).bg(COLOR_ACTION_SOURCE_BG)
        } else if is_selected && squash_source_idx.is_some() {
            text_style.bg(COLOR_ACTION_TARGET_BG).reversed()
        } else if is_selected && move_info.is_none() {
            text_style.reversed()
        } else {
            text_style
        };

        let mut cells = vec![
            Cell::from(Span::styled(short_sha, text_cell_style)),
            Cell::from(Span::styled(commit.summary.clone(), text_cell_style)),
        ];

        if let Some(ref fragmap) = app.fragmap
            && !layout.display_clusters.is_empty()
        {
            cells.push(hunk_groups::build_fragmap_cell(
                fragmap,
                commit_idx_in_fragmap,
                focus_idx_in_fragmap,
                &layout.display_clusters,
                is_selected,
                app.squashable_scope,
                app.theme.as_theme(),
            ));
        }

        rows.push(Row::new(cells));
    }

    // Append separator after the last visible commit when moving to the end.
    if let Some((source_index, insert_before)) = move_info {
        let last_visible_idx = layout.scroll_offset + visible_commits.len();
        if insert_before >= last_visible_idx
            && insert_before == app.commits.len()
            && insert_before != source_index
            && insert_before != source_index + 1
        {
            rows.push(build_move_separator_row(app, layout, source_index));
        }
    }

    rows
}

/// Build the styled "▶ move here" separator row for MoveSelect mode.
fn build_move_separator_row<'a>(
    app: &AppState,
    layout: &LayoutInfo,
    source_index: usize,
) -> Row<'a> {
    let source = app.commits.get(source_index);
    let short_oid = source.map(|c| c.oid.short()).unwrap_or("?");

    let style = Style::new().fg(Color::White).bg(COLOR_ACTION_INSERT_BG);
    let label = format!("▶ move {} here", short_oid);

    let mut cells = vec![
        Cell::from(Span::styled("", style)),
        Cell::from(Span::styled(label, style)),
    ];

    if !layout.display_clusters.is_empty() {
        cells.push(Cell::from(Span::styled("", style)));
    }

    Row::new(cells)
}

pub(crate) fn render_footer(frame: &mut Frame, app: &AppState, area: Rect) {
    if let Some(msg) = &app.status_message {
        let bg = if app.status_is_error {
            Color::Red
        } else {
            Color::Green
        };
        let style = Style::new().fg(Color::White).bg(bg);
        let footer = Paragraph::new(Span::styled(format!(" {}", msg), style)).style(style);
        frame.render_widget(footer, area);
        return;
    }

    if let AppMode::SquashSelect {
        source_index,
        is_fixup,
    } = app.mode
    {
        render_squash_footer(frame, app, area, source_index, is_fixup);
        return;
    }

    if let AppMode::MoveSelect {
        source_index,
        insert_before: _,
    } = app.mode
    {
        render_move_footer(frame, app, area, source_index);
        return;
    }

    const HINT: &str = "Press 'h' for help";
    let left = if app.commits.is_empty() {
        String::from("No commits")
    } else {
        let commit = &app.commits[app.selection_index];
        let position = app.commits.len() - app.selection_index;
        format!(" {} {}/{}", commit.oid.long(), position, app.commits.len())
    };
    let hint_style = FOOTER_STYLE.add_modifier(Modifier::DIM);
    let width = area.width as usize;
    let left_len = left.len();
    let hint_len = HINT.len();
    let line = if left_len + 2 + hint_len <= width {
        let padding = width - left_len - hint_len;
        Line::from(vec![
            Span::styled(left, FOOTER_STYLE),
            Span::styled(" ".repeat(padding), FOOTER_STYLE),
            Span::styled(HINT, hint_style),
        ])
    } else {
        Line::from(Span::styled(left, FOOTER_STYLE))
    };
    let footer = Paragraph::new(line).style(FOOTER_STYLE);
    frame.render_widget(footer, area);
}

const ACTION_FOOTER_STYLE: Style = Style::new().fg(Color::White).bg(Color::Cyan);
const ACTION_FOOTER_ACCENT: Style = Style::new().fg(Color::Gray).bg(Color::Cyan);

fn render_squash_footer(
    frame: &mut Frame,
    app: &AppState,
    area: Rect,
    source_index: usize,
    is_fixup: bool,
) {
    render_action_footer(
        frame,
        app,
        area,
        if is_fixup { "Fixup" } else { "Squash" },
        source_index,
        " into\u{2026}",
        &[("Enter", " confirm \u{b7} "), ("Esc", " cancel")],
    );
}

fn render_move_footer(frame: &mut Frame, app: &AppState, area: Rect, source_index: usize) {
    render_action_footer(
        frame,
        app,
        area,
        "Move",
        source_index,
        "",
        &[
            ("\u{2191}/\u{2193}", " pick position \u{b7} "),
            ("Enter", " confirm \u{b7} "),
            ("Esc", " cancel"),
        ],
    );
}

fn render_action_footer(
    frame: &mut Frame,
    app: &AppState,
    area: Rect,
    label: &str,
    source_index: usize,
    after_summary: &str,
    hints: &[(&str, &str)],
) {
    let source = match app.commits.get(source_index) {
        Some(c) => c,
        None => return,
    };

    let short_oid = source.oid.short();

    let hints_len: usize =
        " \u{b7} ".len() + hints.iter().map(|(k, d)| k.len() + d.len()).sum::<usize>();
    let max_summary_len = (area.width as usize)
        .saturating_sub(label.len() + 2)
        .saturating_sub(short_oid.len())
        .saturating_sub(3 + after_summary.len())
        .saturating_sub(hints_len);

    let summary = if source.summary.len() > max_summary_len && max_summary_len > 3 {
        format!("{}\u{2026}", &source.summary[..max_summary_len - 1])
    } else {
        source.summary.clone()
    };

    let mut spans = vec![
        Span::styled(format!(" {label} "), ACTION_FOOTER_STYLE),
        Span::styled(short_oid.to_string(), ACTION_FOOTER_ACCENT),
        Span::styled(
            format!(" \"{summary}\"{after_summary}"),
            ACTION_FOOTER_STYLE,
        ),
        Span::styled(" \u{b7} ", ACTION_FOOTER_STYLE),
    ];

    for (key, description) in hints {
        spans.push(Span::styled(key.to_string(), ACTION_FOOTER_ACCENT));
        spans.push(Span::styled(description.to_string(), ACTION_FOOTER_STYLE));
    }

    let footer = Paragraph::new(Line::from(spans)).style(ACTION_FOOTER_STYLE);
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
