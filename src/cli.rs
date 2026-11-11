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

// Command-line argument definitions for the `gt` binary.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use clap_complete::engine::ArgValueCandidates;

use git_tailor::app::ScrollMargin;
use git_tailor::domain::search::SearchCase;
use git_tailor::views::palette::{Colors, Scheme};
use git_tailor::views::theme::Theme;

/// `--help` tip. Platform-specific because the "Edit" operation opens your
/// shell: `$SHELL` on Unix (default `/bin/sh`), `%COMSPEC%` on Windows
/// (default `cmd.exe`).
#[cfg(not(windows))]
const AFTER_HELP: &str = "Tip: git-tailor uses your Git editor to edit commit messages, your \
                          Git merge tool to resolve conflicts, and your shell for the \"Edit\" \
                          operation. It is wise to configure them, e.g.:\n  \
                          git config --global core.editor \"vim\"\n  \
                          git config --global merge.tool \"kdiff3\"\n  \
                          export SHELL=/bin/bash   # \"Edit\" opens $SHELL (default /bin/sh)";

#[cfg(windows)]
const AFTER_HELP: &str = "Tip: git-tailor uses your Git editor to edit commit messages, your \
                          Git merge tool to resolve conflicts, and your shell for the \"Edit\" \
                          operation. It is wise to configure them, e.g.:\n  \
                          git config --global core.editor \"notepad\"\n  \
                          git config --global merge.tool \"kdiff3\"\n  \
                          set COMSPEC=powershell.exe   # \"Edit\" opens %COMSPEC% (default cmd.exe)";

/// An interactive terminal tool for tidying up Git commits on a branch.
#[derive(Parser)]
#[command(
    //name = "gt",
    version,
    help_template = "{name} {version}\n{about-with-newline}\n{usage-heading} {usage}\n\n{all-args}{after-help}",
    after_help = AFTER_HELP,
)]
pub struct Cli {
    /// Optional maintenance subcommand (e.g. `completions`).
    ///
    /// When omitted, git-tailor launches its interactive TUI as usual.
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// A commit-ish to use as the base reference (branch, tag, or hash).
    ///
    /// When omitted, the tool resolves `origin/HEAD` to find the repository's
    /// default upstream branch (e.g. `origin/main`).  If `origin/HEAD` is not
    /// configured it falls back to `main`.
    #[arg(add = ArgValueCandidates::new(crate::completions::base_ref_candidates))]
    pub base: Option<String>,

    /// Display commits in reverse order (HEAD at top).
    #[arg(short, long, env = "GT_REVERSE")]
    pub reverse: bool,

    /// Never stash: refuse a dirty tree, and Staged or Unstaged folds while
    /// the other row has changes.
    ///
    /// By default, operations that need a clean working tree (move, drop,
    /// squash, fixup, undo, redo) stash staged and unstaged changes, keeping the
    /// two apart, and restore them afterwards; squashing or fixing up the Staged
    /// or Unstaged row sets the other row's changes aside the same way. With
    /// this flag they refuse to run instead.
    ///
    /// Untracked files are left where they are either way. An operation that
    /// would check out over one is refused outright.
    #[arg(long, env = "GT_NO_STASH")]
    pub no_stash: bool,

    /// Show all hunk group columns without deduplication.
    ///
    /// By default the hunk group matrix merges columns whose set of touching
    /// commits is identical, producing a compact view. With this flag every
    /// raw hunk group gets its own column.
    #[arg(short = 'f', long, env = "GT_FULL")]
    pub full: bool,

    /// Print the hunk group matrix to stdout and exit without launching the TUI.
    ///
    /// Output format matches the original fragmap tool.
    #[arg(short = 's', long = "static")]
    pub static_output: bool,

    /// Disable ANSI color output. Requires --static.
    ///
    /// Uses plain ASCII symbols: '#' for a direct touch, '|' for a squashable
    ///  connector, '^' for a conflicting connector, '.' for an empty cell.
    #[arg(long = "no-color", requires = "static_output")]
    pub no_color: bool,

    /// Show the complete repository history from HEAD down to the first commit.
    ///
    /// Cannot be combined with a BASE argument.
    #[arg(long, conflicts_with = "base")]
    pub all: bool,

    /// Rendering theme for the hunk group matrix.
    ///
    /// Styles the fragmap squares, connectors, and commit row text (not the
    /// overall colors — see `--palette`).
    #[arg(long = "matrix-theme", value_enum, env = "GT_MATRIX_THEME")]
    pub matrix_theme: Option<Theme>,

