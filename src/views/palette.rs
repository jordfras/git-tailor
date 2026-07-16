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

//! Color palette selection (`--palette`).
//!
//! By default (`Terminal`) the UI uses the terminal's own ANSI colors, so it
//! adopts the user's theme. A fixed [`Scheme`] instead resolves every ANSI
//! color to a specific RGB value, reproducing that scheme's look on *any*
//! terminal — including light or pastel ones, where the UI (matrix, diff, bars)
//! is otherwise unreadable because it is designed for a dark background.
//!
//! Two schemes are built in ([`Scheme::CAMPBELL`], [`Scheme::DARK_PLUS`]), and a
//! custom one can be loaded from a Windows Terminal color-scheme JSON file via
//! [`Scheme::from_wt_json`].
//!
//! Views resolve their colors through the active [`Colors`] as they render
//! (Terminal is the identity), and `main` paints a base background with
//! [`Colors::base_style`] so unstyled cells adopt the palette's background.

use anyhow::{Context, Result, bail};
use ratatui::style::{Color, Style};
use serde::Deserialize;

/// Which color palette to render with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Colors {
    /// Use the terminal's own colors (adopts the user's theme). The default.
    #[default]
    Terminal,
    /// A fixed scheme (built-in or user-supplied), applied on any terminal.
    Fixed(Scheme),
}

impl Colors {
    /// Resolve a color for the active palette. `Terminal` is the identity; a
    /// `Fixed` scheme maps each ANSI-named color to its RGB and leaves explicit
    /// `Rgb`/`Indexed` (and `Reset`) colors untouched.
    pub fn resolve(self, color: Color) -> Color {
        match self {
            Colors::Terminal => color,
            Colors::Fixed(scheme) => scheme.resolve(color),
        }
    }

    /// Resolve both colors of a style (modifiers/underline untouched). Handy for
    /// styles built elsewhere, e.g. the fragmap theme's output.
    pub fn resolve_style(self, style: Style) -> Style {
        Style {
            fg: style.fg.map(|c| self.resolve(c)),
            bg: style.bg.map(|c| self.resolve(c)),
            ..style
        }
    }

    /// Base style for the whole frame: nothing for `Terminal` (keep the
    /// terminal's own background), the scheme's fg/bg for a `Fixed` palette so
    /// every cell that sets no background adopts it.
    pub fn base_style(self) -> Style {
        match self {
            Colors::Terminal => Style::default(),
            Colors::Fixed(scheme) => Style::new().fg(rgb(scheme.fg)).bg(rgb(scheme.bg)),
        }
    }

    /// The RGB the terminal's *default* background should be set to (via OSC 11)
    /// while this palette is active, so the terminal's window padding around the
    /// text grid matches the UI. `None` for `Terminal` — leave the user's
    /// terminal background untouched.
    pub fn terminal_background(self) -> Option<(u8, u8, u8)> {
        match self {
            Colors::Terminal => None,
            Colors::Fixed(scheme) => Some(scheme.bg),
        }
    }
}

/// A fully-resolved color scheme: an RGB for each of the 16 ANSI slots plus a
/// foreground and background. Built-ins are `const`; custom schemes are parsed
/// from Windows Terminal color-scheme JSON with [`Scheme::from_wt_json`].
///
/// `ansi` is indexed by ANSI color number: 0–7 are the normal colors
/// (black, red, green, yellow, blue, magenta, cyan, white) and 8–15 their
/// bright variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scheme {
    fg: (u8, u8, u8),
    bg: (u8, u8, u8),
    ansi: [(u8, u8, u8); 16],
}

