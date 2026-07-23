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

// Side-effect handler for the Edit operation: suspend the TUI, drop the user
// into a shell to rewrite the commit, then splice the result back in.

use anyhow::{Context, Result};
use git_tailor::Oid;
use git_tailor::app::AppState;
use git_tailor::repo::{EditOutcome, GitRepo};

use crate::dispatch::{LoopAction, settle_autostash};
use crate::external_tool::with_tui_suspended;
use crate::get_head_oid_or_continue;

pub(crate) fn handle_execute_edit(
    git_repo: &mut impl GitRepo,
    app: &mut AppState,
    commit_oid: Oid,
    commit_summary: String,
    terminal_guard: &mut crate::terminal_guard::TerminalGuard,
    kb_enhanced: bool,
) -> Result<LoopAction> {
    let head_oid = get_head_oid_or_continue!(git_repo, app);
    // Auto-stash only stashes under `--autostash`; otherwise it is a no-op and
    // `begin_edit`'s clean-tree check refuses a dirty working tree.
    if let Err(e) = git_repo.autostash_save() {
        app.set_error_message(format!("Auto-stash failed: {e}"));
        return Ok(LoopAction::Proceed);
    }
    if let Err(e) = git_repo.begin_edit(&commit_oid, &head_oid) {
        let _ = git_repo.autostash_restore();
        app.set_error_message(format!("Edit failed: {e}"));
        return Ok(LoopAction::Proceed);
    }

    // Suspend the TUI and drop the user into a shell to rewrite the commit.
    // Re-open it if they exit with uncommitted changes — we must never finish
    // (and force-checkout) over a dirty tree, which would discard their work.
    let terminal_bg = terminal_guard.background();
    let short = commit_oid.short().to_string();
    let mut dirty = false; // begin_edit left a clean checkout
    loop {
        let shell_result =
            with_tui_suspended(terminal_guard.terminal(), kb_enhanced, terminal_bg, || {
                run_edit_shell(git_repo, &short, &commit_summary, dirty)
            })?;
        if let Err(e) = shell_result {
            // The shell could not even be launched — abort so the branch is restored.
            let _ = git_repo.abort_edit();
            let _ = git_repo.autostash_restore();
            app.set_error_message(format!("Edit failed: {e}"));
            return Ok(LoopAction::Reload);
        }
        dirty = git_repo.is_worktree_dirty().unwrap_or(false);
        if !dirty {
            break;
        }
    }

    let outcome = git_repo.finish_edit(&commit_oid);
    Ok(handle_edit_outcome(git_repo, app, outcome))
}

fn handle_edit_outcome(
    git_repo: &mut impl GitRepo,
    app: &mut AppState,
    outcome: Result<EditOutcome>,
) -> LoopAction {
    match outcome {
        Ok(EditOutcome::Complete) => settle_autostash(
            app,
            git_repo.autostash_restore(),
            "Edit",
            "Commit edited",
            LoopAction::ReloadPreserving,
        ),
        Ok(EditOutcome::Cancelled) => {
            let _ = git_repo.autostash_restore();
            app.set_success_message("Edit cancelled — no changes");
            LoopAction::Reload
        }
        Ok(EditOutcome::Conflict(state)) => {
            // Replaying descendants conflicted — defer the auto-stash restore
            // until the conflict is resolved/aborted, like every other rebase.
            app.enter_rebase_conflict(*state);
            LoopAction::Continue
        }
        Err(e) => {
            let _ = git_repo.autostash_restore();
            app.set_error_message(format!("Edit failed: {e}"));
            LoopAction::Reload
        }
    }
}

/// Print an instruction banner, then spawn `$SHELL` in the repo's working
/// directory so the user can rewrite the checked-out commit by hand. The
/// shell's exit code is ignored — the edit is judged by the resulting commits.
///
/// `dirty` is set when re-opening the shell because the user exited with
/// uncommitted changes; the banner then explains they must commit or discard
/// them (nothing is applied while the tree is dirty, so nothing is lost).
fn run_edit_shell(
    git_repo: &impl GitRepo,
    short_oid: &str,
    summary: &str,
    dirty: bool,
) -> Result<()> {
    use std::io::Write;
    let workdir = git_repo
        .workdir()
        .ok_or_else(|| anyhow::anyhow!("repository has no working directory"))?;
    let (prog, args) = resolve_shell();

    let mut err = std::io::stderr();
    if dirty {
        let _ = writeln!(
            err,
            "\n⚠ Uncommitted changes remain — the edit was not applied (nothing lost)."
        );
        let _ = writeln!(
            err,
            "Commit them (git commit --amend) to keep them, or discard them"
        );
        let _ = writeln!(
            err,
            "(git restore . && git restore --staged .) to cancel, then exit again.\n"
        );
    } else {
        let _ = writeln!(
            err,
            "\n── git-tailor: editing commit {short_oid} {summary} ──"
        );
        let _ = writeln!(
            err,
            "The commit is checked out. Rewrite it however you like:"
        );
        let _ = writeln!(err, "  • change files, then: git commit --amend");
        let _ = writeln!(
            err,
            "  • or split it: git reset HEAD~   then re-commit in pieces (e.g. git add -p)"
        );
        let _ = writeln!(
            err,
            "Run `exit` (or Ctrl-D) when done. Exit without changing the commit to cancel.\n"
        );
    }
    let _ = err.flush();

    std::process::Command::new(&prog)
        .args(&args)
        .current_dir(&workdir)
        .status()
        .with_context(|| format!("failed to launch shell `{prog}`"))?;
    Ok(())
}

/// The shell program to spawn for an Edit: `$SHELL` on unix (fallback
/// `/bin/sh`), `%COMSPEC%` on windows (fallback `cmd.exe`). We deliberately do
/// not try to detect the launching shell — the parent process is often not a
/// shell (terminal emulator, tmux, IDE, script), so `$SHELL` (the user's
/// preferred shell, as git/less/fzf use) is the reliable choice.
fn resolve_shell() -> (String, Vec<String>) {
    #[cfg(windows)]
    {
        let prog = std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string());
        (prog, Vec::new())
    }
    #[cfg(not(windows))]
    {
        let prog = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
        (prog, Vec::new())
    }
}
