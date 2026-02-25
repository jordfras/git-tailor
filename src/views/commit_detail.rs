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

use crate::{app::AppState, repo};

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
pub fn render(frame: &mut Frame, app: &AppState, area: Rect) {
    // Split area into header, content, and footer
    let [header_area, content_area, footer_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(area);

    // Render header
    let header_text = " Commit information";
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
        if let Ok(diff) = repo::commit_diff(&selected.oid) {
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
        }

        let paragraph = Paragraph::new(content);
        frame.render_widget(paragraph, content_area);
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