    /// Color palette for the UI: a built-in name or a scheme file.
    ///
    /// `terminal` (default) uses your terminal's own colors. `campbell` and
    /// `dark+` are built-in dark schemes applied on any terminal (useful on
    /// light or pastel themes where the UI would otherwise be hard to read). Any
    /// other value is a path to a Windows Terminal color-scheme JSON file, so you
    /// can apply any custom scheme.
    #[arg(
        long = "palette",
        env = "GT_PALETTE",
        value_name = "terminal|campbell|dark+|FILE",
        default_value = "terminal",
        value_parser = parse_palette
    )]
    pub palette: PaletteArg,

    /// How searching treats letter case.
    ///
    /// `smart` (default) ignores case unless the pattern has an uppercase
    /// letter of its own; `\S`, `\W` and the like do not count.
    #[arg(
        short = 'S',
        long = "search-case",
        value_enum,
        env = "GT_SEARCH_CASE",
        default_value = "smart"
    )]
    pub search_case: SearchCase,

    /// Rows kept visible beyond the cursor as the commit list scrolls.
    ///
    /// `auto` (default) is a sixth of the window height; `0` scrolls only once
    /// the cursor is at the edge.
    #[arg(
        long = "scroll-margin",
        env = "GT_SCROLL_MARGIN",
        value_name = "auto|N",
        default_value = "auto",
        value_parser = parse_scroll_margin
    )]
    pub scroll_margin: ScrollMargin,

    /// Remove all git-tailor recovery state and exit, without launching the TUI.
    ///
    /// Deletes the journal file (`.git/git-tailor/journal.json`) and this
    /// working tree's undo pins and in-progress pin under `refs/git-tailor/*`,
    /// found by namespace so stray refs go even if the journal is missing. A
    /// manual escape hatch for when recovery state gets stuck.
    ///
    /// Rescued working trees (`refs/git-tailor/rescue/*`) are kept — each is
    /// uncommitted work with no other copy. See `--drop-rescued`. When the
    /// journal cannot be read, every tree the pins still name is kept as one
    /// first, since nothing else can say which of them is your working tree.
    #[arg(long = "clean-journal", conflicts_with_all = ["base", "all", "static_output"])]
    pub clean_journal: bool,

    /// List every rescued working tree and exit, without launching the TUI.
    ///
    /// A rescued working tree is uncommitted work git-tailor kept under
    /// `refs/git-tailor/rescue/*` when it had to discard the record naming it.
    /// Shows each with the command that restores it. Read-only; see
    /// `--drop-rescued` to remove them.
    #[arg(long = "list-rescued", conflicts_with_all = ["base", "all", "static_output", "clean_journal", "drop_rescued"])]
    pub list_rescued: bool,

    /// Remove every rescued working tree and exit, without launching the TUI.
    ///
    /// A rescued working tree is uncommitted work git-tailor kept under
    /// `refs/git-tailor/rescue/*` when it had to discard the record naming it.
    /// Every one is removed without a prompt, and this is the only copy — use
    /// `--list-rescued` first to look. Separate from `--clean-journal`, which
    /// clears this working tree's recovery state.
    #[arg(long = "drop-rescued", conflicts_with_all = ["base", "all", "static_output", "clean_journal"])]
    pub drop_rescued: bool,

    /// Check crates.io for a newer release and exit, without launching the TUI.
    ///
    /// Reports the outcome, including why a check failed — the background check
    /// the TUI runs stays silent so an offline or firewalled machine is never
    /// nagged. Ignores the cached result and always asks crates.io.
    #[arg(long = "check-update", conflicts_with_all = ["base", "all", "static_output", "clean_journal", "drop_rescued", "list_rescued"])]
    pub check_update: bool,
}

impl Cli {
    /// Resolve the runtime palette from `--palette`. A built-in name maps to its
    /// scheme; any other value is a path to a Windows Terminal scheme file,
    /// which is read and parsed here (either can fail, hence the `Result`).
    pub fn resolve_palette(&self) -> Result<Colors> {
        Ok(match &self.palette {
            PaletteArg::Terminal => Colors::Terminal,
            PaletteArg::Campbell => Colors::Fixed(Scheme::CAMPBELL),
            PaletteArg::DarkPlus => Colors::Fixed(Scheme::DARK_PLUS),
            PaletteArg::File(path) => {
                let json = std::fs::read_to_string(path)
                    .with_context(|| format!("reading color scheme {}", path.display()))?;
                let scheme = Scheme::from_wt_json(&json)
                    .with_context(|| format!("parsing color scheme {}", path.display()))?;
                Colors::Fixed(scheme)
            }
        })
    }
}

/// The parsed value of `--palette`: a built-in scheme, or a path to a custom
/// Windows Terminal scheme file. Resolved by [`Cli::resolve_palette`].
#[derive(Clone, Debug)]
pub enum PaletteArg {
    Terminal,
    Campbell,
    DarkPlus,
    File(PathBuf),
}

fn parse_scroll_margin(value: &str) -> std::result::Result<ScrollMargin, String> {
    if value == "auto" {
        return Ok(ScrollMargin::Auto);
    }
    value
        .parse()
        .map(ScrollMargin::Fixed)
        .map_err(|_| format!("expected `auto` or a row count, got `{value}`"))
}

