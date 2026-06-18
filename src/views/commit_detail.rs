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

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
};
use regex::RegexBuilder;

const HEADER_STYLE: Style = Style::new().fg(Color::White).bg(Color::Green);

use crate::VirtualOid;
use crate::app::{AppAction, AppState, KeyCommand};

#[derive(Clone, Copy, PartialEq, Eq)]
enum SearchDirection {
    Next,
    Prev,
}
use crate::repo::GitRepo;

/// Transient info about the search bar, computed during render.
enum SearchBarInfo {
    NoMatches,
    InvalidPattern,
    Matches { current: usize, total: usize },
}

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
            app.scroll_detail_up();
            AppAction::Handled
        }
        KeyCommand::MoveDown => {
            app.scroll_detail_down();
            AppAction::Handled
        }
        KeyCommand::PageUp => {
            app.scroll_detail_page_up(app.detail_visible_height);
            AppAction::Handled
        }
        KeyCommand::PageDown => {
            app.scroll_detail_page_down(app.detail_visible_height);
            AppAction::Handled
        }
        KeyCommand::HalfPageUp => {
            app.scroll_detail_half_page_up(app.detail_visible_height);
            AppAction::Handled
        }
        KeyCommand::HalfPageDown => {
            app.scroll_detail_half_page_down(app.detail_visible_height);
            AppAction::Handled
        }
        KeyCommand::JumpToTop => {
            app.scroll_detail_to_top();
            AppAction::Handled
        }
        KeyCommand::JumpToBottom => {
            app.scroll_detail_to_bottom();
            AppAction::Handled
        }
        KeyCommand::ScrollLeft => {
            app.scroll_detail_left();
            AppAction::Handled
        }
        KeyCommand::ScrollRight => {
            app.scroll_detail_right();
            AppAction::Handled
        }
        KeyCommand::ScrollToLeftEdge => {
            app.scroll_detail_to_left_edge();
            AppAction::Handled
        }
        KeyCommand::ScrollToRightEdge => {
            app.scroll_detail_to_right_edge();
            AppAction::Handled
        }
        KeyCommand::NavFileNext => {
            app.jump_to_next_file();
            AppAction::Handled
        }
        KeyCommand::NavFilePrev => {
            app.jump_to_prev_file();
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
            app.activate_search();
            AppAction::Handled
        }
        KeyCommand::SearchNext => {
            advance_search_match(app, SearchDirection::Next);
            AppAction::Handled
        }
        KeyCommand::SearchPrev => {
            advance_search_match(app, SearchDirection::Prev);
            AppAction::Handled
        }
        KeyCommand::Update => AppAction::ReloadCommits,
        KeyCommand::Quit => {
            if app.search_active {
                app.clear_search();
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

/// Handle raw keyboard events while the search input bar is active.
///
/// Called from main.rs before `parse_key` so that character keys are
/// captured as search-query input rather than dispatched as commands.
pub fn handle_search_event(event: Event, app: &mut AppState) -> AppAction {
    if let Event::Key(KeyEvent {
        code,
        kind,
        modifiers,
        ..
    }) = event
        && kind == event::KeyEventKind::Press
    {
        if code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
            return AppAction::Quit;
        }
        match code {
            KeyCode::Esc => {
                app.clear_search();
            }
            KeyCode::Enter => {
                app.search_input_active = false;
                // Jump to the first match at or after the current scroll
                // position, wrapping to match 0 when all matches lie above —
                // the same behaviour as `less`.
                if !app.search_matches.is_empty() {
                    let next_idx = app
                        .search_matches
                        .iter()
                        .position(|&line| line >= app.detail_scroll_offset)
                        .unwrap_or(0);
                    app.search_match_index = Some(next_idx);
                    scroll_to_current_match(app);
                }
            }
            KeyCode::Backspace => {
                app.search_query.pop();
            }
            KeyCode::Char(c) => {
                app.search_query.push(c);
            }
            _ => {}
        }
    }
    AppAction::Handled
}

/// Advance to the next or previous search match, wrapping around.
fn advance_search_match(app: &mut AppState, dir: SearchDirection) {
    if !app.search_active || app.search_matches.is_empty() {
        return;
    }
    let len = app.search_matches.len();
    app.search_match_index = Some(next_match_index(app.search_match_index, len, dir));
    scroll_to_current_match(app);
}

/// Return the next match index given the current index, list length, and direction.
/// Wraps around at both ends.
fn next_match_index(current: Option<usize>, len: usize, dir: SearchDirection) -> usize {
    match (current, dir) {
        (Some(idx), SearchDirection::Next) => (idx + 1) % len,
        (Some(0), SearchDirection::Prev) => len - 1,
        (Some(idx), SearchDirection::Prev) => idx - 1,
        (None, SearchDirection::Next) => 0,
        (None, SearchDirection::Prev) => len - 1,
    }
}

/// Scroll the detail view so the current search match line is visible.
fn scroll_to_current_match(app: &mut AppState) {
    if let Some(match_idx) = app.search_match_index
        && let Some(&target_line) = app.search_matches.get(match_idx)
    {
        let vh = app.detail_visible_height;
        if vh == 0 {
            return;
        }
        if target_line < app.detail_scroll_offset || target_line >= app.detail_scroll_offset + vh {
            let centered = target_line.saturating_sub(vh / 2);
            app.detail_scroll_offset = centered.min(app.max_detail_scroll);
        }
    }
}

/// Highlight regex matches within a single styled Span, returning one or more
/// replacement Spans.
fn highlight_span_matches(
    span: &Span<'_>,
    regex: &regex::Regex,
    highlight_style: Style,
) -> Vec<Span<'static>> {
    let text = span.content.as_ref();
    let style = span.style;
    let mut result = Vec::new();
    let mut last_end = 0;

    for m in regex.find_iter(text) {
        if m.start() > last_end {
            result.push(Span::styled(text[last_end..m.start()].to_string(), style));
        }
        result.push(Span::styled(
            text[m.start()..m.end()].to_string(),
            highlight_style,
        ));
        last_end = m.end();
    }
    if last_end < text.len() {
        result.push(Span::styled(text[last_end..].to_string(), style));
    }
    if result.is_empty() {
        result.push(Span::styled(text.to_string(), style));
    }
    result
}

