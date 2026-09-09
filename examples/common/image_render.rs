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

//! Rasterizes a rendered ratatui buffer to a PNG, resolving colors through the
//! same `views::palette::Scheme` the live `--palette` option uses.
//!
//! Kept beside `synthetic_repo` in `examples/common/` so `gen_screenshots`
//! stays about shots rather than about fonts and pixels. Not an example in its
//! own right: Cargo only treats top-level `examples/*.rs` as binaries, so a
//! file in this subdirectory is included with `#[path]` instead.

use std::path::Path;

use anyhow::Result;
use fontdue::Font;
use git_tailor::views::palette::Scheme;
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier};

/// Bundled monospace font with full Block Elements / Box Drawing coverage
/// (the matrix uses `█` and `│`). Vendored under `examples/assets/`.
const FONT_BYTES: &[u8] = include_bytes!("../assets/DejaVuSansMono.ttf");

/// Font size in pixels. Larger = higher-resolution screenshot.
const FONT_PX: f32 = 22.0;
/// Outer padding around the terminal grid, in pixels.
const MARGIN: u32 = 14;

pub fn buffer_to_png(buffer: &Buffer, out: &Path, scheme: Scheme) -> Result<()> {
    let default_bg = rgb(scheme.background());
    let font = Font::from_bytes(FONT_BYTES, fontdue::FontSettings::default())
        .map_err(|e| anyhow::anyhow!("loading font: {e}"))?;

    let lm = font
        .horizontal_line_metrics(FONT_PX)
        .ok_or_else(|| anyhow::anyhow!("font has no horizontal line metrics"))?;
    let cell_w = font.metrics('M', FONT_PX).advance_width.round().max(1.0) as u32;
    let ascent = lm.ascent.round();
    let cell_h = (lm.ascent - lm.descent).round().max(1.0) as u32;
    let baseline = ascent as i32;

    let area = buffer.area;
    let img_w = MARGIN * 2 + cell_w * area.width as u32;
    let img_h = MARGIN * 2 + cell_h * area.height as u32;

    let mut img = image::RgbImage::from_pixel(img_w, img_h, image::Rgb(default_bg));

    for y in 0..area.height {
        for x in 0..area.width {
            let cell = &buffer[(x, y)];
            let (fg, bg) = resolve_colors(cell.fg, cell.bg, cell.modifier, scheme);

            let px0 = MARGIN + x as u32 * cell_w;
            let py0 = MARGIN + y as u32 * cell_h;
            fill_rect(&mut img, px0, py0, cell_w, cell_h, bg);

            let symbol = cell.symbol();
            let Some(ch) = symbol.chars().next() else {
                continue;
            };
            if ch == ' ' {
                continue;
            }

            let (m, bitmap) = font.rasterize(ch, FONT_PX);
            if m.width == 0 || m.height == 0 {
                continue;
            }
            let gx0 = px0 as i32 + m.xmin;
            let gy0 = py0 as i32 + baseline - m.height as i32 - m.ymin;
            blit_glyph(&mut img, &bitmap, m.width, m.height, gx0, gy0, fg);
        }
    }

    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    img.save(out)?;
    Ok(())
}

/// Resolve a cell's foreground/background to concrete RGB, applying the
/// REVERSED and DIM modifiers (BOLD is ignored — the font has one weight).
fn resolve_colors(fg: Color, bg: Color, modifier: Modifier, scheme: Scheme) -> ([u8; 3], [u8; 3]) {
    let mut fg = to_rgb(fg, true, scheme);
    let mut bg = to_rgb(bg, false, scheme);
    if modifier.contains(Modifier::REVERSED) {
        std::mem::swap(&mut fg, &mut bg);
    }
    if modifier.contains(Modifier::DIM) {
        fg = blend(fg, bg, 0.5);
    }
    (fg, bg)
}

fn fill_rect(img: &mut image::RgbImage, x0: u32, y0: u32, w: u32, h: u32, color: [u8; 3]) {
    let px = image::Rgb(color);
    for yy in y0..(y0 + h).min(img.height()) {
        for xx in x0..(x0 + w).min(img.width()) {
            img.put_pixel(xx, yy, px);
        }
    }
}

/// Alpha-composite a coverage bitmap (`fg` over the existing background).
fn blit_glyph(
    img: &mut image::RgbImage,
    bitmap: &[u8],
    w: usize,
    h: usize,
    x0: i32,
    y0: i32,
    fg: [u8; 3],
) {
    for row in 0..h {
        for col in 0..w {
            let coverage = bitmap[row * w + col];
            if coverage == 0 {
                continue;
            }
            let px = x0 + col as i32;
            let py = y0 + row as i32;
            if px < 0 || py < 0 || px as u32 >= img.width() || py as u32 >= img.height() {
                continue;
            }
            let dst = img.get_pixel(px as u32, py as u32).0;
            let out = blend(fg, dst, coverage as f32 / 255.0);
            img.put_pixel(px as u32, py as u32, image::Rgb(out));
        }
    }
}

/// Linear blend `a*t + b*(1-t)`.
fn blend(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    let t = t.clamp(0.0, 1.0);
    [
        (a[0] as f32 * t + b[0] as f32 * (1.0 - t)).round() as u8,
        (a[1] as f32 * t + b[1] as f32 * (1.0 - t)).round() as u8,
        (a[2] as f32 * t + b[2] as f32 * (1.0 - t)).round() as u8,
    ]
}

/// Resolve a ratatui color to concrete RGB, using the shared palette
/// [`Scheme`] for the named ANSI slots and the scheme's own fg/bg for
/// `Reset`. This drives the same mapping the live TUI uses (`--palette`), so
/// the screenshot's colors can never drift from it.
fn to_rgb(color: Color, is_fg: bool, scheme: Scheme) -> [u8; 3] {
    match color {
        Color::Reset => rgb(if is_fg {
            scheme.foreground()
        } else {
            scheme.background()
        }),
        Color::Rgb(r, g, b) => [r, g, b],
        // The UI never emits indexed colors; handled defensively via the
        // standard xterm-256 palette (system colors mirror the scheme).
        Color::Indexed(i) => xterm256(i),
        // Named ANSI colors resolve through the scheme (yields `Rgb`).
        named => match scheme.resolve(named) {
            Color::Rgb(r, g, b) => [r, g, b],
            _ => rgb(if is_fg {
                scheme.foreground()
            } else {
                scheme.background()
            }),
        },
    }
}

fn rgb((r, g, b): (u8, u8, u8)) -> [u8; 3] {
    [r, g, b]
}

/// Map an xterm 256-color index to RGB (6×6×6 cube + grayscale ramp; the
/// 0–15 system slots fall back to a neutral gray, as the UI never uses them).
fn xterm256(i: u8) -> [u8; 3] {
    match i {
        0..=15 => [0xcc, 0xcc, 0xcc],
        16..=231 => {
            let i = i - 16;
            let r = i / 36;
            let g = (i % 36) / 6;
            let b = i % 6;
            let step = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            [step(r), step(g), step(b)]
        }
        232..=255 => {
            let v = 8 + (i - 232) * 10;
            [v, v, v]
        }
    }
}
