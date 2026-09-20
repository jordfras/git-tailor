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

//! Renders TUI screenshots: the README image, and a gallery of representative
//! views.
//!
//! Rather than hand-capturing a terminal, each shot drives the production views
//! into a ratatui `TestBackend` buffer exactly as the TUI tests do, then
//! rasterizes that buffer with a bundled monospace font, resolving colors
//! through the shared `views::palette::Scheme` the live `--palette` option uses.
//! Because it drives the production rendering and color code, the images can
//! never drift from the real UI. Shots are declared as `AppMode`s and drawn
//! through the same dispatch the event loop uses, so a still cannot show a
//! layout the TUI never produces.
//!
//! Two sets, differing mainly in which repository they are filmed on:
//!
//! * **readme** — one shot of the commit list, on a synthetic history built
//!   in-process (see `common/synthetic_repo.rs`), whose entangled commits make
//!   the matrix show both connector colors.
//! * **gallery** — one shot per mode worth showing, filmed on one of the demo
//!   fixtures under `demo/`, so the stills and the videos tell one story. All
//!   at one size, so they can be laid out in a grid without letterboxing.
//!   Written under `target/`, not into the repository: nothing references them,
//!   so they are regenerated when something wants them rather than tracked and
//!   left to rot.
//!
//! Run with:
//! ```text
//! cargo run --example gen_screenshots
//! demo/promo/make-repo.sh /tmp/shots-repo
//! cargo run --example gen_screenshots -- --gallery /tmp/shots-repo
//! ```
//!
//! Note: `examples/` is excluded from the published crate (see `Cargo.toml`),
//! so neither this file, the bundled font, nor the `fontdue`/`image`
//! dev-dependencies are part of what users install.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use git_tailor::{
    VirtualOid,
    app::{AppMode, AppState, Operation},
    fragmap,
    repo::{Git2Repo, RepoRead},
    views,
    views::palette::Scheme,
    views::theme::Theme,
};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, layout::Rect};

const USAGE: &str = "usage: gen_screenshots [--gallery <repo-path> [out-dir] [base-ref]]";

/// Palette every shot is rendered with.
const PALETTE: Scheme = Scheme::DARK_PLUS;

/// Diff context lines, matching the detail view's default.
const CONTEXT_LINES: u32 = 3;

/// Blank columns left after the longest commit summary when fitting the title.
const TITLE_PAD: u16 = 2;

/// The floor `commit_list` clamps the title column to.
const MIN_TITLE: u16 = 10;

/// Where the gallery goes when no output directory is given. Under `target/`
/// because these are regenerable and unreferenced — only the README image is
/// committed, since `README.md` embeds it.
const GALLERY_DIR: &str = "target/screenshots";

/// The branch the fixtures fork from, and so the default commit range's base.
const DEFAULT_BASE: &str = "main";

/// Size the gallery is filmed at. One size for all of them, so they can be laid
/// out in a uniform grid, and close to 16:9 at the rasterizer's cell size.
///
/// The smallest that still fits the split dialog whole — its five strategies
/// each carry a description line, making it the tallest dialog that has to fit,
/// and the fixtures are short enough that extra rows only add dead space below
/// the commit list. Any clipped dialog is reported, so this can be retuned
/// against the warning rather than by eye. The help dialog is a full keybinding
/// reference and scrolls at any sane size; it is shown as users see it.
const GALLERY_COLS: u16 = 94;
const GALLERY_ROWS: u16 = 26;

/// Which repository a set is filmed on.
enum RepoSource {
    /// Built in-process, in a temp dir, by `common/synthetic_repo.rs`.
    Synthetic,
    /// An existing checkout, normally produced by a `demo/*/make-repo.sh`.
    Fixture(PathBuf),
}

/// How the title column is sized.
enum TitleWidth {
    /// Pinned to a published offset, so the image stays byte-identical.
    Fixed(i16),
    /// Fitted to the widest commit summary in the repository.
    FitToSummaries,
}

/// Shots sharing a repository. Grouped rather than standalone so the history is
/// walked and the fragmap built once per set, not once per image.
struct ShotSet {
    repo: RepoSource,
    base: String,
    /// Commit-summary prefix to select before rendering.
    select: &'static str,
    title: TitleWidth,
    shots: Vec<Shot>,
}

/// One screenshot: where it lands, the terminal size, and the mode to film.
struct Shot {
    out: PathBuf,
    cols: u16,
    rows: u16,
    mode: AppMode,
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let set = match args.first().map(String::as_str) {
        None => readme_set(),
        Some("--gallery") => {
            let repo = args.get(1).context(USAGE)?;
            let out_dir = args
                .get(2)
                .map_or_else(|| PathBuf::from(GALLERY_DIR), PathBuf::from);
            let base = args
                .get(3)
                .cloned()
                .unwrap_or_else(|| DEFAULT_BASE.to_string());
            gallery_set(PathBuf::from(repo), &out_dir, base)
        }
        Some(_) => bail!(USAGE),
    };
    render_set(&set)
}