/// Apply search-match highlighting to an entire Line.
fn highlight_line_matches(
    line: Line<'_>,
    regex: &regex::Regex,
    is_current_match: bool,
) -> Line<'static> {
    let highlight_style = if is_current_match {
        Style::default().fg(Color::Black).bg(Color::LightCyan)
    } else {
        Style::default().fg(Color::Black).bg(Color::Yellow)
    };
    let new_spans: Vec<Span<'static>> = line
        .spans
        .iter()
        .flat_map(|span| highlight_span_matches(span, regex, highlight_style))
        .collect();
    Line::from(new_spans)
}

/// Return the plain-text content of a Line (all spans concatenated).
fn line_text(line: &Line<'_>) -> String {
    line.spans.iter().map(|s| s.content.as_ref()).collect()
}

/// Find all content lines matching `regex`, update `app.search_matches` and
/// `app.search_match_index`, and return the corresponding `SearchBarInfo` plus
/// the previous match index (so callers can detect whether it changed).
fn compute_search_matches(
    app: &mut AppState,
    content: &[Line<'_>],
    regex: &regex::Regex,
) -> (Option<SearchBarInfo>, Option<usize>) {
    let prev_match_index = app.search_match_index;

    app.search_matches = content
        .iter()
        .enumerate()
        .filter(|(_, line)| regex.is_match(&line_text(line)))
        .map(|(i, _)| i)
        .collect();

    if app.search_matches.is_empty() {
        app.search_match_index = None;
        (Some(SearchBarInfo::NoMatches), prev_match_index)
    } else {
        match app.search_match_index {
            None => app.search_match_index = Some(0),
            Some(idx) if idx >= app.search_matches.len() => {
                app.search_match_index = Some(app.search_matches.len() - 1);
            }
            _ => {}
        }
        (
            Some(SearchBarInfo::Matches {
                current: app.search_match_index.unwrap(),
                total: app.search_matches.len(),
            }),
            prev_match_index,
        )
    }
}

/// Scroll the detail view so the current match is visible, but only when the
/// match index actually changed (first match found or index clamped). This
/// avoids overriding manual arrow/PageUp/PageDown scrolling on every render.
fn auto_scroll_to_new_match(
    app: &mut AppState,
    prev_match_index: Option<usize>,
    visible_height: usize,
    max_scroll: usize,
) {
    if app.search_match_index != prev_match_index
        && let Some(mi) = app.search_match_index
    {
        let target_line = app.search_matches[mi];
        if visible_height > 0
            && (target_line < app.detail_scroll_offset
                || target_line >= app.detail_scroll_offset + visible_height)
        {
            let centered = target_line.saturating_sub(visible_height / 2);
            app.detail_scroll_offset = centered.min(max_scroll);
        }
    }
}

/// Apply search-match highlighting to all content lines.
fn apply_search_highlighting(
    content: Vec<Line<'_>>,
    app: &AppState,
    regex: &regex::Regex,
) -> Vec<Line<'static>> {
    let current_match_line = app.search_match_index.map(|i| app.search_matches[i]);
    content
        .into_iter()
        .enumerate()
        .map(|(i, line)| {
            let is_current = current_match_line == Some(i);
            highlight_line_matches(line, regex, is_current)
        })
        .collect()
}

