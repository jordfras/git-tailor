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

// Split strategy selection dialog

use crate::app::{AppState, SplitStrategy};
use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

/// Render the split strategy selection dialog as a centered overlay.
pub fn render(app: &AppState, frame: &mut Frame) {
    let area = frame.area();

    let commit_summary = app
        .commits
        .get(app.selection_index)
        .map(|c| {
            let short_oid = if c.oid.len() > 10 {
                &c.oid[..10]
            } else {
                &c.oid
            };
            format!("{} {}", short_oid, c.summary)
        })
        .unwrap_or_default();

    // Truncate summary if too long for dialog
    let max_summary_len = 44;
    let display_summary = if commit_summary.len() > max_summary_len {
        format!("{}…", &commit_summary[..max_summary_len - 1])
    } else {
        commit_summary
    };

    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!(" {}", display_summary),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::DIM),
        )),
        Line::from(""),
        Line::from(Span::styled(
            " Choose split strategy:",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    for (i, strategy) in SplitStrategy::ALL.iter().enumerate() {
        let selected = i == app.split_strategy_index;
        let marker = if selected { "▸ " } else { "  " };
        let style = if selected {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };

        lines.push(Line::from(Span::styled(
            format!(" {}  {}", marker, strategy.label()),
            style,
        )));

        let desc_style = Style::default().fg(Color::DarkGray);
        lines.push(Line::from(Span::styled(
            format!("        {}", strategy.description()),
            desc_style,
        )));
        lines.push(Line::from(""));
    }

    lines.push(
        Line::from(vec![
            Span::styled("Enter ", Style::default().fg(Color::Cyan)),
            Span::raw("Select   "),
            Span::styled("Esc ", Style::default().fg(Color::Cyan)),
            Span::raw("Cancel"),
        ])
        .alignment(Alignment::Center),
    );
    lines.push(Line::from(""));

    let content_width = 50;
    let content_height = lines.len() as u16;
    let dialog_width = content_width.min(area.width.saturating_sub(4));
    let dialog_height = (content_height + 2).min(area.height.saturating_sub(2));

    let dialog_x = (area.width.saturating_sub(dialog_width)) / 2;
    let dialog_y = (area.height.saturating_sub(dialog_height)) / 2;

    let dialog_area = Rect {
        x: area.x + dialog_x,
        y: area.y + dialog_y,
        width: dialog_width,
        height: dialog_height,
    };

    frame.render_widget(Clear, dialog_area);

    let dialog = Paragraph::new(lines)
        .block(
            Block::default()
                .title(" Split Commit ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .style(Style::default().bg(Color::Black)),
        )
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: false });

    frame.render_widget(dialog, dialog_area);
}
