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

//! Generates `doc/tui_example.png` — the screenshot shown in the README.
//!
//! Rather than hand-capturing a terminal, this builds a small *synthetic* git
//! repository in a temp dir (the same `TempDir` + `git2` approach used by the
//! integration tests), loads it through the real `Git2Repo` + fragmap pipeline,
//! renders the commit-list view into a ratatui `TestBackend` buffer exactly as
//! the TUI tests do, and then rasterizes that buffer to a PNG with a bundled
//! monospace font. Because it drives the production rendering code, the image
//! can never drift from the real UI.
//!
//! Run with: `cargo run --example gen_screenshot`
//!
//! Note: `examples/` is excluded from the published crate (see `Cargo.toml`),
//! so neither this file, the bundled font, nor the `fontdue`/`image`
//! dev-dependencies are part of what users install.

use std::path::Path;

use anyhow::{Context, Result};
use git_tailor::{
    VirtualOid,
    app::AppState,
    fragmap,
    repo::{Git2Repo, GitRepo},
    views,
    views::theme::Theme,
};
use git2::{Repository, Signature, Time};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

/// Terminal dimensions (columns × rows) the screenshot is rendered at.
const COLS: u16 = 68;
const ROWS: u16 = 12;

/// Output image location, relative to the crate root.
const OUTPUT: &str = "doc/tui_example.png";

fn main() -> Result<()> {
    let tmp = tempfile::TempDir::new()?;
    build_synthetic_repo(tmp.path()).context("building synthetic repo")?;

    let mut app = load_app(tmp.path()).context("loading commits + fragmap")?;

    // Select a commit whose hunk group contains both a squashable partner
    // (green: "feat: add expression parser") and a later conflicting edit
    // (red: "refactor: track source spans"). With the default Highlight theme
    // this lights up that one column and dims the rest — showcasing how the
    // theme focuses attention on the selected commit's relationships.
    if let Some(idx) = app
        .commits
        .iter()
        .position(|c| c.summary.starts_with("fix: handle unary minus"))
    {
        app.selection_index = idx;
    }

    let buffer = render_to_buffer(&mut app)?;

    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join(OUTPUT);
    image_render::buffer_to_png(&buffer, &out).context("rasterizing buffer to PNG")?;
    println!("Wrote {} ({COLS}x{ROWS} cells)", out.display());
    Ok(())
}

/// Build the `AppState` exactly the way `loader.rs` does, minus the live
/// progress UI: list commits HEAD..main, drop the merge-base, diff each commit,
/// and build the deduplicated fragmap.
fn load_app(repo_path: &Path) -> Result<AppState> {
    let git_repo = Git2Repo::open(repo_path.to_path_buf())?;
    let head = git_repo.head_oid()?;
    let base = git_repo.find_reference_point("main")?;

    let commits: Vec<_> = git_repo
        .list_commits(&head, &base)?
        .into_iter()
        .filter(|c| c.oid != VirtualOid::Real(base.clone()))
        .collect();

    let diffs = commits
        .iter()
        .map(|c| {
            let oid = c.oid.as_oid().expect("real commit");
            git_repo.commit_diff_for_fragmap(oid)
        })
        .collect::<Result<Vec<_>>>()?;

    let fragmap = fragmap::build_fragmap(&diffs, true, &mut |_| true)
        .context("fragmap computation returned None")?;

    let mut app = AppState::with_commits(commits);
    // Render with the default theme so the screenshot matches what users see.
    app.theme = Theme::default();
    app.fragmap = Some(fragmap);
    // Trim the title column down to the widest summary so the hunk-group matrix
    // sits right next to the titles, leaving just enough room for its header.
    app.separator_offset = -4;
    Ok(app)
}

/// Render the commit-list view into an off-screen buffer of the configured size.
fn render_to_buffer(app: &mut AppState) -> Result<Buffer> {
    let mut terminal = Terminal::new(TestBackend::new(COLS, ROWS))?;
    terminal.draw(|frame| views::commit_list::render(app, frame))?;
    Ok(terminal.backend().buffer().clone())
}

// ---------------------------------------------------------------------------
// Synthetic repository
// ---------------------------------------------------------------------------

/// One commit's worth of changes: a message and the full new contents of each
/// touched file.
struct Change {
    message: &'static str,
    files: &'static [(&'static str, &'static str)],
}

