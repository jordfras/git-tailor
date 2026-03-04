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

/// Open `message` in the configured editor and return the edited result.
///
/// Suspends the TUI (disables raw mode, leaves the alternate screen) before
/// launching the editor, then restores it unconditionally before returning.
/// Works for both terminal-UI editors (e.g. `vim`, `emacs -nw`) and GUI
/// editors that manage their own window (e.g. `code --wait`).
///
/// The editor command may include arguments (e.g. `"emacs -nw"`) — they are
/// split on whitespace and forwarded before the temp-file path.
pub fn edit_message_in_editor(repo: &impl GitRepo, message: &str) -> anyhow::Result<String> {
    use anyhow::Context;
    use crossterm::{execute, terminal};
    use std::io::Write as _;

    let mut tmpfile =
        tempfile::NamedTempFile::new().context("failed to create temp file for commit message")?;
    write!(tmpfile, "{message}").context("failed to write commit message to temp file")?;

    let editor_cmd = resolve_editor(repo);
    let mut parts = editor_cmd.split_whitespace();
    let prog = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("editor command is empty"))?;
    let args: Vec<&str> = parts.collect();

    // Suspend TUI before handing the terminal to the editor.
    terminal::disable_raw_mode().context("failed to disable raw mode")?;
    execute!(std::io::stdout(), terminal::LeaveAlternateScreen)
        .context("failed to leave alternate screen")?;

    let status = std::process::Command::new(prog)
        .args(&args)
        .arg(tmpfile.path())
        .status();

    // Restore TUI unconditionally so the app is never left in a broken state.
    let _ = terminal::enable_raw_mode();
    let _ = execute!(std::io::stdout(), terminal::EnterAlternateScreen);

    let status = status.with_context(|| format!("failed to launch editor `{prog}`"))?;
    if !status.success() {
        anyhow::bail!("editor exited with {status}");
    }

    let edited =
        std::fs::read_to_string(tmpfile.path()).context("failed to read edited commit message")?;
    Ok(edited.trim().to_string() + "\n")
}