/// The README image: one commit-list shot on the synthetic history.
fn readme_set() -> ShotSet {
    ShotSet {
        repo: RepoSource::Synthetic,
        base: DEFAULT_BASE.to_string(),
        // A commit whose hunk group contains both a squashable partner (green:
        // "feat: add expression parser") and a later conflicting edit (red:
        // "refactor: track source spans"). With the default Highlight theme this
        // lights up that one column and dims the rest — showcasing how the theme
        // focuses attention on the selected commit's relationships.
        select: "fix: handle unary minus",
        // Trims the title column down to the widest summary so the matrix sits
        // right next to the titles. Pinned rather than fitted: this is the value
        // doc/tui_example.png was published with, and README images should not
        // churn on unrelated changes.
        title: TitleWidth::Fixed(-4),
        shots: vec![Shot {
            out: PathBuf::from("doc/tui_example.png"),
            cols: 68,
            rows: 12,
            mode: AppMode::CommitList,
        }],
    }
}

/// The gallery: one shot per mode worth showing, on a demo fixture.
fn gallery_set(repo: PathBuf, out_dir: &Path, base: String) -> ShotSet {
    let shot = |name: &str, mode| Shot {
        out: out_dir.join(format!("{name}.png")),
        cols: GALLERY_COLS,
        rows: GALLERY_ROWS,
        mode,
    };
    ShotSet {
        repo: RepoSource::Fixture(repo),
        base,
        // A fix belonging to an earlier commit: its hunk group is shared with
        // that commit, so the matrix shows the squash relationship the tool
        // exists to reveal.
        select: "fix: guard against divide by zero",
        title: TitleWidth::FitToSummaries,
        // `CommitDetail` is the list beside the diff; the matrix belongs to
        // `CommitList`, so the two are separate shots rather than one.
        shots: vec![
            shot("01-hunk-group-matrix", AppMode::CommitList),
            shot("02-commit-detail", AppMode::CommitDetail),
            shot(
                "03-operations",
                AppMode::OperationSelect {
                    operation: Operation::Split,
                },
            ),
            shot("04-split", AppMode::SplitSelect { strategy_index: 0 }),
            shot("05-help", AppMode::Help(Box::new(AppMode::CommitList))),
        ],
    }
}

fn render_set(set: &ShotSet) -> Result<()> {
    // Held for the duration: dropping the TempDir deletes the repository.
    let mut _synthetic = None;
    let repo_path = match &set.repo {
        RepoSource::Synthetic => {
            let dir = tempfile::TempDir::new()?;
            synthetic_repo::build(dir.path()).context("building synthetic repo")?;
            let path = dir.path().to_path_buf();
            _synthetic = Some(dir);
            path
        }
        RepoSource::Fixture(path) => path.clone(),
    };

    let git_repo = Git2Repo::open(repo_path)?;
    let mut app = load_app(&git_repo, &set.base).context("loading commits + fragmap")?;
    select(&mut app, set.select);
    load_detail(&git_repo, &mut app)?;

    for shot in &set.shots {
        app.mode = shot.mode.clone();
        // Dialog bounds are only written by dialog renders, so a stale value
        // from the previous shot would misreport clipping here.
        app.dialog = Default::default();

        let buffer = render_to_buffer(&mut app, shot, &set.title)?;
        if app.dialog.max > 0 {
            println!(
                "  warning: {} clips {} row(s) of dialog",
                shot.out.display(),
                app.dialog.max
            );
        }

        let out = resolve(&shot.out);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        image_render::buffer_to_png(&buffer, &out, PALETTE)
            .with_context(|| format!("rasterizing {}", out.display()))?;
        println!(
            "Wrote {} ({}x{} cells)",
            out.display(),
            shot.cols,
            shot.rows
        );
    }
    Ok(())
}

/// Resolve output paths against the crate root, so a shot lands in the same
/// place no matter where `cargo run` was invoked from.
fn resolve(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
    }
}

fn render_to_buffer(app: &mut AppState, shot: &Shot, title: &TitleWidth) -> Result<Buffer> {
    let area = Rect::new(0, 0, shot.cols, shot.rows);
    // Re-applied per shot: the list view clamps `separator_offset` against the
    // area it is given and writes the clamped value back.
    match title {
        TitleWidth::Fixed(offset) => app.separator_offset = *offset,
        TitleWidth::FitToSummaries => fit_title_column(app, area),
    }

    let mut terminal = Terminal::new(TestBackend::new(shot.cols, shot.rows))?;
    terminal.draw(|frame| render_mode(&shot.mode, app, frame))?;
    Ok(terminal.backend().buffer().clone())
}

