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

// The one-row search bar: the query being typed and how many matches it has.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::SearchState;
use crate::views::palette::Colors;

/// Transient info about the search bar, computed during render.
pub enum SearchBarInfo {
    NoMatches,
    InvalidPattern,
    Matches { current: usize, total: usize },
}

/// Render the search bar widget into the given area, `tag` naming what the
/// search looks at when the view has a choice.
pub fn render<M>(
    frame: &mut Frame,
    search: &SearchState<M>,
    tag: Option<&str>,
    info: Option<SearchBarInfo>,
    colors: Colors,
    area: Rect,
) {
    let dark_gray = colors.resolve(Color::DarkGray);
    let red = colors.resolve(Color::Red);
    let mut bar_spans = Vec::new();
    if let Some(tag) = tag {
        bar_spans.push(Span::styled(
            format!("[{tag}] "),
            Style::default().fg(colors.resolve(Color::Cyan)),
        ));
    }
    bar_spans.extend([
        Span::styled("/", Style::default().fg(dark_gray)),
        Span::styled(
            search.query.clone(),
            Style::default().fg(colors.resolve(Color::White)),
        ),
    ]);
    if search.input_active {
        bar_spans.push(Span::styled("█", Style::default().fg(dark_gray)));
    }
    match info {
        Some(SearchBarInfo::NoMatches) => {
            bar_spans.push(Span::styled("  [no matches]", Style::default().fg(red)));
        }
        Some(SearchBarInfo::InvalidPattern) => {
            bar_spans.push(Span::styled(
                "  [invalid pattern]",
                Style::default().fg(red),
            ));
        }
        Some(SearchBarInfo::Matches { current, total }) => {
            bar_spans.push(Span::styled(
                format!("  [{}/{}]", current + 1, total),
                Style::default().fg(dark_gray),
            ));
        }
        None => {}
    }
    frame.render_widget(Paragraph::new(Line::from(bar_spans)), area);
}
