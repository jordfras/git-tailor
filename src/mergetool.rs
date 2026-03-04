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

// Launch the user-configured merge tool to resolve index conflicts.
//
// Git's merge tool contract:
//   - `merge.tool` names the tool
//   - `mergetool.<name>.cmd` is an optional shell command for that tool
//   - The shell command has access to $LOCAL (ours), $REMOTE (theirs),
//     $BASE (ancestor), and $MERGED (the working-tree file to save the result)
//   - git runs the cmd through a shell and waits for it to exit
//
// We follow the same contract: suspend the TUI, write the three index stages
// to temp files, run the tool via `sh -c`, wait for exit, then restore.

use crate::repo::GitRepo;
use anyhow::{Context, Result};
use crossterm::{execute, terminal};
use std::io::Write as _;
use std::path::Path;

/// Resolve the shell command to use for the configured merge tool.
///
/// Lookup order:
/// 1. `merge.tool` config → tool name
/// 2. `mergetool.<name>.cmd` → custom shell command for that tool
/// 3. Built-in patterns for well-known tools
///
/// Returns `None` when no merge tool is configured or the named tool is not
/// recognised and has no custom cmd.
pub fn resolve_merge_tool_cmd(repo: &impl GitRepo) -> Option<String> {
    let name = repo.get_config_string("merge.tool")?;
    let name = name.trim().to_string();

    if let Some(cmd) = repo.get_config_string(&format!("mergetool.{name}.cmd")) {
        return Some(cmd.trim().to_string());
    }

    builtin_cmd(&name)
}

/// Shell command for well-known built-in merge tools.
fn builtin_cmd(name: &str) -> Option<String> {
    match name {
        // vimdiff / nvimdiff family — two-way diff with MERGED as output
        "vimdiff" | "vimdiff2" => Some(format!("{name} -d $LOCAL $MERGED $REMOTE")),
        "vimdiff3" => Some(format!("{name} -d $LOCAL $BASE $REMOTE $MERGED")),
        "nvimdiff" | "nvimdiff2" => Some(format!("{name} -d $LOCAL $MERGED $REMOTE")),
        "nvimdiff3" => Some("nvim -d $LOCAL $BASE $REMOTE $MERGED".to_string()),
        "meld" => Some("meld $LOCAL $MERGED $REMOTE".to_string()),
        "kdiff3" => Some(
            "kdiff3 --L1 $MERGED --L2 $LOCAL --L3 $REMOTE -o $MERGED $BASE $LOCAL $REMOTE"
                .to_string(),
        ),
        "opendiff" => Some("opendiff $LOCAL $REMOTE -ancestor $BASE -merge $MERGED".to_string()),
        _ => None,
    }
}

/// Launch the configured merge tool for every file in `conflicting_files`.
///
/// Suspends the TUI before the first tool invocation and restores it after the
/// last one, so each tool instance has full control of the terminal. The TUI is
/// always restored, even when a tool exits with a non-zero status.
///
/// Returns `true` when the tool was invoked for at least one file, or `false`
/// when no merge tool is configured (so the caller can show a hint).
pub fn run_mergetool(repo: &impl GitRepo, conflicting_files: &[String]) -> Result<bool> {
    let Some(cmd) = resolve_merge_tool_cmd(repo) else {
        return Ok(false);
    };

    if conflicting_files.is_empty() {
        return Ok(true);
    }

    let workdir = repo
        .workdir()
        .ok_or_else(|| anyhow::anyhow!("repository has no working directory"))?;

    // Suspend the TUI before handing the terminal to the merge tool.
    terminal::disable_raw_mode().context("failed to disable raw mode")?;
    let _ = execute!(std::io::stdout(), terminal::LeaveAlternateScreen);

    let result = run_for_all_files(&cmd, &workdir, repo, conflicting_files);

    // Restore the TUI unconditionally so the app is never left in a broken state.
    let _ = terminal::enable_raw_mode();
    let _ = execute!(std::io::stdout(), terminal::EnterAlternateScreen);

    result?;
    Ok(true)
}

/// Lower-level entry point that runs the tool against every file without
/// touching the TUI. Exposed for integration tests.
///
/// Use [`run_mergetool`] from application code — it handles TUI suspend/restore.
#[doc(hidden)]
pub fn run_for_all_files(
    cmd: &str,
    workdir: &Path,
    repo: &impl GitRepo,
    files: &[String],
) -> Result<()> {
    for file_path in files {
        run_tool_for_file(cmd, workdir, repo, file_path)
            .with_context(|| format!("merge tool failed on '{file_path}'"))?;
    }
    Ok(())
}

fn run_tool_for_file(
    cmd: &str,
    workdir: &Path,
    repo: &impl GitRepo,
    file_path: &str,
) -> Result<()> {
    let base_content = repo
        .read_index_stage(file_path, 1)
        .context("failed to read base stage")?
        .unwrap_or_default();
    let ours_content = repo
        .read_index_stage(file_path, 2)
        .context("failed to read ours stage")?
        .unwrap_or_default();
    let theirs_content = repo
        .read_index_stage(file_path, 3)
        .context("failed to read theirs stage")?
        .unwrap_or_default();

    // Include the original file extension so tools can apply syntax highlighting.
    let ext = Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();

    let mut base_tmp = tempfile::Builder::new()
        .suffix(&format!(".BASE{ext}"))
        .tempfile()
        .context("failed to create BASE temp file")?;
    let mut local_tmp = tempfile::Builder::new()
        .suffix(&format!(".LOCAL{ext}"))
        .tempfile()
        .context("failed to create LOCAL temp file")?;
    let mut remote_tmp = tempfile::Builder::new()
        .suffix(&format!(".REMOTE{ext}"))
        .tempfile()
        .context("failed to create REMOTE temp file")?;

    base_tmp
        .write_all(&base_content)
        .context("failed to write BASE temp file")?;
    local_tmp
        .write_all(&ours_content)
        .context("failed to write LOCAL temp file")?;
    remote_tmp
        .write_all(&theirs_content)
        .context("failed to write REMOTE temp file")?;

    // Flush before the child process opens the files.
    base_tmp.flush().context("failed to flush BASE temp file")?;
    local_tmp
        .flush()
        .context("failed to flush LOCAL temp file")?;
    remote_tmp
        .flush()
        .context("failed to flush REMOTE temp file")?;

    let merged_path = workdir.join(file_path);

    // Run via shell so $LOCAL / $BASE / $REMOTE / $MERGED expand from the env vars.
    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .env("BASE", base_tmp.path())
        .env("LOCAL", local_tmp.path())
        .env("REMOTE", remote_tmp.path())
        .env("MERGED", &merged_path)
        .status()
        .context("failed to launch merge tool")?;

    // Temp files are kept alive (not dropped) until here, ensuring the child
    // can read them for the full duration of its execution.
    drop(base_tmp);
    drop(local_tmp);
    drop(remote_tmp);

    if !status.success() {
        anyhow::bail!("merge tool exited with {status}");
    }

    // Stage the resolved file so the index conflict entries (stage 1/2/3) are
    // replaced with a normal stage-0 entry. This is what `git mergetool` does
    // automatically and what makes `index.has_conflicts()` return false afterward.
    repo.stage_file(file_path)
        .with_context(|| format!("failed to stage resolved file '{file_path}'"))?;

    Ok(())
}
