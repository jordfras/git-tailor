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

use crate::app::AppState;

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
        let content = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("Commit: ", Style::default().fg(Color::Yellow)),
                Span::raw(&selected.oid),
            ]),
            Line::from(""),
            Line::from(vec![Span::styled(
                &selected.summary,
                Style::default().fg(Color::White),
            )]),
            Line::from(""),
            Line::from(Span::styled(
                "(Detailed metadata and diff",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                " will be in T064b-T064d)",
                Style::default().fg(Color::DarkGray),
            )),
        ];

        let paragraph = Paragraph::new(content);
        frame.render_widget(paragraph, content_area);
    }

    // Render footer
    let footer = Paragraph::new("").style(FOOTER_STYLE);
    frame.render_widget(footer, footer_area);
}
