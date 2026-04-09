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

use crate::repo::GitRepo;

use anyhow::Context as _;
use crossterm::{execute, terminal};

/// Resolve the editor command to use for editing commit messages.
///
/// Walks git's canonical editor lookup chain:
/// 1. `GIT_EDITOR` environment variable
/// 2. `core.editor` git config setting
/// 3. `VISUAL` environment variable
/// 4. `EDITOR` environment variable
/// 5. Fallback: `"vi"`
fn resolve_editor(repo: &impl GitRepo) -> String {
    if let Ok(e) = std::env::var("GIT_EDITOR") {
        return e.trim().to_string();
    }

    if let Some(e) = repo.get_config_string("core.editor") {
        return e.trim().to_string();
    }

    for var in ["VISUAL", "EDITOR"] {
        if let Ok(e) = std::env::var(var) {
            return e.trim().to_string();
        }
    }

    "vi".to_string()
}

/// Suspend the TUI, open `path` in the configured editor, then restore the TUI.
///
/// The editor command may include arguments (e.g. `"emacs -nw"`) — they are
/// split on whitespace and forwarded before the file path.  Works for both
/// terminal editors (e.g. `vim`) and GUI editors that manage their own window
/// (e.g. `code --wait`).  The TUI is restored unconditionally so the app is
/// never left in a broken state.
fn launch_editor(repo: &impl GitRepo, path: &std::path::Path) -> anyhow::Result<()> {
    let editor_cmd = resolve_editor(repo);
    let mut parts = shell_words::split(&editor_cmd)
        .with_context(|| format!("failed to parse editor command `{editor_cmd}`"))?;
    if parts.is_empty() {
        anyhow::bail!("editor command is empty");
    }
    let prog = parts.remove(0);
    let args = parts;

    // Suspend TUI before handing the terminal to the editor.
    terminal::disable_raw_mode().context("failed to disable raw mode")?;
    execute!(std::io::stdout(), terminal::LeaveAlternateScreen)
        .context("failed to leave alternate screen")?;

    let status = std::process::Command::new(&prog)
        .args(&args)
        .arg(path)
        .status();

    // Restore TUI unconditionally so the app is never left in a broken state.
    let _ = terminal::enable_raw_mode();
    let _ = execute!(std::io::stdout(), terminal::EnterAlternateScreen);

    let status = status.with_context(|| format!("failed to launch editor `{prog}`"))?;
    if !status.success() {
        anyhow::bail!("editor exited with {status}");
    }
    Ok(())
}

/// Open `message` in the configured editor and return the edited result.
pub fn edit_message_in_editor(repo: &impl GitRepo, message: &str) -> anyhow::Result<String> {
    use std::io::Write as _;

    let mut tmpfile =
        tempfile::NamedTempFile::new().context("failed to create temp file for commit message")?;
    write!(tmpfile, "{message}").context("failed to write commit message to temp file")?;

    launch_editor(repo, tmpfile.path())?;

    let edited =
        std::fs::read_to_string(tmpfile.path()).context("failed to read edited commit message")?;
    Ok(edited.trim().to_string() + "\n")
}

/// Open an existing working-tree file in the configured editor.
///
/// `path` should be the absolute path to the file.  Returns when the editor
/// process exits.
pub fn open_file_in_editor(repo: &impl GitRepo, path: &std::path::Path) -> anyhow::Result<()> {
    launch_editor(repo, path)
}
