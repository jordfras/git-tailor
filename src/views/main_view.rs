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
use crate::repo::GitRepo;
use crate::views;

/// Render the main view with split screen (commit list on left, detail on right).
pub fn render(git_repo: &impl GitRepo, app: &mut AppState, frame: &mut ratatui::Frame) {
    let area = frame.area();
    const BASE_SPLIT_X: i32 = 72; // SHA(10) + gap(1) + title(60) + gap(1)
    // MIN_LEFT: scrollbar(1) + SHA(10) + col-gap(1) + min-title(10) + panel-sep(1) = 23
    // This matches CommitList mode's minimum separator position so both modes
    // stop at the same column, and SHA is never obscured by the panel separator.
    const MIN_LEFT: i32 = 23;
    const MIN_RIGHT: i32 = 20;
    let max_offset = (area.width as i32 - BASE_SPLIT_X - MIN_RIGHT).max(0);
    let min_offset = (MIN_LEFT - BASE_SPLIT_X).min(0);
    let clamped_offset = (app.separator_offset as i32).clamp(min_offset, max_offset);
    app.separator_offset = clamped_offset as i16;
    let separator_x = ((BASE_SPLIT_X + clamped_offset) as u16).min(area.width);
    let left_width = separator_x.min(area.width);
    let right_width = area.width.saturating_sub(left_width);

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

        // Temporarily hide fragmap so commit list renders without it.
        // Also save/restore separator_offset: compute_layout writes it back
        // based on the sub-panel width, which would clobber the value that
        // render already clamped to the panel-boundary range.
        let saved_fragmap = app.fragmap.take();
        let saved_offset = app.separator_offset;
        views::commit_list::render_in_area(app, frame, left_area);
        app.fragmap = saved_fragmap;
        app.separator_offset = saved_offset;

        // Render separator between left and right
        let sep_height = area.height.saturating_sub(1); // exclude footer row
        let separator_spans: Vec<Line> = (0..sep_height)
            .map(|_| {
                Line::from(Span::styled(
                    "│",
                    Style::new().fg(Color::White).bg(Color::Blue),
                ))
            })
            .collect();
        let sep_area = Rect {
            x: left_area.x + left_width - 1,
            y: area.y,
            width: 1,
            height: sep_height,
        };
        frame.render_widget(Paragraph::new(separator_spans), sep_area);

        views::commit_detail::render(git_repo, frame, app, right_area);

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
