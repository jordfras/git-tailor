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

use clap::Parser;

use git_tailor::fragmap::SquashableScope;
use git_tailor::views::theme::Theme;

/// An interactive terminal tool for tidying up Git commits on a branch.
#[derive(Parser)]
#[command(
    //name = "gt",
    version,
    help_template = "{name} {version}\n{about-with-newline}\n{usage-heading} {usage}\n\n{all-args}{after-help}"
)]
pub struct Cli {
    /// A commit-ish to use as the base reference (branch, tag, or hash).
    ///
    /// When omitted, the tool resolves `origin/HEAD` to find the repository's
    /// default upstream branch (e.g. `origin/main`).  If `origin/HEAD` is not
    /// configured it falls back to `main`.
    pub base: Option<String>,

    /// Display commits in reverse order (HEAD at top).
    #[arg(short, long, env = "GT_REVERSE")]
    pub reverse: bool,

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

    /// Controls what the yellow squashable-connector indicator means.
    #[arg(long = "squashable-scope", value_enum, env = "GT_SQUASHABLE_SCOPE")]
    pub squashable_scope: Option<SquashableScope>,

    /// Show the complete repository history from HEAD down to the first commit.
    ///
    /// Cannot be combined with a BASE argument.
    #[arg(long, conflicts_with = "base")]
    pub all: bool,

    /// Hunk group matrix rendering theme.
    #[arg(long = "theme", value_enum, env = "GT_THEME")]
    pub theme: Option<Theme>,
}
