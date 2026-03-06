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

// Help dialog view showing keybindings

use super::dialog::render_centered_dialog;
use crate::app::AppAction;
use crate::event::KeyCommand;

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    Frame,
};

/// Handle an action while in Help mode.
pub fn handle_key(action: KeyCommand, app: &mut crate::app::AppState) -> AppAction {
    match action {
        KeyCommand::Quit | KeyCommand::ShowHelp => {
            app.toggle_help();
            AppAction::Handled
        }
        _ => AppAction::Handled,
    }
}

/// Render the help dialog as a centered overlay.
pub fn render(frame: &mut Frame) {
    // Build help content first to calculate required size
    let help_lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            " Navigation",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("   ↑/↓, j/k  ", Style::default().fg(Color::Cyan)),
            Span::raw("Move selection up/down"),
        ]),
        Line::from(vec![
            Span::styled("   PgUp/PgDn ", Style::default().fg(Color::Cyan)),
            Span::raw("Move one page up/down"),
        ]),
        Line::from(vec![
            Span::styled("   ←/→       ", Style::default().fg(Color::Cyan)),
            Span::raw("Scroll fragmap left/right"),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            " Views",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("   Enter, i  ", Style::default().fg(Color::Cyan)),
            Span::raw("Toggle commit detail view"),
        ]),
        Line::from(vec![
            Span::styled("   s         ", Style::default().fg(Color::Cyan)),
            Span::raw("Split commit (choose strategy)"),
        ]),
        Line::from(vec![
            Span::styled("   r         ", Style::default().fg(Color::Cyan)),
            Span::raw("Reword commit message"),
        ]),
        Line::from(vec![
            Span::styled("   d         ", Style::default().fg(Color::Cyan)),
            Span::raw("Drop commit"),
        ]),
        Line::from(vec![
            Span::styled("   m         ", Style::default().fg(Color::Cyan)),
            Span::raw("Launch merge tool (during drop conflict)"),
        ]),
        Line::from(vec![
            Span::styled("   h         ", Style::default().fg(Color::Cyan)),
            Span::raw("Show this help dialog"),
        ]),
        Line::from(vec![
            Span::styled("   u         ", Style::default().fg(Color::Cyan)),
            Span::raw("Update commit list from HEAD"),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            " Other",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("   Esc, q    ", Style::default().fg(Color::Cyan)),
            Span::raw("Close dialog / Quit application"),
        ]),
        Line::from(""),
    ];

    render_centered_dialog(frame, " Help - Keybindings ", Color::White, 48, help_lines);
}
