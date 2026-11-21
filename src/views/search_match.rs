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

use std::ops::Range;

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

/// Split `spans` around the matches of `regex`, drawing them in
/// `highlight_style`. The pattern runs over the spans' joined text, so a match
/// may cross from one span into the next.
pub fn highlight_spans(
    spans: &[Span<'_>],
    regex: &Regex,
    highlight_style: Style,
) -> Vec<Span<'static>> {
    let text: String = spans.iter().map(|span| span.content.as_ref()).collect();
    let matches: Vec<Range<usize>> = regex
        .find_iter(&text)
        .map(|m| m.range())
        .filter(|range| !range.is_empty())
        .collect();
    let mut result = Vec::new();
    let mut span_start = 0;

    for span in spans {
        let span_end = span_start + span.content.len();
        let mut cut = span_start;
        for m in matches
            .iter()
            .filter(|m| m.start < span_end && m.end > span_start)
        {
            let start = m.start.max(span_start);
            let end = m.end.min(span_end);
            if start > cut {
                result.push(Span::styled(text[cut..start].to_string(), span.style));
            }
            result.push(Span::styled(text[start..end].to_string(), highlight_style));
            cut = end;
        }
        if cut < span_end || span_start == span_end {
            result.push(Span::styled(text[cut..span_end].to_string(), span.style));
        }
        span_start = span_end;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: Style = Style::new().fg(Color::Red);
    const B: Style = Style::new().fg(Color::Green);
    const C: Style = Style::new().fg(Color::Blue);
    const HIT: Style = Style::new().bg(Color::Yellow);

    fn highlighted(spans: &[(&str, Style)], pattern: &str) -> Vec<(String, Style)> {
        let spans: Vec<Span> = spans
            .iter()
            .map(|&(text, style)| Span::styled(text, style))
            .collect();
        highlight_spans(&spans, &Regex::new(pattern).unwrap(), HIT)
            .into_iter()
            .map(|span| (span.content.into_owned(), span.style))
            .collect()
    }

    fn owned(spans: &[(&str, Style)]) -> Vec<(String, Style)> {
        spans.iter().map(|&(t, s)| (t.to_string(), s)).collect()
    }

    #[test]
    fn a_match_inside_one_span_splits_only_that_span() {
        assert_eq!(
            highlighted(&[("ab", A), ("cxd", B)], "x"),
            owned(&[("ab", A), ("c", B), ("x", HIT), ("d", B)])
        );
    }

    #[test]
    fn a_match_across_three_spans_highlights_its_part_of_each() {
        assert_eq!(
            highlighted(&[("abc", A), ("d", B), ("efg", C)], "cde"),
            owned(&[("ab", A), ("c", HIT), ("d", HIT), ("e", HIT), ("fg", C)])
        );
    }

    #[test]
    fn several_matches_in_one_span_each_split_it() {
        assert_eq!(
            highlighted(&[("axbxc", A)], "x"),
            owned(&[("a", A), ("x", HIT), ("b", A), ("x", HIT), ("c", A)])
        );
    }

    #[test]
    fn an_empty_span_is_kept_between_matches() {
        assert_eq!(
            highlighted(&[("x", A), ("", B), ("x", C)], "x"),
            owned(&[("x", HIT), ("", B), ("x", HIT)])
        );
    }

    #[test]
    fn multibyte_text_splits_at_a_span_boundary() {
        assert_eq!(
            highlighted(&[("åä", A), ("öx", B)], "äö"),
            owned(&[("å", A), ("ä", HIT), ("ö", HIT), ("x", B)])
        );
    }

    #[test]
    fn without_a_match_the_spans_come_back_unchanged() {
        assert_eq!(
            highlighted(&[("ab", A), ("cd", B)], "z"),
            owned(&[("ab", A), ("cd", B)])
        );
    }

    #[test]
    fn no_spans_give_no_spans() {
        assert_eq!(highlighted(&[], "x"), owned(&[]));
    }
}
