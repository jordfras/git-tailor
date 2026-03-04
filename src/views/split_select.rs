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

use crate::app::{AppMode, AppState, SplitStrategy};
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

    let strategy_index = match app.mode {
        AppMode::SplitSelect { strategy_index } => strategy_index,
        _ => 0,
    };
    for (i, strategy) in SplitStrategy::ALL.iter().enumerate() {
        let selected = i == strategy_index;
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

/// Render the large-split confirmation dialog as a centered overlay.
pub fn render_split_confirm(app: &AppState, frame: &mut Frame) {
    let area = frame.area();
    let pending = match &app.mode {
        AppMode::SplitConfirm(p) => p,
        _ => return,
    };
    let strategy_name = match pending.strategy {
        crate::app::SplitStrategy::PerFile => "per file",
        crate::app::SplitStrategy::PerHunk => "per hunk",
        crate::app::SplitStrategy::PerHunkGroup => "per hunk group",
    };

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!(
                " This will create {} commits ({}).",
                pending.count, strategy_name
            ),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::raw(" Do you want to proceed?")),
        Line::from(""),
        Line::from(vec![
            Span::styled("Enter ", Style::default().fg(Color::Cyan)),
            Span::raw("Confirm   "),
            Span::styled("Esc ", Style::default().fg(Color::Cyan)),
            Span::raw("Cancel"),
        ])
        .alignment(Alignment::Center),
        Line::from(""),
    ];

    let dialog_width = 52u16.min(area.width.saturating_sub(4));
    let dialog_height = (lines.len() as u16 + 2).min(area.height.saturating_sub(2));
    let dialog_x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
    let dialog_y = area.y + (area.height.saturating_sub(dialog_height)) / 2;
    let dialog_area = Rect {
        x: dialog_x,
        y: dialog_y,
        width: dialog_width,
        height: dialog_height,
    };

    frame.render_widget(Clear, dialog_area);
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title(" Confirm Split ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Yellow))
                    .style(Style::default().bg(Color::Black)),
            )
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: false }),
        dialog_area,
    );
}

/// Word-wrap `text` to at most `width` display columns per line.
///
/// Breaks at the last space within the allowed width; falls back to a hard
/// break at `width` characters when no space is found. Always returns at
/// least one element.
fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 || text.is_empty() {
        return vec![text.to_string()];
    }
    let mut result = Vec::new();
    let mut remaining = text;
    while remaining.chars().count() > width {
        // byte offset of the character just past the width limit
        let byte_limit = remaining
            .char_indices()
            .nth(width)
            .map(|(i, _)| i)
            .unwrap_or(remaining.len());
        let break_at = remaining[..byte_limit]
            .rfind(' ')
            .filter(|&p| p > 0)
            .unwrap_or(byte_limit);
        result.push(remaining[..break_at].to_string());
        remaining = remaining[break_at..].trim_start_matches(' ');
    }
    result.push(remaining.to_string());
    result
}

/// Render the drop confirmation dialog as a centered overlay.
pub fn render_drop_confirm(app: &AppState, frame: &mut Frame) {
    let area = frame.area();
    let pending = match &app.mode {
        AppMode::DropConfirm(p) => p,
        _ => return,
    };

    let short_oid = if pending.commit_oid.len() >= 10 {
        &pending.commit_oid[..10]
    } else {
        &pending.commit_oid
    };

    let dialog_width = 60u16.min(area.width.saturating_sub(4));
    let inner_width = dialog_width.saturating_sub(2) as usize;

    // Pre-wrap the summary so that lines.len() reflects the real rendered
    // height and dialog_height is computed accurately.
    let summary_chunks = wrap_text(&pending.commit_summary, inner_width.saturating_sub(1));

    let mut lines: Vec<Line> = vec![
        Line::from(""),
        Line::from(Span::styled(
            " Drop this commit?",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            format!(" {short_oid}"),
            Style::default().fg(Color::Cyan),
        )),
    ];
    for chunk in &summary_chunks {
        lines.push(Line::from(Span::raw(format!(" {chunk}"))));
    }
    lines.push(Line::from(""));
    lines.push(
        Line::from(vec![
            Span::styled("Enter ", Style::default().fg(Color::Cyan)),
            Span::raw("Confirm   "),
            Span::styled("Esc ", Style::default().fg(Color::Cyan)),
            Span::raw("Cancel"),
        ])
        .alignment(Alignment::Center),
    );
    lines.push(Line::from(""));

    let dialog_height = (lines.len() as u16 + 2).min(area.height.saturating_sub(2));
    let dialog_x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
    let dialog_y = area.y + (area.height.saturating_sub(dialog_height)) / 2;
    let dialog_area = Rect {
        x: dialog_x,
        y: dialog_y,
        width: dialog_width,
        height: dialog_height,
    };

    frame.render_widget(Clear, dialog_area);
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title(" Confirm Drop ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Yellow))
                    .style(Style::default().bg(Color::Black)),
            )
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: false }),
        dialog_area,
    );
}

