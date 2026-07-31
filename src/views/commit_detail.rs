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

// Commit detail view — metadata and diff

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
};

const HEADER_STYLE: Style = Style::new().fg(Color::White).bg(Color::Green);

use crate::VirtualOid;
use crate::app::{AppAction, AppState, KeyCommand};
use crate::views::palette::Colors;

use crate::repo::RepoRead;

mod search;

pub use search::handle_search_event;

/// File status indicator for changed files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
}

/// Handle an action while in CommitDetail mode.
pub fn handle_key(action: KeyCommand, app: &mut AppState) -> AppAction {
    match action {
        KeyCommand::MoveUp => {
            app.detail.v.step_back();
            AppAction::Handled
        }
        KeyCommand::MoveDown => {
            app.detail.v.step_forward();
            AppAction::Handled
        }
        KeyCommand::PageUp => {
            app.detail.v.page_back();
            AppAction::Handled
        }
        KeyCommand::PageDown => {
            app.detail.v.page_forward();
            AppAction::Handled
        }
        KeyCommand::HalfPageUp => {
            app.detail.v.half_page_back();
            AppAction::Handled
        }
        KeyCommand::HalfPageDown => {
            app.detail.v.half_page_forward();
            AppAction::Handled
        }
        KeyCommand::JumpToTop => {
            app.detail.v.to_start();
            AppAction::Handled
        }
        KeyCommand::JumpToBottom => {
            app.detail.v.to_end();
            AppAction::Handled
        }
        KeyCommand::ScrollLeft => {
            app.detail.h.step_back();
            AppAction::Handled
        }
        KeyCommand::ScrollRight => {
            app.detail.h.step_forward();
            AppAction::Handled
        }
        KeyCommand::ScrollToLeftEdge => {
            app.detail.h.to_start();
            AppAction::Handled
        }
        KeyCommand::ScrollToRightEdge => {
            app.detail.h.to_end();
            AppAction::Handled
        }
        KeyCommand::NavFileNext => {
            app.detail.jump_to_next_file();
            AppAction::Handled
        }
        KeyCommand::NavFilePrev => {
            app.detail.jump_to_prev_file();
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
        KeyCommand::Search => {
            app.search.activate();
            AppAction::Handled
        }
        KeyCommand::SearchNext => {
            search::next_match(app);
            AppAction::Handled
        }
        KeyCommand::SearchPrev => {
            search::prev_match(app);
            AppAction::Handled
        }
        KeyCommand::IncreaseContext => {
            app.detail.increase_context_lines();
            AppAction::Handled
        }
        KeyCommand::DecreaseContext => {
            app.detail.decrease_context_lines();
            AppAction::Handled
        }
        KeyCommand::Refresh => AppAction::ReloadCommits,
        KeyCommand::Quit => {
            if app.search.active {
                app.search.clear();
            } else {
                app.toggle_detail_view();
            }
            AppAction::Handled
        }
        KeyCommand::SeparatorLeft => {
            app.separator_offset = app.separator_offset.saturating_sub(4);
            AppAction::Handled
        }
        KeyCommand::SeparatorRight => {
            app.separator_offset = app.separator_offset.saturating_add(4);
            AppAction::Handled
        }
        _ => AppAction::Handled,
    }
}

/// Render the commit detail view.
///
/// Displays commit metadata and diff in the right panel.
pub fn render(repo: &impl RepoRead, frame: &mut Frame, app: &mut AppState, area: Rect) {
    // Split area into header, content, optional search bar, and footer.
    // When search is active the search bar occupies one row above the footer.
    let search_bar_height: u16 = if app.search.active { 1 } else { 0 };
    let [header_area, content_area, search_bar_area, footer_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(search_bar_height),
        Constraint::Length(1),
    ])
    .areas(area);

    // Render header: title on the left, current diff context lines on the right.
    let title = "Commit information";
    let context = format!("context: {} ", app.detail.context_lines.0);
    let width = header_area.width as usize;
    let pad = width.saturating_sub(title.len() + context.len());
    let header_line = Line::from(vec![
        Span::raw(title.to_string()),
        Span::raw(" ".repeat(pad)),
        Span::raw(context),
    ]);
    let header = Paragraph::new(header_line).style(app.colors.resolve_style(HEADER_STYLE));
    frame.render_widget(header, header_area);

    // Render content
    let mut search_info = None;
    if app.commits.is_empty() {
        let placeholder = Paragraph::new("No commits")
            .style(Style::default().fg(app.colors.resolve(Color::DarkGray)));
        frame.render_widget(placeholder, content_area);
    } else {
        let selected = app.commits[app.selection_index].clone();
        let oid = selected.oid.clone();
        let context_lines = app.detail.context_lines.0;

        let diff_opt = match oid {
            VirtualOid::Staged => match repo.staged_diff(context_lines) {
                Ok(diff_opt) => diff_opt,
                Err(err) => {
                    app.set_error_message(format!("Failed to load staged diff: {err}"));
                    None
                }
            },
            VirtualOid::Unstaged => match repo.unstaged_diff(context_lines) {
                Ok(diff_opt) => diff_opt,
                Err(err) => {
                    app.set_error_message(format!("Failed to load unstaged diff: {err}"));
                    None
                }
            },
            VirtualOid::Real(ref real_oid) => match repo.commit_diff(real_oid, context_lines) {
                Ok(diff) => Some(diff),
                Err(err) => {
                    app.set_error_message(format!("Failed to load commit diff: {err}"));
                    None
                }
            },
        };

        let mut content = build_metadata_lines(&selected, app.colors);
        if let Some(ref diff) = diff_opt
            && !diff.files.is_empty()
        {
            content.extend(build_file_list_lines(&diff.files, app.colors));
            let (diff_lines, file_offsets) = build_diff_lines(&diff.files, app.colors);
            let diff_start = content.len();
            app.detail.file_start_lines = file_offsets.iter().map(|&o| diff_start + o).collect();
            content.extend(diff_lines);
        } else {
            app.detail.file_start_lines.clear();
        }

        let layout = compute_scroll_layout(content_area, &content);

        // Update scroll state in app for proper bounds and page scrolling
        app.detail
            .v
            .set_bounds(layout.max_scroll, layout.visible_height);
        app.detail
            .h
            .set_bounds(layout.max_h_scroll, layout.text_area_width);

        (content, search_info) = search::apply(app, content);

        // Clamp the stored scroll offsets to the current content bounds. Writing
        // them back (not just a local copy) matters when the content shrinks —
        // e.g. reducing the diff context lines — so the offset snaps to the new
        // bottom instead of leaving the user stuck unable to scroll up.
        app.detail.v.clamp_offset();
        app.detail.h.clamp_offset();
        let scroll_offset = app.detail.v.offset;
        let h_scroll = app.detail.h.offset;

        let paragraph = Paragraph::new(content).scroll((scroll_offset as u16, h_scroll as u16));
        frame.render_widget(paragraph, layout.text_area);

        if layout.max_scroll > 0 && layout.visible_height > 0 {
            render_scrollbar(
                frame,
                layout.v_scrollbar_area,
                scroll_offset,
                layout.total_lines,
                layout.visible_height,
            );
        }
        if layout.max_h_scroll > 0 && layout.text_area_width > 0 {
            render_h_scrollbar(
                frame,
                layout.h_scrollbar_area,
                h_scroll,
                layout.max_line_width,
                layout.text_area_width,
            );
        }
    }

    // Render search bar (row above footer, only when search is active)
    if app.search.active {
        search::render_bar(frame, app, search_info, search_bar_area);
    }

    // Render footer
    super::commit_list::render_footer(frame, app, footer_area);
}

