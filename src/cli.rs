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

use clap::{Parser, Subcommand, ValueEnum};
use clap_complete::engine::ArgValueCandidates;

use git_tailor::views::theme::Theme;

/// An interactive terminal tool for tidying up Git commits on a branch.
#[derive(Parser)]
#[command(
    //name = "gt",
    version,
    help_template = "{name} {version}\n{about-with-newline}\n{usage-heading} {usage}\n\n{all-args}{after-help}",
    after_help = "Tip: git-tailor uses your Git editor to edit commit messages and your Git \
                  merge tool to resolve conflicts. It is wise to configure both, e.g.:\n  \
                  git config --global core.editor \"vim\"\n  \
                  git config --global merge.tool \"kdiff3\""
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

    /// Stash working-tree changes around operations, restoring them afterwards.
    ///
    /// Automatically stash staged/unstaged/untracked changes around operations
    /// that need a clean working tree (move, drop, squash, fixup, undo, redo),
    /// restoring them afterwards, instead of refusing to run.
    #[arg(short = 'a', long, env = "GT_AUTOSTASH")]
    pub autostash: bool,

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

    /// Hunk group matrix rendering theme.
    #[arg(long = "theme", value_enum, env = "GT_THEME")]
    pub theme: Option<Theme>,

    /// Remove all git-tailor recovery state and exit, without launching the TUI.
    ///
    /// Deletes the journal file (`.git/git-tailor/journal.json`) and every ref
    /// git-tailor writes under `refs/git-tailor/*` (undo pins and the
    /// in-progress pin), found by namespace so stray refs are removed even if the
    /// journal is missing or out of sync. A manual escape hatch for when recovery
    /// state gets stuck.
    #[arg(long = "clean-journal", conflicts_with_all = ["base", "all", "static_output"])]
    pub clean_journal: bool,
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