/// Render the drop-conflict resolution dialog as a centered overlay.
///
/// Shown when a drop operation hits a merge conflict. The user must resolve
/// the conflict in the working tree, then press Enter to continue or Esc to
/// abort the entire drop operation (even if this is not the first conflict).
pub fn render_drop_conflict(app: &AppState, frame: &mut Frame) {
    let area = frame.area();
    let state = match &app.mode {
        AppMode::DropConflict(s) => s,
        _ => return,
    };

    let short_oid = if state.conflicting_commit_oid.len() >= 10 {
        &state.conflicting_commit_oid[..10]
    } else {
        &state.conflicting_commit_oid
    };

    // Look up the commit summary from the loaded commit list so the user can
    // see which commit is conflicting without having to remember the OID.
    let commit_summary = app
        .commits
        .iter()
        .find(|c| c.oid == state.conflicting_commit_oid)
        .map(|c| c.summary.as_str())
        .unwrap_or("");

    let dialog_width = 62u16.min(area.width.saturating_sub(4));
    let inner_width = dialog_width.saturating_sub(2) as usize;

    let remaining = state.remaining_oids.len();

    let mut lines: Vec<Line> = vec![
        Line::from(""),
        Line::from(Span::styled(
            " Merge conflict during drop",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::raw(" Conflict in "),
            Span::styled(short_oid, Style::default().fg(Color::Cyan)),
        ]),
    ];

    // Show the summary of the conflicting commit, wrapped to fit the dialog.
    if !commit_summary.is_empty() {
        for chunk in wrap_text(commit_summary, inner_width.saturating_sub(1)) {
            lines.push(Line::from(Span::raw(format!(" {chunk}"))));
        }
    }

    if remaining > 0 {
        let note = format!(" ({remaining} commit(s) still to rebase after this)");
        for chunk in wrap_text(&note, inner_width) {
            lines.push(Line::from(Span::raw(chunk)));
        }
    }

    // List conflicting files so the user knows what to resolve.
    if !state.conflicting_files.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            " Conflicting files:",
            Style::default().fg(Color::Yellow),
        )));
        const MAX_FILES: usize = 5;
        let shown = state.conflicting_files.len().min(MAX_FILES);
        for path in &state.conflicting_files[..shown] {
            let truncated = if path.len() + 3 > inner_width {
                format!(" …{}", &path[path.len().saturating_sub(inner_width - 3)..])
            } else {
                format!(" {path}")
            };
            lines.push(Line::from(Span::styled(
                truncated,
                Style::default().fg(Color::Red),
            )));
        }
        let extra = state.conflicting_files.len().saturating_sub(MAX_FILES);
        if extra > 0 {
            lines.push(Line::from(Span::styled(
                format!(" ... {extra} more"),
                Style::default().fg(Color::DarkGray),
            )));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::raw(
        " Resolve conflicts in your working tree, then:",
    )));
    lines.push(Line::from(""));
    lines.push(
        Line::from(vec![
            Span::styled("Enter ", Style::default().fg(Color::Green)),
            Span::raw("Continue   "),
            Span::styled("Esc ", Style::default().fg(Color::Red)),
            Span::raw("Abort entire drop"),
        ])
        .alignment(Alignment::Center),
    );
    lines.push(Line::from(""));

    let dialog_height = (lines.len() as u16 + 2).min(area.height.saturating_sub(2));
    let dialog_x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
    let dialog_y = area.y + (area.height.saturating_sub(dialog_height)) / 2;
    let dialog_area = Rect {
        x: dialog_x,
        y: dialog_y,
        width: dialog_width,
        height: dialog_height,
    };

    frame.render_widget(Clear, dialog_area);
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title(" Drop Conflict ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Red))
                    .style(Style::default().bg(Color::Black)),
            )
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: false }),
        dialog_area,
    );
}