/// Build the metadata section: OID, full message, author, dates, committer.
fn build_metadata_lines(commit: &crate::CommitInfo, colors: Colors) -> Vec<Line<'static>> {
    let label = Style::default().fg(colors.resolve(Color::Yellow));
    let mut lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("Commit: ", label),
            Span::raw(commit.oid.long().to_string()),
        ]),
        Line::from(""),
    ];

    for line in commit.message.lines() {
        lines.push(Line::from(Span::styled(
            line.to_string(),
            Style::default().fg(colors.resolve(Color::White)),
        )));
    }

    lines.push(Line::from(""));

    if let (Some(author), Some(author_email)) = (&commit.author, &commit.author_email) {
        lines.push(Line::from(vec![
            Span::styled("Author: ", label),
            Span::raw(format!("{} <{}>", author, author_email)),
        ]));

        let fmt = time::macros::format_description!(
            "[year]-[month]-[day] [hour]:[minute]:[second] [offset_hour sign:mandatory][offset_minute]"
        );

        if let Some(author_date) = &commit.author_date {
            let formatted = author_date
                .format(&fmt)
                .unwrap_or_else(|_| String::from("Invalid date"));
            lines.push(Line::from(vec![
                Span::styled("Author Date: ", label),
                Span::raw(formatted),
            ]));
        }

        if let (Some(committer), Some(committer_email)) =
            (&commit.committer, &commit.committer_email)
        {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("Committer: ", label),
                Span::raw(format!("{} <{}>", committer, committer_email)),
            ]));
        }

        if let Some(commit_date) = &commit.commit_date {
            let formatted = commit_date
                .format(&fmt)
                .unwrap_or_else(|_| String::from("Invalid date"));
            lines.push(Line::from(vec![
                Span::styled("Commit Date: ", label),
                Span::raw(formatted),
            ]));
        }
    }

    lines
}

