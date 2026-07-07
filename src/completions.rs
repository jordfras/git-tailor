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

//! `gt completions` — print or install a dynamic shell-completion script.
//!
//! Completions are computed by `gt` itself at completion time (clap_complete's
//! dynamic engine, see the `CompleteEnv` hook in `main`), so the installed
//! script is a small registration stub rather than a full static script.

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};

use clap_complete::engine::CompletionCandidate;
use git_tailor::repo::Git2Repo;

use crate::cli::CompletionShell;

/// Dynamic completion candidates for the `base` argument: local and
/// remote-tracking branch names plus tag names from the current repository.
///
/// Returns no candidates when not inside a git repository (or on any error), so
/// completion degrades gracefully. Wired to the `base` arg in `cli.rs`.
pub fn base_ref_candidates() -> Vec<CompletionCandidate> {
    let names = std::env::current_dir()
        .ok()
        .and_then(|cwd| Git2Repo::open(cwd).ok())
        .and_then(|repo| repo.list_ref_names().ok())
        .unwrap_or_default();
    names.into_iter().map(CompletionCandidate::new).collect()
}

impl CompletionShell {
    /// Name understood by clap_complete's env completers.
    fn env_name(self) -> &'static str {
        match self {
            CompletionShell::Bash => "bash",
            CompletionShell::Zsh => "zsh",
            CompletionShell::Fish => "fish",
        }
    }

    /// Conventional user-local install path for the script, relative to `$HOME`.
    fn install_path(self, home: &Path) -> PathBuf {
        match self {
            CompletionShell::Bash => home.join(".local/share/bash-completion/completions/gt"),
            CompletionShell::Zsh => home.join(".local/share/zsh/site-functions/_gt"),
            CompletionShell::Fish => home.join(".config/fish/completions/gt.fish"),
        }
    }

    /// Hint printed after `--install` so the user knows how to activate it.
    fn reload_hint(self) -> &'static str {
        match self {
            CompletionShell::Bash => {
                "Start a new shell to activate completions — bash-completion loads them from\n\
                 the path above on demand (no shell-startup edit needed). If they don't appear,\n\
                 ensure bash-completion is enabled, or `source` the file above from your\n\
                 shell startup (e.g. ~/.bash_profile)."
            }
            CompletionShell::Zsh => {
                "Restart your shell to activate completions.\n\
                 Ensure `~/.local/share/zsh/site-functions` is on your `fpath` before `compinit`."
            }
            CompletionShell::Fish => "Restart your shell to activate completions.",
        }
    }
}

/// Build the dynamic-completion registration script for `shell`.
fn registration_script(shell: CompletionShell) -> Result<Vec<u8>> {
    let shells = clap_complete::env::Shells::builtins();
    let completer = shells
        .completer(shell.env_name())
        .ok_or_else(|| anyhow!("no completer available for shell `{}`", shell.env_name()))?;
    let mut buf = Vec::new();
    // (var, name, bin, completer): `gt` is the binary and its own completer.
    completer
        .write_registration("COMPLETE", "gt", "gt", "gt", &mut buf)
        .context("failed to render the completion script")?;
    Ok(buf)
}

/// Handle `gt completions`: print the script to stdout, or with `--install`
/// write it to the conventional user-local location and print a reload hint.
///
/// Does not touch the git repository, so it works from anywhere.
pub fn run(shell: CompletionShell, install: bool) -> Result<()> {
    let script = registration_script(shell)?;

    if !install {
        std::io::stdout()
            .write_all(&script)
            .context("failed to write the completion script to stdout")?;
        return Ok(());
    }

    let home = std::env::var_os("HOME").map(PathBuf::from).ok_or_else(|| {
        anyhow!(
            "cannot install: no home directory ($HOME) found.\n\
             Run this from the shell you are installing for (e.g. git-bash), where $HOME is set."
        )
    })?;
    let path = shell.install_path(&home);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    std::fs::write(&path, &script)
        .with_context(|| format!("failed to write {}", path.display()))?;

    println!(
        "Installed {} completions to {}",
        shell.env_name(),
        path.display()
    );
    println!("{}", shell.reload_hint());
    Ok(())
}
