// Commit list view rendering

use crate::app::AppState;
use ratatui::{
    layout::Constraint,
    widgets::{Block, Borders, Cell, Row, Table},
    Frame,
};

/// Render the commit list view.
///
/// Takes application state and renders the commit list to the terminal frame.
pub fn render(_app: &AppState, frame: &mut Frame) {
    let header = Row::new(vec![Cell::from("SHA"), Cell::from("Title")]);

    let rows: Vec<Row> = vec![];
    let table = Table::new(rows, [Constraint::Length(10), Constraint::Min(20)])
        .header(header)
        .block(Block::default().borders(Borders::ALL).title("Commits"));

    frame.render_widget(table, frame.area());
}
