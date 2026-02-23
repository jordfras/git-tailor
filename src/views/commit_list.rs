// Commit list view rendering

use crate::app::AppState;
use ratatui::{
    layout::Constraint,
    style::{Style, Stylize},
    widgets::{Block, Borders, Cell, Row, Scrollbar, ScrollbarOrientation, ScrollbarState, Table},
    Frame,
};

/// Number of characters to display for short SHA.
const SHORT_SHA_LENGTH: usize = 8;

/// Render the commit list view.
///
/// Takes application state and renders the commit list to the terminal frame.
pub fn render(app: &AppState, frame: &mut Frame) {
    let header =
        Row::new(vec![Cell::from("SHA"), Cell::from("Title")]).style(Style::default().bold());

    // Calculate available height for table content (excluding borders and header)
    let available_height = frame.area().height.saturating_sub(3) as usize; // 2 for borders, 1 for header

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

    frame.render_widget(table, frame.area());

    // Render scrollbar if content exceeds visible area
    if !app.commits.is_empty() && app.commits.len() > available_height {
        let mut scrollbar_state =
            ScrollbarState::new(app.commits.len().saturating_sub(1)).position(app.selection_index);

        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalLeft)
            .begin_symbol(None)
            .end_symbol(None)
            .track_symbol(Some("│"));

        // Constrain scrollbar to the data area (skip top border + header, bottom border)
        let scrollbar_area = ratatui::layout::Rect {
            y: frame.area().y + 2,
            height: available_height as u16,
            ..frame.area()
        };
        frame.render_stateful_widget(scrollbar, scrollbar_area, &mut scrollbar_state);
    }
}