/// Build the "Changed Files:" section listing each file with a status indicator.
fn build_file_list_lines(files: &[crate::FileDiff], colors: Colors) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "Changed Files:",
            Style::default().fg(colors.resolve(Color::Yellow)),
        )),
        Line::from(""),
    ];

    for file in files {
        let (status, path) = get_file_status_and_path(file);
        let status_str = format_file_status(status);
        let status_color = colors.resolve(get_status_color(status));
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {} ", status_str),
                Style::default().fg(status_color),
            ),
            Span::raw(path),
        ]));
    }

    lines
}

/// Build the "Diff:" section with file headers, hunk headers, and +/- lines.
/// Returns the lines and a vec of indices (within the returned vec) where each
/// file's `--- ` header starts.
fn build_diff_lines(files: &[crate::FileDiff], colors: Colors) -> (Vec<Line<'static>>, Vec<usize>) {
    let white = colors.resolve(Color::White);
    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "Diff:",
            Style::default().fg(colors.resolve(Color::Yellow)),
        )),
        Line::from(""),
    ];

    let mut file_header_indices: Vec<usize> = Vec::new();

    for file in files {
        let old_path = diff_path_with_prefix(file.old_path.as_deref(), "a");
        let new_path = diff_path_with_prefix(file.new_path.as_deref(), "b");

        file_header_indices.push(lines.len());
        lines.push(Line::from(Span::styled(
            format!("--- {}", old_path),
            Style::default().fg(white),
        )));
        lines.push(Line::from(Span::styled(
            format!("+++ {}", new_path),
            Style::default().fg(white),
        )));

        for hunk in &file.hunks {
            lines.push(Line::from(Span::styled(
                format!(
                    "@@ -{},{} +{},{} @@",
                    hunk.old_start, hunk.old_lines, hunk.new_start, hunk.new_lines
                ),
                Style::default().fg(colors.resolve(Color::Cyan)),
            )));

            for diff_line in &hunk.lines {
                use crate::DiffLineKind;
                let (prefix, style) = match diff_line.kind {
                    DiffLineKind::Addition => {
                        ("+", Style::default().fg(colors.resolve(Color::Green)))
                    }
                    DiffLineKind::Deletion => {
                        ("-", Style::default().fg(colors.resolve(Color::Red)))
                    }
                    DiffLineKind::Context => (" ", Style::default().fg(white)),
                };
                let content_str = diff_line.content.trim_end_matches(['\n', '\r']);
                lines.push(Line::from(Span::styled(
                    format!("{}{}", prefix, content_str),
                    style,
                )));
            }
        }

        lines.push(Line::from(""));
    }

    (lines, file_header_indices)
}

/// Geometry produced by [`compute_scroll_layout`].
struct ScrollLayout {
    text_area: Rect,
    v_scrollbar_area: Rect,
    h_scrollbar_area: Rect,
    total_lines: usize,
    max_line_width: usize,
    text_area_width: usize,
    visible_height: usize,
    max_scroll: usize,
    max_h_scroll: usize,
}

