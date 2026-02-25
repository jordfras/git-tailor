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
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

const HEADER_STYLE: Style = Style::new().fg(Color::White).bg(Color::Green);
const FOOTER_STYLE: Style = Style::new().fg(Color::White).bg(Color::Blue);

use crate::{app::AppState, repo::GitRepo};

/// File status indicator for changed files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
}

/// Render the commit detail view.
///
/// Displays commit metadata and diff in the right panel.
/// Currently a placeholder showing the selected commit's summary.
pub fn render(repo: &impl GitRepo, frame: &mut Frame, app: &mut AppState, area: Rect) {
    // Split area into header, content, and footer
    let [header_area, content_area, footer_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(area);

    // Render header
    let header_text = "Commit information";
    let header = Paragraph::new(header_text).style(HEADER_STYLE);
    frame.render_widget(header, header_area);

    // Render content
    if app.commits.is_empty() {
        let placeholder = Paragraph::new("No commits").style(Style::default().fg(Color::DarkGray));
        frame.render_widget(placeholder, content_area);
    } else {
        let selected = &app.commits[app.selection_index];

        // Build metadata lines
        let mut content = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("Commit: ", Style::default().fg(Color::Yellow)),
                Span::raw(&selected.oid),
            ]),
            Line::from(""),
        ];

        // Add full message (split into lines)
        for line in selected.message.lines() {
            content.push(Line::from(Span::styled(
                line,
                Style::default().fg(Color::White),
            )));
        }

        content.push(Line::from(""));
        content.push(Line::from(vec![
            Span::styled("Author: ", Style::default().fg(Color::Yellow)),
            Span::raw(format!("{} <{}>", selected.author, selected.author_email)),
        ]));

        // Format dates as "YYYY-MM-DD HH:MM:SS ±HHMM"
        let format = time::format_description::parse(
            "[year]-[month]-[day] [hour]:[minute]:[second] [offset_hour sign:mandatory][offset_minute]"
        ).unwrap();

        let author_date_formatted = selected
            .author_date
            .format(&format)
            .unwrap_or_else(|_| String::from("Invalid date"));

        let commit_date_formatted = selected
            .commit_date
            .format(&format)
            .unwrap_or_else(|_| String::from("Invalid date"));

        content.push(Line::from(vec![
            Span::styled("Author Date: ", Style::default().fg(Color::Yellow)),
            Span::raw(author_date_formatted),
        ]));
        content.push(Line::from(""));
        content.push(Line::from(vec![
            Span::styled("Committer: ", Style::default().fg(Color::Yellow)),
            Span::raw(format!(
                "{} <{}>",
                selected.committer, selected.committer_email
            )),
        ]));
        content.push(Line::from(vec![
            Span::styled("Commit Date: ", Style::default().fg(Color::Yellow)),
            Span::raw(commit_date_formatted),
        ]));

        // Add file list with status indicators
        let diff_opt = match selected.oid.as_str() {
            "staged" => repo.staged_diff(),
            "unstaged" => repo.unstaged_diff(),
            oid => repo.commit_diff(oid).ok(),
        };
        if let Some(diff) = diff_opt {
            content.push(Line::from(""));
            content.push(Line::from(Span::styled(
                "Changed Files:",
                Style::default().fg(Color::Yellow),
            )));
            content.push(Line::from(""));

            for file in &diff.files {
                let (status, path) = get_file_status_and_path(file);
                let status_str = format_file_status(status);
                let status_color = get_status_color(status);

                content.push(Line::from(vec![
                    Span::styled(
                        format!("  {} ", status_str),
                        Style::default().fg(status_color),
                    ),
                    Span::raw(path),
                ]));
            }

            // Add complete diff rendering
            content.push(Line::from(""));
            content.push(Line::from(Span::styled(
                "Diff:",
                Style::default().fg(Color::Yellow),
            )));
            content.push(Line::from(""));

            for file in &diff.files {
                // File headers (unified diff format)
                let old_path = file
                    .old_path
                    .as_ref()
                    .map(|s| format!("a/{}", s))
                    .unwrap_or_else(|| "/dev/null".to_string());
                let new_path = file
                    .new_path
                    .as_ref()
                    .map(|s| format!("b/{}", s))
                    .unwrap_or_else(|| "/dev/null".to_string());

                content.push(Line::from(Span::styled(
                    format!("--- {}", old_path),
                    Style::default().fg(Color::White),
                )));
                content.push(Line::from(Span::styled(
                    format!("+++ {}", new_path),
                    Style::default().fg(Color::White),
                )));

                // Render each hunk
                for hunk in &file.hunks {
                    // Hunk header
                    let hunk_header = format!(
                        "@@ -{},{} +{},{} @@",
                        hunk.old_start, hunk.old_lines, hunk.new_start, hunk.new_lines
                    );
                    content.push(Line::from(Span::styled(
                        hunk_header,
                        Style::default().fg(Color::Cyan),
                    )));

                    // Render each line
                    for line in &hunk.lines {
                        use crate::DiffLineKind;

                        let (prefix, style) = match line.kind {
                            DiffLineKind::Addition => ("+", Style::default().fg(Color::Green)),
                            DiffLineKind::Deletion => ("-", Style::default().fg(Color::Red)),
                            DiffLineKind::Context => (" ", Style::default().fg(Color::White)),
                        };

                        // Remove trailing newline if present
                        let content_str = line.content.trim_end_matches('\n');
                        content.push(Line::from(Span::styled(
                            format!("{}{}", prefix, content_str),
                            style,
                        )));
                    }
                }

                content.push(Line::from(""));
            }
        }

        // Calculate scrolling bounds
        let total_lines = content.len();
        let visible_height = content_area.height as usize;
        let max_scroll = total_lines.saturating_sub(visible_height);

        // Update scroll state in app for proper bounds and page scrolling
        app.max_detail_scroll = max_scroll;
        app.detail_visible_height = visible_height;

        // Clamp scroll offset to valid range
        let scroll_offset = app.detail_scroll_offset.min(max_scroll);

        // Split content area to make room for scrollbar
        let scrollbar_width = if max_scroll > 0 { 1 } else { 0 };
        let scrollbar_area = Rect {
            x: content_area.x,
            y: content_area.y,
            width: scrollbar_width,
            height: content_area.height,
        };
        let text_area = Rect {
            x: content_area.x + scrollbar_width,
            y: content_area.y,
            width: content_area.width.saturating_sub(scrollbar_width),
            height: content_area.height,
        };

        let paragraph = Paragraph::new(content).scroll((scroll_offset as u16, 0));
        frame.render_widget(paragraph, text_area);

        // Render scrollbar if content doesn't fit
        if max_scroll > 0 && visible_height > 0 {
            render_scrollbar(
                frame,
                scrollbar_area,
                scroll_offset,
                total_lines,
                visible_height,
            );
        }
    }

    // Render footer
    let footer = Paragraph::new("").style(FOOTER_STYLE);
    frame.render_widget(footer, footer_area);
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

    let scrollbar_height = area.height as usize;

    // Calculate thumb size (proportional to visible content)
    let thumb_size = ((visible_height as f64 / total_lines as f64) * scrollbar_height as f64)
        .ceil()
        .max(1.0) as usize;
    let thumb_size = thumb_size.min(scrollbar_height);

    // Calculate thumb position
    let scrollable_height = scrollbar_height.saturating_sub(thumb_size);
    let thumb_position = if total_lines > visible_height {
        ((scroll_offset as f64 / (total_lines - visible_height) as f64) * scrollable_height as f64)
            .round() as usize
    } else {
        0
    };

    // Build scrollbar lines
    let mut scrollbar_lines = Vec::new();
    for i in 0..scrollbar_height {
        let char = if i >= thumb_position && i < thumb_position + thumb_size {
            "█" // Solid block for thumb
        } else {
            "│" // Light vertical line for track
        };
        scrollbar_lines.push(Line::from(Span::styled(
            char,
            Style::default().fg(Color::DarkGray),
        )));
    }

    let scrollbar = Paragraph::new(scrollbar_lines);
    frame.render_widget(scrollbar, area);
}