impl Scheme {
    /// Windows Terminal's default "Campbell" scheme.
    pub const CAMPBELL: Scheme = Scheme {
        fg: (0xcc, 0xcc, 0xcc),
        bg: (0x0c, 0x0c, 0x0c),
        ansi: [
            (0x0c, 0x0c, 0x0c), // black
            (0xc5, 0x0f, 0x1f), // red
            (0x13, 0xa1, 0x0e), // green
            (0xc1, 0x9c, 0x00), // yellow
            (0x00, 0x37, 0xda), // blue
            (0x88, 0x17, 0x98), // magenta
            (0x3a, 0x96, 0xdd), // cyan
            (0xcc, 0xcc, 0xcc), // white
            (0x76, 0x76, 0x76), // bright black
            (0xe7, 0x48, 0x56), // bright red
            (0x16, 0xc6, 0x0c), // bright green
            (0xf9, 0xf1, 0xa5), // bright yellow
            (0x3b, 0x78, 0xff), // bright blue
            (0xb4, 0x00, 0x9e), // bright magenta
            (0x61, 0xd6, 0xd6), // bright cyan
            (0xf2, 0xf2, 0xf2), // bright white
        ],
    };

    /// Windows Terminal's "Dark+" scheme — softer than Campbell, with a lighter
    /// `#1e1e1e` background and slightly less-saturated accents.
    pub const DARK_PLUS: Scheme = Scheme {
        fg: (0xcc, 0xcc, 0xcc),
        bg: (0x1e, 0x1e, 0x1e),
        ansi: [
            (0x00, 0x00, 0x00), // black
            (0xcd, 0x31, 0x31), // red
            (0x0d, 0xbc, 0x79), // green
            (0xe5, 0xe5, 0x10), // yellow
            (0x24, 0x72, 0xc8), // blue
            (0xbc, 0x3f, 0xbc), // magenta
            (0x11, 0xa8, 0xcd), // cyan
            (0xe5, 0xe5, 0xe5), // white
            (0x66, 0x66, 0x66), // bright black
            (0xf1, 0x4c, 0x4c), // bright red
            (0x23, 0xd1, 0x8b), // bright green
            (0xf5, 0xf5, 0x43), // bright yellow
            (0x3b, 0x8e, 0xea), // bright blue
            (0xd6, 0x70, 0xd6), // bright magenta
            (0x29, 0xb8, 0xdb), // bright cyan
            (0xe5, 0xe5, 0xe5), // bright white
        ],
    };

    /// This scheme's default foreground color, as an RGB triple.
    pub fn foreground(self) -> (u8, u8, u8) {
        self.fg
    }

    /// This scheme's default background color, as an RGB triple.
    pub fn background(self) -> (u8, u8, u8) {
        self.bg
    }

    /// Resolve one ratatui color against this scheme. ANSI-named colors map to
    /// their RGB; everything else (explicit `Rgb`/`Indexed`, `Reset`) passes
    /// through — `Reset` is covered by the base background.
    pub fn resolve(self, color: Color) -> Color {
        match ansi_index(color) {
            Some(i) => rgb(self.ansi[i]),
            None => color,
        }
    }

    /// Parse a [Windows Terminal color-scheme] JSON object. Extra fields (name,
    /// `cursorColor`, `selectionBackground`, …) are ignored, so any scheme
    /// exported in this widely-used format works as-is.
    ///
    /// [Windows Terminal color-scheme]: https://learn.microsoft.com/windows/terminal/customize-settings/color-schemes
    pub fn from_wt_json(json: &str) -> Result<Scheme> {
        let raw: WtScheme = serde_json::from_str(json)
            .context("color scheme is not valid Windows Terminal scheme JSON")?;
        Ok(Scheme {
            fg: parse_hex(&raw.foreground)?,
            bg: parse_hex(&raw.background)?,
            ansi: [
                parse_hex(&raw.black)?,
                parse_hex(&raw.red)?,
                parse_hex(&raw.green)?,
                parse_hex(&raw.yellow)?,
                parse_hex(&raw.blue)?,
                parse_hex(&raw.purple)?,
                parse_hex(&raw.cyan)?,
                parse_hex(&raw.white)?,
                parse_hex(&raw.bright_black)?,
                parse_hex(&raw.bright_red)?,
                parse_hex(&raw.bright_green)?,
                parse_hex(&raw.bright_yellow)?,
                parse_hex(&raw.bright_blue)?,
                parse_hex(&raw.bright_purple)?,
                parse_hex(&raw.bright_cyan)?,
                parse_hex(&raw.bright_white)?,
            ],
        })
    }
}