/// Compute scroll geometry for the content area given the current content lines.
fn compute_scroll_layout(content_area: Rect, content: &[Line<'_>]) -> ScrollLayout {
    let max_line_width = content.iter().map(|l| l.width()).max().unwrap_or(0);
    let total_lines = content.len();

    // Pass 1: tentative v-scrollbar width (before knowing h-scrollbar height)
    let v_scrollbar_width_tentative: u16 = if total_lines > content_area.height as usize {
        1
    } else {
        0
    };
    let text_area_width = content_area
        .width
        .saturating_sub(v_scrollbar_width_tentative) as usize;
    let max_h_scroll = max_line_width.saturating_sub(text_area_width);
    let h_scrollbar_height: u16 = if max_h_scroll > 0 { 1 } else { 0 };

    let visible_height = content_area.height.saturating_sub(h_scrollbar_height) as usize;
    let max_scroll = total_lines.saturating_sub(visible_height);

    let v_scrollbar_width: u16 = if max_scroll > 0 { 1 } else { 0 };

    let v_scrollbar_area = Rect {
        x: content_area.x,
        y: content_area.y,
        width: v_scrollbar_width,
        height: content_area.height.saturating_sub(h_scrollbar_height),
    };
    let text_area = Rect {
        x: content_area.x + v_scrollbar_width,
        y: content_area.y,
        width: content_area.width.saturating_sub(v_scrollbar_width),
        height: content_area.height.saturating_sub(h_scrollbar_height),
    };
    let h_scrollbar_area = Rect {
        x: content_area.x + v_scrollbar_width,
        y: content_area.y + content_area.height.saturating_sub(h_scrollbar_height),
        width: content_area.width.saturating_sub(v_scrollbar_width),
        height: h_scrollbar_height,
    };

    ScrollLayout {
        text_area,
        v_scrollbar_area,
        h_scrollbar_area,
        total_lines,
        max_line_width,
        text_area_width,
        visible_height,
        max_scroll,
        max_h_scroll,
    }
}

/// Determine file status and display path from a FileDiff.
fn get_file_status_and_path(file: &crate::FileDiff) -> (FileStatus, String) {
    use crate::DeltaStatus;

    let status = match file.status {
        DeltaStatus::Added => FileStatus::Added,
        DeltaStatus::Deleted => FileStatus::Deleted,
        DeltaStatus::Modified => FileStatus::Modified,
        DeltaStatus::Renamed | DeltaStatus::Copied => FileStatus::Renamed,
        DeltaStatus::Typechange => FileStatus::Modified,
        _ => FileStatus::Modified,
    };

    let path = match (&file.old_path, &file.new_path) {
        (_, Some(new))
            if file.status != DeltaStatus::Renamed && file.status != DeltaStatus::Copied =>
        {
            new.clone()
        }
        (Some(old), Some(new)) => format!("{} → {}", old, new),
        (Some(old), None) => old.clone(),
        (None, Some(new)) => new.clone(),
        (None, None) => String::from("<unknown>"),
    };

    (status, path)
}

/// Format file status as a single character indicator.
fn format_file_status(status: FileStatus) -> &'static str {
    match status {
        FileStatus::Added => "A",
        FileStatus::Modified => "M",
        FileStatus::Deleted => "D",
        FileStatus::Renamed => "R",
    }
}

/// Get color for file status indicator.
fn get_status_color(status: FileStatus) -> Color {
    match status {
        FileStatus::Added => Color::Green,
        FileStatus::Modified => Color::Blue,
        FileStatus::Deleted => Color::Red,
        FileStatus::Renamed => Color::Cyan,
    }
}

/// Render a vertical scrollbar indicating scroll position.
fn render_scrollbar(
    frame: &mut Frame,
    area: Rect,
    scroll_offset: usize,
    total_lines: usize,
    visible_height: usize,
) {
    if area.height == 0 || total_lines == 0 {
        return;
    }
    let max_scroll = total_lines.saturating_sub(visible_height);
    let mut state = ScrollbarState::new(max_scroll).position(scroll_offset);
    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalLeft)
        .begin_symbol(None)
        .end_symbol(None)
        .track_symbol(Some("│"));
    frame.render_stateful_widget(scrollbar, area, &mut state);
}

/// Render a horizontal scrollbar indicating horizontal scroll position.
fn render_h_scrollbar(
    frame: &mut Frame,
    area: Rect,
    h_scroll: usize,
    max_line_width: usize,
    visible_width: usize,
) {
    if area.width == 0 || max_line_width == 0 {
        return;
    }
    let max_h_scroll = max_line_width.saturating_sub(visible_width);
    let mut state = ScrollbarState::new(max_h_scroll).position(h_scroll);
    let scrollbar = Scrollbar::new(ScrollbarOrientation::HorizontalBottom)
        .begin_symbol(None)
        .end_symbol(None)
        .track_symbol(Some("─"));
    frame.render_stateful_widget(scrollbar, area, &mut state);
}

/// Format a diff file path with the given prefix, falling back to `/dev/null`
/// when the path is absent (e.g. for added or deleted files).
fn diff_path_with_prefix(path: Option<&str>, prefix: &str) -> String {
    path.map(|s| format!("{prefix}/{s}"))
        .unwrap_or_else(|| "/dev/null".to_string())
}
