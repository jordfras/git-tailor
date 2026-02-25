// Commit list view rendering

use crate::app::AppState;
use ratatui::{
    layout::{Constraint, Layout},
    style::{Color, Style, Stylize},
    text::Span,
    widgets::{
        Block, Borders, Cell, Paragraph, Row, Scrollbar, ScrollbarOrientation, ScrollbarState,
        Table,
    },
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

    // Available height for data rows: table area minus borders (2) and header (1)
    let available_height = table_area.height.saturating_sub(3) as usize;

    // Calculate scroll offset to keep selection visible
    let scroll_offset = if app.commits.is_empty()
        || available_height == 0
        || app.selection_index < available_height
    {
        0
    } else {
        app.selection_index.saturating_sub(available_height - 1)
    };

    // Slice commits to visible range
    let visible_commits = if app.commits.is_empty() {
        &app.commits[..]
    } else {
        let end = (scroll_offset + available_height).min(app.commits.len());
        &app.commits[scroll_offset..end]
    };

    let rows: Vec<Row> = visible_commits
        .iter()
        .enumerate()
        .map(|(visible_index, commit)| {
            let absolute_index = scroll_offset + visible_index;
            let short_sha: String = commit.oid.chars().take(SHORT_SHA_LENGTH).collect();
            let row = Row::new(vec![
                Cell::from(short_sha),
                Cell::from(commit.summary.clone()),
            ]);

            if absolute_index == app.selection_index {
                row.style(Style::default().reversed())
            } else {
                row
            }
        })
        .collect();

    let table = Table::new(rows, [Constraint::Length(10), Constraint::Min(20)])
        .header(header)
        .block(Block::default().borders(Borders::ALL).title("Commits"));

    frame.render_widget(table, table_area);

    // Render scrollbar if content exceeds visible area
    if !app.commits.is_empty() && app.commits.len() > available_height {
        let mut scrollbar_state =
            ScrollbarState::new(app.commits.len().saturating_sub(1)).position(app.selection_index);

        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalLeft)
            .begin_symbol(None)
            .end_symbol(None)
            .track_symbol(Some("│"));

        let scrollbar_area = ratatui::layout::Rect {
            y: table_area.y + 2,
            height: available_height as u16,
            ..table_area
        };
        frame.render_stateful_widget(scrollbar, scrollbar_area, &mut scrollbar_state);
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
