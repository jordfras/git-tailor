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
// How a search match is drawn, shared by every view that searches.

use ratatui::{
    style::{Color, Style},
    text::Span,
};
use regex::Regex;

use crate::views::palette::Colors;

/// The style of matched text; the current match stands out from the rest.
pub fn match_style(is_current: bool, colors: Colors) -> Style {
    let bg = if is_current {
        Color::LightCyan
    } else {
        Color::Yellow
    };
    colors.resolve_style(Style::new().fg(Color::Black).bg(bg))
}

/// Split `span` around the matches of `regex`, drawing them in
/// `highlight_style`.
pub fn highlight_matches(
    span: &Span<'_>,
    regex: &Regex,
    highlight_style: Style,
) -> Vec<Span<'static>> {
    let text = span.content.as_ref();
    let style = span.style;
    let mut result = Vec::new();
    let mut last_end = 0;

    for m in regex.find_iter(text) {
        if m.start() > last_end {
            result.push(Span::styled(text[last_end..m.start()].to_string(), style));
        }
        result.push(Span::styled(
            text[m.start()..m.end()].to_string(),
            highlight_style,
        ));
        last_end = m.end();
    }
    if last_end < text.len() {
        result.push(Span::styled(text[last_end..].to_string(), style));
    }
    if result.is_empty() {
        result.push(Span::styled(text.to_string(), style));
    }
    result
}