/// Render the search bar widget into the given area.
fn render_search_bar(
    frame: &mut Frame,
    app: &AppState,
    search_info: Option<SearchBarInfo>,
    area: Rect,
) {
    let mut bar_spans = vec![
        Span::styled("/", Style::default().fg(Color::DarkGray)),
        Span::styled(app.search_query.clone(), Style::default().fg(Color::White)),
    ];
    if app.search_input_active {
        bar_spans.push(Span::styled("█", Style::default().fg(Color::DarkGray)));
    }
    match search_info {
        Some(SearchBarInfo::NoMatches) => {
            bar_spans.push(Span::styled(
                "  [no matches]",
                Style::default().fg(Color::Red),
            ));
        }
        Some(SearchBarInfo::InvalidPattern) => {
            bar_spans.push(Span::styled(
                "  [invalid pattern]",
                Style::default().fg(Color::Red),
            ));
        }
        Some(SearchBarInfo::Matches { current, total }) => {
            bar_spans.push(Span::styled(
                format!("  [{}/{}]", current + 1, total),
                Style::default().fg(Color::DarkGray),
            ));
        }
        None => {}
    }
    frame.render_widget(Paragraph::new(Line::from(bar_spans)), area);
}

/// Render the commit detail view.
///
/// Displays commit metadata and diff in the right panel.
pub fn render(repo: &impl GitRepo, frame: &mut Frame, app: &mut AppState, area: Rect) {
    // Split area into header, content, optional search bar, and footer.
    // When search is active the search bar occupies one row above the footer.
    let search_bar_height: u16 = if app.search_active { 1 } else { 0 };
    let [header_area, content_area, search_bar_area, footer_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(search_bar_height),
        Constraint::Length(1),
    ])
    .areas(area);

    // Render header
    let header_text = "Commit information";
    let header = Paragraph::new(header_text).style(HEADER_STYLE);
    frame.render_widget(header, header_area);

    // Render content
    let mut search_info: Option<SearchBarInfo> = None;
    if app.commits.is_empty() {
        let placeholder = Paragraph::new("No commits").style(Style::default().fg(Color::DarkGray));
        frame.render_widget(placeholder, content_area);
    } else {
        let selected = app.commits[app.selection_index].clone();
        let oid = selected.oid.clone();

        let diff_opt = match oid {
            VirtualOid::Staged => match repo.staged_diff() {
                Ok(diff_opt) => diff_opt,
                Err(err) => {
                    app.set_error_message(format!("Failed to load staged diff: {err}"));
                    None
                }
            },
            VirtualOid::Unstaged => match repo.unstaged_diff() {
                Ok(diff_opt) => diff_opt,
                Err(err) => {
                    app.set_error_message(format!("Failed to load unstaged diff: {err}"));
                    None
                }
            },
            VirtualOid::Real(ref real_oid) => match repo.commit_diff(real_oid) {
                Ok(diff) => Some(diff),
                Err(err) => {
                    app.set_error_message(format!("Failed to load commit diff: {err}"));
                    None
                }
            },
        };

        let mut content = build_metadata_lines(&selected);
        if let Some(ref diff) = diff_opt
            && !diff.files.is_empty()
        {
            content.extend(build_file_list_lines(&diff.files));
            let (diff_lines, file_offsets) = build_diff_lines(&diff.files);
            let diff_start = content.len();
            app.file_start_lines = file_offsets.iter().map(|&o| diff_start + o).collect();
            content.extend(diff_lines);
        } else {
            app.file_start_lines.clear();
        }

        let layout = compute_scroll_layout(content_area, &content);

        // Update scroll state in app for proper bounds and page scrolling
        app.max_detail_scroll = layout.max_scroll;
        app.detail_visible_height = layout.visible_height;
        app.max_detail_h_scroll = layout.max_h_scroll;

        // --- Search: compute matches, auto-scroll, apply highlighting ---
        if app.search_active && !app.search_query.is_empty() {
            match RegexBuilder::new(&app.search_query).build() {
                Ok(regex) => {
                    let (info, prev_match_index) = compute_search_matches(app, &content, &regex);
                    search_info = info;
                    if !app.search_matches.is_empty() {
                        if !app.search_input_active {
                            auto_scroll_to_new_match(
                                app,
                                prev_match_index,
                                layout.visible_height,
                                layout.max_scroll,
                            );
                        }
                        content = apply_search_highlighting(content, app, &regex);
                    }
                }
                Err(_) => {
                    app.search_matches.clear();
                    app.search_match_index = None;
                    search_info = Some(SearchBarInfo::InvalidPattern);
                }
            }
        } else if !app.search_active {
            app.search_matches.clear();
            app.search_match_index = None;
        }

        // Clamp scroll offsets to valid range
        let scroll_offset = app.detail_scroll_offset.min(layout.max_scroll);
        let h_scroll = app.detail_h_scroll_offset.min(layout.max_h_scroll);

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
    if app.search_active {
        render_search_bar(frame, app, search_info, search_bar_area);
    }

    // Render footer
    super::commit_list::render_footer(frame, app, footer_area);
}

