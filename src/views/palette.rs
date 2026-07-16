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

//! Colour palette selection (`--colors`).
//!
//! By default (`Terminal`) the UI uses the terminal's own ANSI colours, so it
//! adopts the user's theme. A built-in palette (currently `Campbell`, Windows
//! Terminal's default scheme) instead resolves every ANSI colour to a fixed RGB
//! value, reproducing that scheme's look on *any* terminal — including light or
//! pastel ones, where the UI (matrix, diff, bars) is otherwise unreadable
//! because it is designed for a dark background.
//!
//! Views resolve their colours through the active [`Colors`] as they render
//! (Terminal is the identity), and `main` paints a base background with
//! [`Colors::base_style`] so unstyled cells adopt the palette's background.

use clap::ValueEnum;
use ratatui::style::{Color, Style};

/// Which colour palette to render with (`--colors` / `GT_COLORS`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum Colors {
    /// Use the terminal's own colours (adopts the user's theme). The default.
    #[default]
    Terminal,
    /// The Windows Terminal "Campbell" scheme, applied on any terminal.
    Campbell,
}

impl Colors {
    /// Resolve a colour for the active palette. `Terminal` is the identity;
    /// `Campbell` maps each ANSI-named colour to its Campbell RGB and leaves
    /// explicit `Rgb`/`Indexed` (and `Reset`) colours untouched.
    pub fn resolve(self, color: Color) -> Color {
        match self {
            Colors::Terminal => color,
            Colors::Campbell => campbell(color),
        }
    }

    /// Resolve both colours of a style (modifiers/underline untouched). Handy for
    /// styles built elsewhere, e.g. the fragmap theme's output.
    pub fn resolve_style(self, style: Style) -> Style {
        Style {
            fg: style.fg.map(|c| self.resolve(c)),
            bg: style.bg.map(|c| self.resolve(c)),
            ..style
        }
    }

    /// Base style for the whole frame: nothing for `Terminal` (keep the
    /// terminal's own background), the scheme's fg/bg for a built-in palette so
    /// every cell that sets no background adopts it.
    pub fn base_style(self) -> Style {
        match self {
            Colors::Terminal => Style::default(),
            Colors::Campbell => Style::new()
                .fg(Color::Rgb(0xcc, 0xcc, 0xcc))
                .bg(Color::Rgb(0x0c, 0x0c, 0x0c)),
        }
    }

    /// The RGB the terminal's *default* background should be set to (via OSC 11)
    /// while this palette is active, so the terminal's window padding around the
    /// text grid matches the UI. `None` for `Terminal` — leave the user's
    /// terminal background untouched.
    pub fn terminal_background(self) -> Option<(u8, u8, u8)> {
        match self {
            Colors::Terminal => None,
            Colors::Campbell => Some((0x0c, 0x0c, 0x0c)),
        }
    }
}

/// The Campbell RGB for a ratatui colour (mirrors `examples/gen_screenshot.rs`).
/// `Reset`, `Rgb`, and `Indexed` are returned unchanged — `Reset` is covered by
/// the base background, and explicit colours are already specific.
fn campbell(color: Color) -> Color {
    let [r, g, b] = match color {
        Color::Black => [0x0c, 0x0c, 0x0c],
        Color::Red => [0xc5, 0x0f, 0x1f],
        Color::Green => [0x13, 0xa1, 0x0e],
        Color::Yellow => [0xc1, 0x9c, 0x00],
        Color::Blue => [0x00, 0x37, 0xda],
        Color::Magenta => [0x88, 0x17, 0x98],
        Color::Cyan => [0x3a, 0x96, 0xdd],
        Color::Gray => [0xcc, 0xcc, 0xcc],
        Color::DarkGray => [0x76, 0x76, 0x76],
        Color::LightRed => [0xe7, 0x48, 0x56],
        Color::LightGreen => [0x16, 0xc6, 0x0c],
        Color::LightYellow => [0xf9, 0xf1, 0xa5],
        Color::LightBlue => [0x3b, 0x78, 0xff],
        Color::LightMagenta => [0xb4, 0x00, 0x9e],
        Color::LightCyan => [0x61, 0xd6, 0xd6],
        Color::White => [0xf2, 0xf2, 0xf2],
        // Already specific, or handled by the base background.
        other => return other,
    };
    Color::Rgb(r, g, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_is_the_identity() {
        assert_eq!(Colors::Terminal.resolve(Color::Green), Color::Green);
        assert_eq!(Colors::Terminal.resolve(Color::Reset), Color::Reset);
        assert_eq!(Colors::Terminal.base_style(), Style::default());
    }

    #[test]
    fn campbell_maps_ansi_to_rgb_and_leaves_the_rest() {
        assert_eq!(
            Colors::Campbell.resolve(Color::Green),
            Color::Rgb(0x13, 0xa1, 0x0e)
        );
        assert_eq!(
            Colors::Campbell.resolve(Color::White),
            Color::Rgb(0xf2, 0xf2, 0xf2)
        );
        // Explicit RGB and Reset pass through.
        let rgb = Color::Rgb(1, 2, 3);
        assert_eq!(Colors::Campbell.resolve(rgb), rgb);
        assert_eq!(Colors::Campbell.resolve(Color::Reset), Color::Reset);
    }

    #[test]
    fn campbell_base_style_sets_scheme_fg_and_bg() {
        let base = Colors::Campbell.base_style();
        assert_eq!(base.fg, Some(Color::Rgb(0xcc, 0xcc, 0xcc)));
        assert_eq!(base.bg, Some(Color::Rgb(0x0c, 0x0c, 0x0c)));
    }

    #[test]
    fn resolve_style_maps_both_colours_and_keeps_modifiers() {
        use ratatui::style::Modifier;
        let s = Style::new()
            .fg(Color::White)
            .bg(Color::Green)
            .add_modifier(Modifier::BOLD);
        let r = Colors::Campbell.resolve_style(s);
        assert_eq!(r.fg, Some(Color::Rgb(0xf2, 0xf2, 0xf2)));
        assert_eq!(r.bg, Some(Color::Rgb(0x13, 0xa1, 0x0e)));
        assert!(r.add_modifier.contains(Modifier::BOLD));
    }
}
