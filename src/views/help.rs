// Help dialog view showing keybindings

use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

/// Render the help dialog as a centered overlay.
pub fn render(frame: &mut Frame) {
    let area = frame.area();

    // Build help content first to calculate required size
    let help_lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "Navigation",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("  ↑/↓       ", Style::default().fg(Color::Cyan)),
            Span::raw("Move selection up/down"),
        ]),
        Line::from(vec![
            Span::styled("  PgUp/PgDn ", Style::default().fg(Color::Cyan)),
            Span::raw("Move one page up/down"),
        ]),
        Line::from(vec![
            Span::styled("  ←/→       ", Style::default().fg(Color::Cyan)),
            Span::raw("Scroll fragmap left/right"),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Views",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("  i         ", Style::default().fg(Color::Cyan)),
            Span::raw("Toggle commit detail view"),
        ]),
        Line::from(vec![
            Span::styled("  h         ", Style::default().fg(Color::Cyan)),
            Span::raw("Show this help dialog"),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Other",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Esc       ", Style::default().fg(Color::Cyan)),
            Span::raw("Close dialog / Quit application"),
        ]),
        Line::from(""),
    ];

    // Calculate dialog size based on content
    let content_width = 48; // Longest line + padding
    let content_height = help_lines.len() as u16;
    let dialog_width = content_width.min(area.width.saturating_sub(4));
    let dialog_height = (content_height + 2).min(area.height.saturating_sub(2)); // +2 for borders

    // Center the dialog
    let dialog_x = (area.width.saturating_sub(dialog_width)) / 2;
    let dialog_y = (area.height.saturating_sub(dialog_height)) / 2;

    let dialog_area = Rect {
        x: area.x + dialog_x,
        y: area.y + dialog_y,
        width: dialog_width,
        height: dialog_height,
    };

    // Clear the background to hide underlying content
    frame.render_widget(Clear, dialog_area);

    let help_text = Paragraph::new(help_lines)
        .block(
            Block::default()
                .title(" Help - Keybindings ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::White))
                .style(Style::default().bg(Color::Black)),
        )
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: false });

    frame.render_widget(help_text, dialog_area);
}