/// Mirrors `main.rs`'s `render_mode`: a dialog draws over the view it opened
/// from, so the backdrop is rendered first.
fn render_mode(mode: &AppMode, app: &mut AppState, frame: &mut ratatui::Frame) {
    if let Some(bg) = mode.background() {
        render_mode(&bg, app, frame);
    }
    match mode {
        AppMode::CommitList => views::commit_list::render(app, frame),
        AppMode::CommitDetail => views::main_view::render(app, frame),
        AppMode::OperationSelect { .. } => views::operation_select::render(app, frame),
        AppMode::SplitSelect { .. } => views::split_select::render(app, frame),
        AppMode::Help(prev) => views::help::render(prev, app, frame),
        other => panic!("gen_screenshots has no renderer for {other:?}"),
    }
}

/// Build the `AppState` exactly the way `loader.rs` does, minus the live
/// progress UI: list commits HEAD..base, drop the merge-base, diff each commit,
/// and build the deduplicated fragmap.
fn load_app(git_repo: &Git2Repo, base: &str) -> Result<AppState> {
    let head = git_repo.head_oid()?;
    let base_oid = git_repo.find_reference_point(base)?;

    let commits: Vec<_> = git_repo
        .list_commits(&head, &base_oid)?
        .into_iter()
        .filter(|c| c.oid != VirtualOid::Real(base_oid.clone()))
        .collect();
    if commits.is_empty() {
        bail!("no commits between {base} and HEAD — is the fixture built?");
    }

    let diffs = commits
        .iter()
        .filter_map(|c| c.oid.as_oid())
        .map(|oid| git_repo.commit_diff_for_fragmap(oid))
        .collect::<Result<Vec<_>>>()?;

    let fragmap = fragmap::build_fragmap(&diffs, true, &mut |_| true)
        .context("fragmap computation returned None")?;

    let mut app = AppState::with_commits(commits);
    // Render with the default theme so the screenshots match what users see.
    app.theme = Theme::default();
    app.fragmap = Some(fragmap);
    app.reference_oid = base_oid;
    Ok(app)
}

/// Move the selection to the commit whose summary starts with `prefix`, leaving
/// it alone when nothing matches so a fixture change cannot fail the run.
fn select(app: &mut AppState, prefix: &str) {
    if let Some(idx) = app
        .list
        .commits
        .iter()
        .position(|c| c.summary.starts_with(prefix))
    {
        app.list.selection_index = idx;
    }
}

/// Read the selected commit's diff into the detail pane, as opening the detail
/// view does.
fn load_detail(git_repo: &Git2Repo, app: &mut AppState) -> Result<()> {
    let Some(commit) = app.list.commits.get(app.list.selection_index) else {
        return Ok(());
    };
    let Some(oid) = commit.oid.as_oid() else {
        return Ok(());
    };
    app.detail.diff = Some(git_repo.commit_diff(oid, CONTEXT_LINES)?);
    Ok(())
}

/// Pull the hunk-group matrix up against the commit titles.
///
/// Left alone, the title column expands to fill whatever the terminal is wide,
/// stranding the matrix far to the right behind a band of empty space — fine
/// interactively (`<` / `>` adjust it), wrong for a still. `separator_offset`
/// is defined relative to that natural width, so derive the natural width from
/// the separator position instead of guessing: the view clamps the offset and
/// writes it back, so reading the position at offset 0 and at the far-left
/// clamp gives both the natural title width and the fixed columns before it.
fn fit_title_column(app: &mut AppState, area: Rect) {
    let widest = app
        .list
        .commits
        .iter()
        .map(|c| c.summary.chars().count())
        .max()
        .unwrap_or(0) as u16;
    let desired = widest + TITLE_PAD;

    app.separator_offset = 0;
    let Some(natural_sep) = views::commit_list::compute_fragmap_sep_x(app, area) else {
        return;
    };
    app.separator_offset = i16::MIN / 2;
    let Some(min_sep) = views::commit_list::compute_fragmap_sep_x(app, area) else {
        app.separator_offset = 0;
        return;
    };

    // At the far-left clamp the title is MIN_TITLE wide, which reveals how many
    // columns (scrollbar, SHA, gap) sit before the title in both measurements.
    let fixed = min_sep.saturating_sub(MIN_TITLE);
    let natural_title = natural_sep.saturating_sub(fixed);
    app.separator_offset = desired as i16 - natural_title as i16;
}

// ---------------------------------------------------------------------------
// Fixture and rasterization
// ---------------------------------------------------------------------------

// Cargo only treats top-level examples/*.rs as binaries, so these live in a
// subdirectory and are pulled in with #[path] rather than becoming examples of
// their own.
#[path = "common/image_render.rs"]
mod image_render;
#[path = "common/synthetic_repo.rs"]
mod synthetic_repo;