/// The ANSI color number (0–15) a ratatui color occupies, or `None` for colors
/// that are already specific (`Rgb`/`Indexed`) or that the base background
/// covers (`Reset`). ratatui names `Gray`/`White` for ANSI 7/15 and
/// `DarkGray`/bright-`Light*` for the 8–15 bright range.
fn ansi_index(color: Color) -> Option<usize> {
    Some(match color {
        Color::Black => 0,
        Color::Red => 1,
        Color::Green => 2,
        Color::Yellow => 3,
        Color::Blue => 4,
        Color::Magenta => 5,
        Color::Cyan => 6,
        Color::Gray => 7,
        Color::DarkGray => 8,
        Color::LightRed => 9,
        Color::LightGreen => 10,
        Color::LightYellow => 11,
        Color::LightBlue => 12,
        Color::LightMagenta => 13,
        Color::LightCyan => 14,
        Color::White => 15,
        _ => return None,
    })
}

fn rgb((r, g, b): (u8, u8, u8)) -> Color {
    Color::Rgb(r, g, b)
}

/// Parse a `#rrggbb` (or `rrggbb`) hex color into an RGB triple.
fn parse_hex(s: &str) -> Result<(u8, u8, u8)> {
    let h = s.strip_prefix('#').unwrap_or(s);
    if h.len() != 6 || !h.is_ascii() {
        bail!("invalid hex color {s:?}, expected #rrggbb");
    }
    let component = |range: std::ops::Range<usize>| {
        u8::from_str_radix(&h[range], 16).map_err(|_| anyhow::anyhow!("invalid hex color {s:?}"))
    };
    Ok((component(0..2)?, component(2..4)?, component(4..6)?))
}