fn parse_palette(value: &str) -> std::result::Result<PaletteArg, String> {
    Ok(match value {
        "terminal" => PaletteArg::Terminal,
        "campbell" => PaletteArg::Campbell,
        "dark+" | "dark-plus" => PaletteArg::DarkPlus,
        path => PaletteArg::File(PathBuf::from(path)),
    })
}

/// Maintenance subcommands that run instead of the interactive TUI.
#[derive(Subcommand)]
pub enum Commands {
    /// Print or install a shell completion script for `gt`.
    ///
    /// Completions are computed dynamically by `gt` at completion time, so they
    /// always match the installed version. Use `--install` to write the script
    /// to the conventional user-local location, or omit it to print to stdout.
    Completions {
        /// Shell to generate the completion script for.
        #[arg(long, value_enum)]
        shell: CompletionShell,

        /// Install into the user-local completion directory instead of printing.
        #[arg(long)]
        install: bool,
    },
}

/// Shells for which `gt completions` can print/install a script.
#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum CompletionShell {
    Bash,
    Zsh,
    Fish,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{CommandFactory, FromArgMatches};
    use std::io::Write;

    /// Parse `args` with `GT_NO_STASH` left out, so a developer who exported it
    /// does not change what these tests see.
    fn parse(args: &[&str]) -> Cli {
        let command = Cli::command().mut_arg("no_stash", |arg| arg.env(None));
        Cli::from_arg_matches(&command.try_get_matches_from(args).unwrap()).unwrap()
    }

    #[test]
    fn no_stash_is_off_by_default() {
        assert!(!parse(&["gt"]).no_stash);
    }

    #[test]
    fn no_stash_turns_it_on() {
        assert!(parse(&["gt", "--no-stash"]).no_stash);
    }

    /// The flag decides whether the Staged and Unstaged rows can be folded too,
    /// which is not something anyone would guess from its name.
    #[test]
    fn no_stash_says_it_covers_the_working_tree_rows() {
        let command = Cli::command();
        let arg = command
            .get_arguments()
            .find(|arg| arg.get_id() == "no_stash")
            .expect("no no_stash flag");
        // The short help, what `-h` prints, as well as the long one.
        for help in [arg.get_help(), arg.get_long_help()] {
            let help = help.map(ToString::to_string).unwrap_or_default();
            assert!(
                help.contains("Staged") && help.contains("Unstaged"),
                "{help}"
            );
        }
    }

    #[test]
    fn search_case_defaults_to_smart_and_takes_the_other_two() {
        let cli = Cli::try_parse_from(["gt"]).expect("args parse");
        assert_eq!(cli.search_case, SearchCase::Smart);
        for (value, expected) in [
            ("sensitive", SearchCase::Sensitive),
            ("insensitive", SearchCase::Insensitive),
        ] {
            for flag in ["--search-case", "-S"] {
                let cli = Cli::try_parse_from(["gt", flag, value]).expect("args parse");
                assert_eq!(cli.search_case, expected);
            }
        }
    }

    #[test]
    fn scroll_margin_defaults_to_auto() {
        let cli = Cli::try_parse_from(["gt"]).expect("args parse");
        assert_eq!(cli.scroll_margin, ScrollMargin::Auto);
    }

    #[test]
    fn scroll_margin_accepts_a_row_count_including_zero() {
        for (value, expected) in [("3", 3), ("0", 0)] {
            let cli = Cli::try_parse_from(["gt", "--scroll-margin", value]).expect("args parse");
            assert_eq!(cli.scroll_margin, ScrollMargin::Fixed(expected));
        }
    }

    #[test]
    fn scroll_margin_rejects_a_non_numeric_value() {
        let err = Cli::try_parse_from(["gt", "--scroll-margin", "lots"])
            .map(|_| ())
            .expect_err("a non-numeric margin must be rejected");
        let msg = err.to_string();
        assert!(msg.contains("expected `auto` or a row count"), "{msg}");
    }

    #[test]
    fn palette_builtins_resolve_without_a_file() {
        let cli = Cli::try_parse_from(["gt", "--palette", "dark+"]).expect("args parse");
        assert!(matches!(cli.palette, PaletteArg::DarkPlus));
        assert!(cli.resolve_palette().is_ok());
    }

    #[test]
    fn palette_missing_file_errors_with_reading_context() {
        let cli =
            Cli::try_parse_from(["gt", "--palette", "/no/such/scheme.json"]).expect("args parse");
        let err = cli
            .resolve_palette()
            .expect_err("a missing file must error");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("reading color scheme"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn palette_bad_file_content_errors_with_parsing_context() {
        let mut file = tempfile::NamedTempFile::new().expect("temp file");
        write!(file, "{{ this is not a scheme").expect("write temp file");
        let path = file.path().to_str().expect("utf-8 path");

        let cli = Cli::try_parse_from(["gt", "--palette", path]).expect("args parse");
        let err = cli.resolve_palette().expect_err("bad content must error");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("parsing color scheme"),
            "unexpected error: {msg}"
        );
    }
}
