// Commit list view rendering

use crate::app::AppState;
use ratatui::{
    layout::Constraint,
    style::{Style, Stylize},
    widgets::{Block, Borders, Cell, Row, Table},
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

    let rows: Vec<Row> = app
        .commits
        .iter()
        .enumerate()
        .map(|(index, commit)| {
            let short_sha: String = commit.oid.chars().take(SHORT_SHA_LENGTH).collect();
            let row = Row::new(vec![
                Cell::from(short_sha),
                Cell::from(commit.summary.clone()),
            ]);

            if index == app.selection_index {
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
}