/// Build the metadata section: OID, full message, author, dates, committer.
fn build_metadata_lines(commit: &crate::CommitInfo) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("Commit: ", Style::default().fg(Color::Yellow)),
            Span::raw(commit.oid.long().to_string()),
        ]),
        Line::from(""),
    ];

    for line in commit.message.lines() {
        lines.push(Line::from(Span::styled(
            line.to_string(),
            Style::default().fg(Color::White),
        )));
    }

    lines.push(Line::from(""));

    if let (Some(author), Some(author_email)) = (&commit.author, &commit.author_email) {
        lines.push(Line::from(vec![
            Span::styled("Author: ", Style::default().fg(Color::Yellow)),
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
                Span::styled("Author Date: ", Style::default().fg(Color::Yellow)),
                Span::raw(formatted),
            ]));
        }

        if let (Some(committer), Some(committer_email)) =
            (&commit.committer, &commit.committer_email)
        {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("Committer: ", Style::default().fg(Color::Yellow)),
                Span::raw(format!("{} <{}>", committer, committer_email)),
            ]));
        }

        if let Some(commit_date) = &commit.commit_date {
            let formatted = commit_date
                .format(&fmt)
                .unwrap_or_else(|_| String::from("Invalid date"));
            lines.push(Line::from(vec![
                Span::styled("Commit Date: ", Style::default().fg(Color::Yellow)),
                Span::raw(formatted),
            ]));
        }
    }

    lines
}

/// Build the "Changed Files:" section listing each file with a status indicator.
fn build_file_list_lines(files: &[crate::FileDiff]) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "Changed Files:",
            Style::default().fg(Color::Yellow),
        )),
        Line::from(""),
    ];

    for file in files {
        let (status, path) = get_file_status_and_path(file);
        let status_str = format_file_status(status);
        let status_color = get_status_color(status);
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
fn build_diff_lines(files: &[crate::FileDiff]) -> (Vec<Line<'static>>, Vec<usize>) {
    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled("Diff:", Style::default().fg(Color::Yellow))),
        Line::from(""),
    ];

    let mut file_header_indices: Vec<usize> = Vec::new();

    for file in files {
        let old_path = diff_path_with_prefix(file.old_path.as_deref(), "a");
        let new_path = diff_path_with_prefix(file.new_path.as_deref(), "b");

        file_header_indices.push(lines.len());
        lines.push(Line::from(Span::styled(
            format!("--- {}", old_path),
            Style::default().fg(Color::White),
        )));
        lines.push(Line::from(Span::styled(
            format!("+++ {}", new_path),
            Style::default().fg(Color::White),
        )));

        for hunk in &file.hunks {
            lines.push(Line::from(Span::styled(
                format!(
                    "@@ -{},{} +{},{} @@",
                    hunk.old_start, hunk.old_lines, hunk.new_start, hunk.new_lines
                ),
                Style::default().fg(Color::Cyan),
            )));

            for diff_line in &hunk.lines {
                use crate::DiffLineKind;
                let (prefix, style) = match diff_line.kind {
                    DiffLineKind::Addition => ("+", Style::default().fg(Color::Green)),
                    DiffLineKind::Deletion => ("-", Style::default().fg(Color::Red)),
                    DiffLineKind::Context => (" ", Style::default().fg(Color::White)),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_from_none_forward() {
        assert_eq!(next_match_index(None, 5, SearchDirection::Next), 0);
    }

    #[test]
    fn next_from_none_backward() {
        assert_eq!(next_match_index(None, 5, SearchDirection::Prev), 4);
    }

    #[test]
    fn next_wraps_forward() {
        assert_eq!(next_match_index(Some(4), 5, SearchDirection::Next), 0);
    }

    #[test]
    fn next_wraps_backward() {
        assert_eq!(next_match_index(Some(0), 5, SearchDirection::Prev), 4);
    }

    #[test]
    fn next_advances_forward() {
        assert_eq!(next_match_index(Some(2), 5, SearchDirection::Next), 3);
    }

    #[test]
    fn next_advances_backward() {
        assert_eq!(next_match_index(Some(3), 5, SearchDirection::Prev), 2);
    }
}