/// Build a small interpreter-project history on a `work` branch forked from
/// `main`. Several commits deliberately re-touch the same regions of the same
/// files (the `fixup!` commits) so the hunk-group matrix shows squash partners
/// and conflicts.
fn build_synthetic_repo(path: &Path) -> Result<()> {
    let mut opts = git2::RepositoryInitOptions::new();
    opts.initial_head("main");
    let repo = Repository::init_opts(path, &opts)?;

    // The merge-base on `main`: the scaffold the branch was forked from.
    commit(&repo, 0, &base_change())?;
    let base_oid = repo.head()?.target().context("no HEAD after base commit")?;
    repo.branch("work", &repo.find_commit(base_oid)?, false)?;
    repo.set_head("refs/heads/work")?;
    repo.checkout_head(None)?;

    for (i, change) in branch_changes().iter().enumerate() {
        commit(&repo, (i + 1) as i64, change)?;
    }
    Ok(())
}

/// Create one commit applying `change`, with a deterministic signature/time so
/// the generated image is reproducible. `seq` spaces commit times apart.
fn commit(repo: &Repository, seq: i64, change: &Change) -> Result<()> {
    let workdir = repo.workdir().context("bare repo has no workdir")?;
    let mut index = repo.index()?;
    for (rel, contents) in change.files {
        let full = workdir.join(rel);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&full, contents)?;
        index.add_path(Path::new(rel))?;
    }
    index.write()?;
    let tree = repo.find_tree(index.write_tree()?)?;

    // Deterministic identity and timestamps (one minute apart per commit).
    let when = Time::new(1_700_000_000 + seq * 60, 0);
    let sig = Signature::new("Ada Lovelace", "ada@example.com", &when)?;

    let parents: Vec<git2::Commit> = match repo.head().ok().and_then(|h| h.target()) {
        Some(oid) => vec![repo.find_commit(oid)?],
        None => vec![],
    };
    let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
    repo.commit(
        Some("HEAD"),
        &sig,
        &sig,
        change.message,
        &tree,
        &parent_refs,
    )?;
    Ok(())
}

// The file contents below are illustrative scaffolding for a toy expression
// interpreter. What matters for the screenshot is *where* successive commits
// edit each file: re-touching the same line range as an earlier commit makes
// the two share a hunk-group column, and the colour of the connector between
// them shows whether they can be cleanly squashed.
//
// The history is arranged to show both connector colours:
//   * yellow — a clean fixup (only re-touches one earlier commit's region), and
//   * red    — an entangled commit that touches regions belonging to *two*
//              different earlier commits, so it can't be folded into a single
//              one (`squash_target` is None). The two cross-cutting commits
//              ("thread source spans…" and "report errors with line numbers")
//              each produce a pair of red connectors.