/// The subset of Windows Terminal's color-scheme JSON we consume. Unknown keys
/// are ignored; the 16 ANSI colors plus fg/bg are required.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WtScheme {
    background: String,
    foreground: String,
    black: String,
    red: String,
    green: String,
    yellow: String,
    blue: String,
    purple: String,
    cyan: String,
    white: String,
    bright_black: String,
    bright_red: String,
    bright_green: String,
    bright_yellow: String,
    bright_blue: String,
    bright_purple: String,
    bright_cyan: String,
    bright_white: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_is_the_identity() {
        assert_eq!(Colors::Terminal.resolve(Color::Green), Color::Green);
        assert_eq!(Colors::Terminal.resolve(Color::Reset), Color::Reset);
        assert_eq!(Colors::Terminal.base_style(), Style::default());
        assert_eq!(Colors::Terminal.terminal_background(), None);
    }

    #[test]
    fn campbell_maps_ansi_to_rgb_and_leaves_the_rest() {
        let campbell = Colors::Fixed(Scheme::CAMPBELL);
        assert_eq!(campbell.resolve(Color::Green), Color::Rgb(0x13, 0xa1, 0x0e));
        assert_eq!(campbell.resolve(Color::White), Color::Rgb(0xf2, 0xf2, 0xf2));
        // Explicit RGB and Reset pass through.
        let explicit = Color::Rgb(1, 2, 3);
        assert_eq!(campbell.resolve(explicit), explicit);
        assert_eq!(campbell.resolve(Color::Reset), Color::Reset);
        // Foreground/background accessors expose the scheme's own fg/bg.
        assert_eq!(Scheme::CAMPBELL.foreground(), (0xcc, 0xcc, 0xcc));
        assert_eq!(Scheme::CAMPBELL.background(), (0x0c, 0x0c, 0x0c));
    }

    #[test]
    fn dark_plus_uses_the_softer_background() {
        let dark_plus = Colors::Fixed(Scheme::DARK_PLUS);
        assert_eq!(
            dark_plus.resolve(Color::Green),
            Color::Rgb(0x0d, 0xbc, 0x79)
        );
        // Normal white (Gray) and bright white (White) match in Dark+.
        assert_eq!(dark_plus.resolve(Color::Gray), Color::Rgb(0xe5, 0xe5, 0xe5));
        assert_eq!(
            dark_plus.resolve(Color::White),
            Color::Rgb(0xe5, 0xe5, 0xe5)
        );
        let base = dark_plus.base_style();
        assert_eq!(base.fg, Some(Color::Rgb(0xcc, 0xcc, 0xcc)));
        assert_eq!(base.bg, Some(Color::Rgb(0x1e, 0x1e, 0x1e)));
        assert_eq!(dark_plus.terminal_background(), Some((0x1e, 0x1e, 0x1e)));
    }

    #[test]
    fn resolve_style_maps_both_colors_and_keeps_modifiers() {
        use ratatui::style::Modifier;
        let s = Style::new()
            .fg(Color::White)
            .bg(Color::Green)
            .add_modifier(Modifier::BOLD);
        let r = Colors::Fixed(Scheme::CAMPBELL).resolve_style(s);
        assert_eq!(r.fg, Some(Color::Rgb(0xf2, 0xf2, 0xf2)));
        assert_eq!(r.bg, Some(Color::Rgb(0x13, 0xa1, 0x0e)));
        assert!(r.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn from_wt_json_parses_a_scheme_and_ignores_extra_fields() {
        // A trimmed Campbell export, with fields WT includes that we ignore.
        let json = r##"{
            "name": "Campbell",
            "cursorColor": "#FFFFFF",
            "selectionBackground": "#FFFFFF",
            "background": "#0C0C0C", "foreground": "#CCCCCC",
            "black": "#0C0C0C", "red": "#C50F1F", "green": "#13A10E",
            "yellow": "#C19C00", "blue": "#0037DA", "purple": "#881798",
            "cyan": "#3A96DD", "white": "#CCCCCC",
            "brightBlack": "#767676", "brightRed": "#E74856",
            "brightGreen": "#16C60C", "brightYellow": "#F9F1A5",
            "brightBlue": "#3B78FF", "brightPurple": "#B4009E",
            "brightCyan": "#61D6D6", "brightWhite": "#F2F2F2"
        }"##;
        let scheme = Scheme::from_wt_json(json).unwrap();
        assert_eq!(scheme, Scheme::CAMPBELL);
    }

    #[test]
    fn from_wt_json_rejects_bad_hex_and_missing_fields() {
        // Missing the whole set of required color keys.
        assert!(Scheme::from_wt_json(r##"{"name": "x"}"##).is_err());
        // Malformed hex in one field.
        let mut bad: serde_json::Value =
            serde_json::from_str(VALID_MINIMAL).expect("fixture parses");
        bad["red"] = serde_json::json!("#zzzzzz");
        assert!(Scheme::from_wt_json(&bad.to_string()).is_err());
    }

    #[test]
    fn parse_hex_accepts_with_or_without_hash() {
        assert_eq!(parse_hex("#0C0C0C").unwrap(), (0x0c, 0x0c, 0x0c));
        assert_eq!(parse_hex("0c0c0c").unwrap(), (0x0c, 0x0c, 0x0c));
        assert!(parse_hex("#fff").is_err());
        assert!(parse_hex("#gggggg").is_err());
    }

    const VALID_MINIMAL: &str = r##"{
        "background": "#0C0C0C", "foreground": "#CCCCCC",
        "black": "#0C0C0C", "red": "#C50F1F", "green": "#13A10E",
        "yellow": "#C19C00", "blue": "#0037DA", "purple": "#881798",
        "cyan": "#3A96DD", "white": "#CCCCCC",
        "brightBlack": "#767676", "brightRed": "#E74856",
        "brightGreen": "#16C60C", "brightYellow": "#F9F1A5",
        "brightBlue": "#3B78FF", "brightPurple": "#B4009E",
        "brightCyan": "#61D6D6", "brightWhite": "#F2F2F2"
    }"##;
}
