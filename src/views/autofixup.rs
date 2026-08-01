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

// Bulk autofixup confirmation dialog

use super::dialog::{Dialog, DialogKind, TextRole, inner_width};
use super::list_nav::{self, ListNav};
use crate::app::{AppAction, AppMode, AppState, KeyCommand};
use crate::autofixup::{self, AutofixupGroup};
use ratatui::{
    Frame,
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

/// Handle an action while in AutofixupConfirm mode.
pub fn handle_confirm_key(action: KeyCommand, app: &mut AppState) -> AppAction {
    let pending = match &app.mode {
        AppMode::AutofixupConfirm(p) => p,
        _ => return AppAction::Handled,
    };
    let groups = autofixup::group_by_target(&pending.pairs);
    let mut cursor = pending.selected_group;

    match list_nav::handle_list_navigation(action, &mut cursor, groups.len(), groups.len(), false) {
        ListNav::Moved => {
            if let AppMode::AutofixupConfirm(pending) = &mut app.mode {
                pending.selected_group = cursor;
            }
            scroll_to_group(app, &groups, cursor);
            AppAction::Handled
        }
        ListNav::Confirmed => {
            if let AppMode::AutofixupConfirm(pending) =
                std::mem::replace(&mut app.mode, AppMode::CommitList)
            {
                AppAction::ExecuteAutofixup {
                    head_oid: pending.head_oid,
                    reference_oid: pending.reference_oid,
                    pairs: pending.pairs,
                    message_overrides: pending.message_overrides,
                }
            } else {
                AppAction::Handled
            }
        }
        ListNav::Cancelled => {
            app.cancel_autofixup_confirm();
            AppAction::Handled
        }
        ListNav::Help => {
            app.toggle_help();
            AppAction::Handled
        }
        ListNav::Unhandled => match action {
            KeyCommand::Reword => {
                let Some(group) = groups.get(cursor) else {
                    return AppAction::Handled;
                };
                AppAction::PrepareAutofixupEditMessage {
                    target_summary: group.target_summary.clone(),
                    template: autofixup::edit_template(group),
                }
            }
            _ => AppAction::Handled,
        },
    }
}

/// Scroll `dialog.offset` so the target group at `index` is visible.
/// Layout before the first group: blank, heading (blank/content/blank), the
/// hint line, then blank — 5 lines, matching `operation_select`'s own header
/// — followed by one line per group's target plus one per source underneath
/// it (assumes no long-summary wrapping, which is an acceptable approximation
/// for scroll-into-view purposes).
fn scroll_to_group(app: &mut AppState, groups: &[AutofixupGroup], index: usize) {
    const HEADER_LINES: usize = 5;
    let group_start: usize = groups[..index]
        .iter()
        .map(|g| 1 + g.sources.len())
        .sum::<usize>()
        + HEADER_LINES;
    let group_height = 1 + groups[index].sources.len();
    let vh = app.dialog.visible_height;
    if vh == 0 {
        return;
    }
    if group_start < app.dialog.offset {
        app.dialog.offset = group_start;
    } else if group_start + group_height > app.dialog.offset + vh {
        app.dialog.offset = group_start + group_height - vh;
    }
    app.dialog.clamp_offset();
}

/// Render the autofixup confirmation dialog as a centered overlay.
pub fn render_autofixup_confirm(app: &mut AppState, frame: &mut Frame) {
    let pending = match &app.mode {
        AppMode::AutofixupConfirm(p) => p,
        _ => return,
    };
    let groups = autofixup::group_by_target(&pending.pairs);
    let selected_group = pending.selected_group;
    let overrides = &pending.message_overrides;

    const PREFERRED_WIDTH: u16 = 70;
    let iw = inner_width(PREFERRED_WIDTH, frame.area().width);

    let mut dialog = Dialog::new(DialogKind::Confirm, app.colors)
        .heading(
            format!(
                "Squash {} commit(s) into {} target(s)?",
                pending.pairs.len(),
                groups.len()
            ),
            TextRole::Highlight,
        )
        .wrapped_styled(
            " \u{2191}/\u{2193} to select a target, r to edit its final message:",
            iw.saturating_sub(1),
            TextRole::Muted,
        )
        .blank();

    for (i, group) in groups.iter().enumerate() {
        dialog = dialog.push_line(target_line(app, group, i == selected_group, overrides));
        for source in &group.sources {
            dialog = dialog.wrapped_styled(
                &format!(
                    "    {} ({})",
                    source.source_summary,
                    source.source_oid.short()
                ),
                iw.saturating_sub(1),
                TextRole::Muted,
            );
        }
    }

    let (max_scroll, visible_height) = dialog
        .blank()
        .instructions(&[
            ("Enter", Color::Cyan, "Confirm"),
            ("r", Color::Cyan, "Edit message"),
            ("Esc", Color::Cyan, "Cancel"),
        ])
        .blank()
        .render(
            frame,
            "Confirm Autofixup",
            PREFERRED_WIDTH,
            app.dialog.offset,
        );
    app.dialog.set_bounds_clamped(max_scroll, visible_height);
}

fn target_line(
    app: &AppState,
    group: &AutofixupGroup,
    selected: bool,
    overrides: &std::collections::HashMap<String, String>,
) -> Line<'static> {
    let marker = if selected { "\u{25b8} " } else { "  " };
    let label_style = if selected {
        Style::default()
            .fg(app.colors.resolve(Color::Cyan))
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(app.colors.resolve(Color::White))
    };
    let mut spans = vec![
        Span::styled(marker.to_string(), label_style),
        Span::styled(group.target_summary.clone(), label_style),
        Span::raw(" ("),
        Span::styled(
            group.target_oid.short().to_string(),
            Style::default().fg(app.colors.resolve(TextRole::Key.color())),
        ),
        Span::raw(")"),
    ];
    if overrides.contains_key(&group.target_summary) {
        spans.push(Span::styled(
            " (edited)".to_string(),
            Style::default().fg(app.colors.resolve(Color::Yellow)),
        ));
    }
    Line::from(spans)
}