fn base_change() -> Change {
    Change {
        message: "chore: initial project scaffold",
        files: &[(
            "Cargo.toml",
            "[package]\nname = \"calc\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )],
    }
}

// Successive states of each file, named by the commit that writes them.
const LEXER_V1: &str = "pub enum Token {\n    Number(f64),\n    Plus,\n    Minus,\n    Star,\n    Slash,\n}\n\npub fn tokenize(src: &str) -> Vec<Token> {\n    src.chars().filter_map(classify).collect()\n}\n";
const LEXER_V2: &str = "pub enum Token {\n    Number(f64),\n    Plus,\n    Minus,\n    Star,\n    Slash,\n    LParen,\n    RParen,\n}\n\npub fn tokenize(src: &str) -> Vec<Token> {\n    src.chars().filter_map(classify).collect()\n}\n";
const LEXER_V3: &str = "pub enum Token {\n    Number(f64, Span),\n    Plus,\n    Minus,\n    Star,\n    Slash,\n    LParen,\n    RParen,\n}\n\npub fn tokenize(src: &str) -> Vec<Token> {\n    src.chars().filter_map(classify).collect()\n}\n";

const PARSER_V1: &str = "use crate::lexer::Token;\n\npub struct Parser {\n    tokens: Vec<Token>,\n    pos: usize,\n}\n\nimpl Parser {\n    pub fn parse(&mut self) -> Expr {\n        self.expression()\n    }\n\n    fn expression(&mut self) -> Expr {\n        self.term()\n    }\n}\n";
const PARSER_V2: &str = "use crate::lexer::Token;\n\npub struct Parser {\n    tokens: Vec<Token>,\n    pos: usize,\n}\n\nimpl Parser {\n    pub fn parse(&mut self) -> Expr {\n        self.expression()\n    }\n\n    fn expression(&mut self) -> Expr {\n        if self.eat(Token::Minus) {\n            return Expr::Neg(Box::new(self.term()));\n        }\n        self.term()\n    }\n}\n";
const PARSER_V3: &str = "use crate::lexer::Token;\n\npub struct Parser {\n    tokens: Vec<Token>,\n    pos: usize,\n}\n\nimpl Parser {\n    pub fn parse(&mut self) -> Expr {\n        self.expression()\n    }\n\n    fn expression(&mut self) -> Expr {\n        if self.eat(Token::Minus) {\n            return Expr::Neg(self.span(), Box::new(self.term()));\n        }\n        self.term()\n    }\n}\n";
const PARSER_V4: &str = "use crate::lexer::Token;\n\npub struct Parser {\n    tokens: Vec<Token>,\n    pos: usize,\n}\n\nimpl Parser {\n    pub fn parse(&mut self) -> Expr {\n        self.expression()\n    }\n\n    fn expression(&mut self) -> Expr {\n        if self.eat(Token::Minus) {\n            return Expr::Neg(self.span(), Box::new(self.unary()));\n        }\n        self.term()\n    }\n}\n";

const EVAL_V1: &str = "use crate::parser::Expr;\n\npub fn eval(expr: &Expr) -> f64 {\n    match expr {\n        Expr::Number(n) => *n,\n        Expr::Add(a, b) => eval(a) + eval(b),\n        Expr::Sub(a, b) => eval(a) - eval(b),\n        Expr::Mul(a, b) => eval(a) * eval(b),\n    }\n}\n";
const EVAL_V2: &str = "use crate::parser::Expr;\n\npub fn eval(expr: &Expr) -> Result<f64, Error> {\n    match expr {\n        Expr::Number(n) => Ok(*n),\n        Expr::Add(a, b) => Ok(eval(a)? + eval(b)?),\n        Expr::Sub(a, b) => Ok(eval(a)? - eval(b)?),\n        Expr::Mul(a, b) => Ok(eval(a)? * eval(b)?),\n    }\n}\n";

const MAIN_V1: &str = "mod eval;\nmod lexer;\nmod parser;\n\nfn main() {\n    for line in std::io::stdin().lines() {\n        let line = line.unwrap();\n        println!(\"= {}\", run(&line));\n    }\n}\n";
const MAIN_V2: &str = "mod eval;\nmod lexer;\nmod parser;\n\nfn main() {\n    for (n, line) in std::io::stdin().lines().enumerate() {\n        let line = line.unwrap();\n        match run(&line) {\n            Ok(value) => println!(\"= {}\", value),\n            Err(e) => eprintln!(\"line {}: {}\", n + 1, e),\n        }\n    }\n}\n";

const README: &str = "# calc\n\nA tiny expression interpreter.\n\n## Grammar\n\n```\nexpr := term ((+|-) term)*\nterm := factor ((*|/) factor)*\n```\n";

fn branch_changes() -> &'static [Change] {
    &[
        Change {
            message: "feat: add token kinds",
            files: &[("src/lexer.rs", LEXER_V1)],
        },
        Change {
            message: "feat: add expression parser",
            files: &[("src/parser.rs", PARSER_V1)],
        },
        Change {
            message: "feat: add tree-walking evaluator",
            files: &[("src/eval.rs", EVAL_V1)],
        },
        Change {
            message: "feat: wire up the REPL entry point",
            files: &[("src/main.rs", MAIN_V1)],
        },
        Change {
            // Re-touches only the lexer enum — cleanly squashes into "add token
            // kinds": yellow connector.
            message: "fixup! add token kinds",
            files: &[("src/lexer.rs", LEXER_V2)],
        },
        Change {
            message: "fix: handle unary minus in the parser",
            files: &[("src/parser.rs", PARSER_V2)],
        },
        Change {
            // Touches BOTH the lexer enum (owned by "add token kinds") and the
            // parser (owned by "handle unary minus") — entangled across two
            // commits, so neither connector is a clean squash: red.
            message: "refactor: track source spans",
            files: &[("src/lexer.rs", LEXER_V3), ("src/parser.rs", PARSER_V3)],
        },
        Change {
            // Touches BOTH the evaluator (owned by "add evaluator") and main
            // (owned by "wire up the REPL") — again red on both connectors.
            message: "feat: report errors with line numbers",
            files: &[("src/eval.rs", EVAL_V2), ("src/main.rs", MAIN_V2)],
        },
        Change {
            // Re-touches only the parser expression — a clean fixup: yellow.
            message: "fixup! add expression parser",
            files: &[("src/parser.rs", PARSER_V4)],
        },
        Change {
            message: "docs: document the grammar",
            files: &[("README.md", README)],
        },
    ]
}

// ---------------------------------------------------------------------------
// Buffer -> PNG rasterization
// ---------------------------------------------------------------------------

mod image_render {
    use std::path::Path;

    use anyhow::Result;
    use fontdue::Font;
    use ratatui::buffer::Buffer;
    use ratatui::style::{Color, Modifier};

    /// Bundled monospace font with full Block Elements / Box Drawing coverage
    /// (the matrix uses `█` and `│`). Vendored under `examples/assets/`.
    const FONT_BYTES: &[u8] = include_bytes!("assets/DejaVuSansMono.ttf");

    /// Font size in pixels. Larger = higher-resolution screenshot.
    const FONT_PX: f32 = 22.0;
    /// Outer padding around the terminal grid, in pixels.
    const MARGIN: u32 = 14;

    // Windows Terminal "Campbell" palette — a familiar, readable terminal look.
    const DEFAULT_FG: [u8; 3] = [0xcc, 0xcc, 0xcc];
    const DEFAULT_BG: [u8; 3] = [0x0c, 0x0c, 0x0c];

    pub fn buffer_to_png(buffer: &Buffer, out: &Path) -> Result<()> {
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

        let mut img = image::RgbImage::from_pixel(img_w, img_h, image::Rgb(DEFAULT_BG));

        for y in 0..area.height {
            for x in 0..area.width {
                let cell = &buffer[(x, y)];
                let (fg, bg) = resolve_colors(cell.fg, cell.bg, cell.modifier);

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
    fn resolve_colors(fg: Color, bg: Color, modifier: Modifier) -> ([u8; 3], [u8; 3]) {
        let mut fg = to_rgb(fg, true);
        let mut bg = to_rgb(bg, false);
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

    fn to_rgb(color: Color, is_fg: bool) -> [u8; 3] {
        match color {
            Color::Reset => {
                if is_fg {
                    DEFAULT_FG
                } else {
                    DEFAULT_BG
                }
            }
            Color::Black => [0x0c, 0x0c, 0x0c],
            Color::Red => [0xc5, 0x0f, 0x1f],
            Color::Green => [0x13, 0xa1, 0x0e],
            Color::Yellow => [0xc1, 0x9c, 0x00],
            Color::Blue => [0x00, 0x37, 0xda],
            Color::Magenta => [0x88, 0x17, 0x98],
            Color::Cyan => [0x3a, 0x96, 0xdd],
            // ratatui `Gray` is the normal-intensity white (palette index 7).
            Color::Gray => [0xcc, 0xcc, 0xcc],
            Color::DarkGray => [0x76, 0x76, 0x76],
            Color::LightRed => [0xe7, 0x48, 0x56],
            Color::LightGreen => [0x16, 0xc6, 0x0c],
            Color::LightYellow => [0xf9, 0xf1, 0xa5],
            Color::LightBlue => [0x3b, 0x78, 0xff],
            Color::LightMagenta => [0xb4, 0x00, 0x9e],
            Color::LightCyan => [0x61, 0xd6, 0xd6],
            Color::White => [0xf2, 0xf2, 0xf2],
            Color::Rgb(r, g, b) => [r, g, b],
            Color::Indexed(i) => xterm256(i),
        }
    }

    /// Map an xterm 256-color index to RGB (16 system + 6×6×6 cube + grayscale).
    fn xterm256(i: u8) -> [u8; 3] {
        match i {
            0 => [0x0c, 0x0c, 0x0c],
            1 => [0xc5, 0x0f, 0x1f],
            2 => [0x13, 0xa1, 0x0e],
            3 => [0xc1, 0x9c, 0x00],
            4 => [0x00, 0x37, 0xda],
            5 => [0x88, 0x17, 0x98],
            6 => [0x3a, 0x96, 0xdd],
            7 => [0xcc, 0xcc, 0xcc],
            8 => [0x76, 0x76, 0x76],
            9 => [0xe7, 0x48, 0x56],
            10 => [0x16, 0xc6, 0x0c],
            11 => [0xf9, 0xf1, 0xa5],
            12 => [0x3b, 0x78, 0xff],
            13 => [0xb4, 0x00, 0x9e],
            14 => [0x61, 0xd6, 0xd6],
            15 => [0xf2, 0xf2, 0xf2],
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
}
