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

use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::AppState;
use crate::views;

/// Render the main view with split screen (commit list on left, detail on right).
pub fn render(app: &mut AppState, frame: &mut ratatui::Frame) {
    let area = frame.area();
    // Position the separator at the same column as the fragmap "│" in CommitList
    // mode, so switching to CommitDetail causes no visual movement.
    // Falls back to BASE_SPLIT_X when no fragmap clusters are visible.
    let (left_width, right_width) =
        if let Some(sep_x) = views::commit_list::compute_fragmap_sep_x(app, area) {
            // sep_x is the column of the "│"; right panel starts one column after.
            let lw = (sep_x + 1).min(area.width);
            (lw, area.width.saturating_sub(lw))
        } else {
            // No fragmap clusters (no commits loaded): give the right panel MIN_RIGHT cols.
            const MIN_RIGHT: u16 = 20;
            if area.width > MIN_RIGHT {
                let lw = area.width - MIN_RIGHT;
                (lw, MIN_RIGHT)
            } else {
                (0, area.width)
            }
        };

    if right_width > 0 {
        let left_area = Rect {
            x: area.x,
            y: area.y,
            width: left_width.saturating_sub(1), // exclude the separator column itself
            height: area.height,
        };
        let right_area = Rect {
            x: area.x + left_width,
            y: area.y,
            width: right_width,
            height: area.height,
        };

        // Suppress fragmap columns in the narrow left panel but keep the fragmap
        // for row coloring.  Also restore separator_offset: compute_layout writes
        // it back based on sub-panel width, which would clobber the value we
        // already clamped to the panel-boundary range.
        let saved_offset = app.separator_offset;
        views::commit_list::render_in_area_without_fragmap_cols(app, frame, left_area);
        app.separator_offset = saved_offset;

        // Render separator between left and right
        let sep_height = area.height.saturating_sub(1); // exclude footer row
        let sep_style = app
            .colors
            .resolve_style(Style::new().fg(Color::White).bg(Color::Blue));
        let separator_spans: Vec<Line> = (0..sep_height)
            .map(|_| Line::from(Span::styled("│", sep_style)))
            .collect();
        let sep_area = Rect {
            x: left_area.x + left_width - 1,
            y: area.y,
            width: 1,
            height: sep_height,
        };
        frame.render_widget(Paragraph::new(separator_spans), sep_area);

        views::commit_detail::render(frame, app, right_area);

        // Render footer at full terminal width so it isn't capped to the left panel.
        let footer_area = Rect {
            x: area.x,
            y: area.y + area.height.saturating_sub(1),
            width: area.width,
            height: 1,
        };
        views::commit_list::render_footer(frame, app, footer_area);
    } else {
        // Screen too narrow, just show commit list
        views::commit_list::render(app, frame);
    }
}
