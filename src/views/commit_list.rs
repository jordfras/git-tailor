// Commit list view rendering

use crate::app::AppState;
use ratatui::{
    layout::{Constraint, Layout},
    style::{Color, Style, Stylize},
    text::Span,
    widgets::{Cell, Paragraph, Row, Scrollbar, ScrollbarOrientation, ScrollbarState, Table},
    Frame,
};

/// Number of characters to display for short SHA.
const SHORT_SHA_LENGTH: usize = 8;

const HEADER_STYLE: Style = Style::new().fg(Color::White).bg(Color::Green);
const FOOTER_STYLE: Style = Style::new().fg(Color::White).bg(Color::Blue);

/// Render the commit list view.
///
/// Takes application state and renders the commit list to the terminal frame.
pub fn render(app: &AppState, frame: &mut Frame) {
    let [table_area, footer_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(frame.area());

    let header = Row::new(vec![Cell::from("SHA"), Cell::from("Title")]).style(HEADER_STYLE);

    // Available height for data rows: table area minus header (1)
    let available_height = table_area.height.saturating_sub(1) as usize;

    // Map selection_index to visual position depending on display order
    let visual_selection = if app.reverse {
        app.commits
            .len()
            .saturating_sub(1)
            .saturating_sub(app.selection_index)
    } else {
        app.selection_index
    };

    // Calculate scroll offset to keep visual selection visible
    let scroll_offset =
        if app.commits.is_empty() || available_height == 0 || visual_selection < available_height {
            0
        } else {
            visual_selection.saturating_sub(available_height - 1)
        };

    // Build commits in display order
    let display_commits: Vec<&crate::CommitInfo> = if app.reverse {
        app.commits.iter().rev().collect()
    } else {
        app.commits.iter().collect()
    };

    // Slice to visible range
    let visible_commits = if display_commits.is_empty() {
        &display_commits[..]
    } else {
        let end = (scroll_offset + available_height).min(display_commits.len());
        &display_commits[scroll_offset..end]
    };

    let rows: Vec<Row> = visible_commits
        .iter()
        .enumerate()
        .map(|(visible_index, commit)| {
            let visual_index = scroll_offset + visible_index;
            let short_sha: String = commit.oid.chars().take(SHORT_SHA_LENGTH).collect();
            let row = Row::new(vec![
                Cell::from(short_sha),
                Cell::from(commit.summary.clone()),
            ]);

            if visual_index == visual_selection {
                row.style(Style::default().reversed())
            } else {
                row
            }
        })
        .collect();

    let has_scrollbar = !app.commits.is_empty() && app.commits.len() > available_height;

    // Reserve left column for scrollbar when needed
    let (scrollbar_area, content_area) = if has_scrollbar {
        let [sb, content] =
            Layout::horizontal([Constraint::Length(1), Constraint::Min(0)]).areas(table_area);
        (Some(sb), content)
    } else {
        (None, table_area)
    };

    let table = Table::new(rows, [Constraint::Length(10), Constraint::Min(20)]).header(header);

    frame.render_widget(table, content_area);

    // Render scrollbar if content exceeds visible area
    if let Some(sb_area) = scrollbar_area {
        let mut scrollbar_state =
            ScrollbarState::new(app.commits.len().saturating_sub(1)).position(visual_selection);

        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalLeft)
            .begin_symbol(None)
            .end_symbol(None)
            .track_symbol(Some("│"));

        let sb_data_area = ratatui::layout::Rect {
            y: sb_area.y + 1,
            height: available_height as u16,
            ..sb_area
        };
        frame.render_stateful_widget(scrollbar, sb_data_area, &mut scrollbar_state);
    }

    // Render footer with selected commit info
    let footer_text = if app.commits.is_empty() {
        String::from("No commits")
    } else {
        let commit = &app.commits[app.selection_index];
        let position = app.commits.len() - app.selection_index;
        format!(" {} {}/{}", commit.oid, position, app.commits.len())
    };

    let footer = Paragraph::new(Span::styled(footer_text, FOOTER_STYLE)).style(FOOTER_STYLE);
    frame.render_widget(footer, footer_area);
}
