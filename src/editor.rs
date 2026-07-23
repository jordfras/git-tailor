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

use crate::repo::RepoRead;

use anyhow::Context as _;

/// Resolve the editor command to use for editing commit messages.
///
/// Gathers git's canonical editor lookup sources, in order:
/// 1. `GIT_EDITOR` environment variable
/// 2. `core.editor` git config setting
/// 3. `VISUAL` environment variable
/// 4. `EDITOR` environment variable
///
/// Unlike git, there is no `vi` fallback: if none of these is configured the
/// call errors with guidance instead of guessing an editor that may not exist
/// (e.g. `vi` on Windows).
fn resolve_editor(repo: &impl RepoRead) -> anyhow::Result<String> {
    let git_editor = std::env::var("GIT_EDITOR").ok();
    let core_editor = repo.get_config_string("core.editor")?;
    let visual = std::env::var("VISUAL").ok();
    let editor = std::env::var("EDITOR").ok();
    choose_editor(&[
        git_editor.as_deref(),
        core_editor.as_deref(),
        visual.as_deref(),
        editor.as_deref(),
    ])
}

/// Pick the first usable editor command from `candidates`, tried in order.
/// Whitespace-only values are treated as unset. Errors (with guidance) when
/// none is usable.
fn choose_editor(candidates: &[Option<&str>]) -> anyhow::Result<String> {
    for candidate in candidates {
        if let Some(cmd) = candidate.map(str::trim).filter(|s| !s.is_empty()) {
            return Ok(cmd.to_string());
        }
    }
    anyhow::bail!("No editor configured — set core.editor or $VISUAL/$EDITOR")
}

/// Suspend the TUI, open `path` in the configured editor, then restore the TUI.
///
/// The editor command may include arguments (e.g. `"emacs -nw"`) — they are
/// split on whitespace and forwarded before the file path.  Works for both
/// terminal editors (e.g. `vim`) and GUI editors that manage their own window
/// (e.g. `code --wait`).  The TUI is restored unconditionally so the app is
/// never left in a broken state.
fn launch_editor(repo: &impl RepoRead, path: &std::path::Path) -> anyhow::Result<()> {
    let editor_cmd = resolve_editor(repo)?;
    let mut parts = shell_words::split(&editor_cmd)
        .with_context(|| format!("failed to parse editor command `{editor_cmd}`"))?;
    if parts.is_empty() {
        anyhow::bail!("editor command is empty");
    }
    let prog = parts.remove(0);
    let args = parts;

    let status = std::process::Command::new(&prog)
        .args(&args)
        .arg(path)
        .status()
        .with_context(|| format!("failed to launch editor `{prog}`"))?;
    if !status.success() {
        anyhow::bail!("editor exited with {status}");
    }
    Ok(())
}

/// Open `message` in the configured editor and return the edited result.
pub fn edit_message_in_editor(repo: &impl RepoRead, message: &str) -> anyhow::Result<String> {
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
pub fn open_file_in_editor(repo: &impl RepoRead, path: &std::path::Path) -> anyhow::Result<()> {
    launch_editor(repo, path)
}

#[cfg(test)]
mod tests {
    use super::choose_editor;

    #[test]
    fn errs_when_no_candidate_is_usable() {
        // No `vi` fallback: nothing configured is a hard error.
        assert!(choose_editor(&[]).is_err());
        assert!(choose_editor(&[None, None, None, None]).is_err());
        // Whitespace-only values count as unset and fall through to the error.
        assert!(choose_editor(&[None, Some("   "), Some("\t")]).is_err());
    }

    #[test]
    fn picks_first_usable_candidate_trimmed() {
        assert_eq!(
            choose_editor(&[None, Some(" code --wait "), Some("vim")]).unwrap(),
            "code --wait"
        );
        // Earlier entries win over later ones.
        assert_eq!(
            choose_editor(&[Some("emacs"), Some("vim")]).unwrap(),
            "emacs"
        );
    }
}
