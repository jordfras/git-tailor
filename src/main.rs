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

// TUI application entry point

mod cli;
mod completions;
mod dispatch;
mod external_tool;
mod loader;
mod terminal_guard;
mod update_check;

use anyhow::Result;
use clap::{CommandFactory, Parser};
use git_tailor::repo::{
    AutostashRestore, Git2Repo, GitRepo, JournalStatus, RepoRead, StashConflictState,
};
use git_tailor::{
    CommitDiff, CommitInfo,
    app::{self, AppAction, AppMode, AppState},
    views,
};

use crate::cli::Cli;
use crate::dispatch::{LoopAction, dispatch_action};
#[cfg(unix)]
use crate::external_tool::with_tui_suspended;
use crate::loader::{load_initial_commits, load_with_progress, resolve_oid_bounds};
use crate::terminal_guard::setup_terminal;

/// A transient footer status is dismissed only by a real key press. Non-key
/// events (resize, focus) still flow through `read_event` so the loop can
/// redraw on resize, but they must not wipe a status message before it can be
/// read.
fn event_dismisses_status(event: &crossterm::event::Event) -> bool {
    matches!(event, crossterm::event::Event::Key(_))
}

fn main() -> Result<()> {
    // Dynamic shell completion: when invoked as a completion request (the
    // COMPLETE env var is set), compute candidates and exit. Must run before
    // anything writes to stdout or opens the repository. `bin("gt")` pins the
    // registration to the `gt` binary (the clap command is named "git-tailor").
    clap_complete::CompleteEnv::with_factory(Cli::command)
        .bin("gt")
        .complete();

    let cli = Cli::parse();

    // Maintenance subcommands run instead of the TUI and must work outside a git
    // repository, so handle them before opening the repo.
    if let Some(cli::Commands::Completions { shell, install }) = &cli.command {
        return completions::run(*shell, *install);
    }

    let mut git_repo = Git2Repo::open(std::env::current_dir()?)?;
    git_repo.set_autostash(cli.autostash);

    // Maintenance path: wipe recovery state and exit without any TUI.
    if cli.clean_journal {
        return run_clean_journal(&git_repo);
    }

    // Static output path: no TUI involved, load commits synchronously.
    if cli.static_output {
        let Some((commits, _reference_oid, _include_reference_oid)) =
            load_initial_commits(&git_repo, &cli)?
        else {
            return Ok(());
        };
        return run_static_output(&git_repo, &commits, &cli);
    }

    // TUI path: resolve bounds first (fast), then start the TUI immediately.
    let Some((from_oid, reference_oid, include_reference_oid)) =
        resolve_oid_bounds(&git_repo, &cli)?
    else {
        return Ok(());
    };

    let colors = cli.resolve_palette()?;
    let terminal_bg = colors.terminal_background();
    let mut terminal_guard = setup_terminal(terminal_bg)?;
    let kb_enhanced = terminal_guard.kb_enhanced();

    // Kick off the crates.io update check immediately so it can run during the
    // loading phase; its result is surfaced on a later keypress (see the loop).
    let mut update_poller = update_check::UpdatePoller::new();

    let mut app = AppState::new();
    app.reverse = cli.reverse;
    app.theme = cli.matrix_theme.unwrap_or_default();
    app.colors = colors;
    app.reference_oid = reference_oid.clone();
    app.include_reference_oid = include_reference_oid;
    app.full_fragmap = cli.full;

    if !load_with_progress(
        &git_repo,
        &mut terminal_guard,
        &mut app,
        &from_oid,
        &reference_oid,
        include_reference_oid,
        cli.full,
    )? {
        terminal_guard.shutdown()?;
        return Ok(());
    }

    if app.commits.is_empty() {
        terminal_guard.shutdown()?;
        eprintln!("No commits to display.");
        return Ok(());
    }

    check_journal_recovery(&mut git_repo, &mut app);

    loop {
        terminal_guard.terminal().draw(|frame| {
            // Paint the palette's base background first so every cell that sets
            // no background of its own adopts it (a no-op for --palette terminal).
            frame.render_widget(
                ratatui::widgets::Block::new().style(app.colors.base_style()),
                frame.area(),
            );
            let mode = app.mode.clone();
            render_mode(&mode, &git_repo, &mut app, frame);
        })?;

        let event = app::read_event()?;
        let dismiss_status = event_dismisses_status(&event);

        if let Some(version) = update_poller.poll() {
            app.update_notice = Some(format!("Version {version} available"));
        }

        // When the search-input bar is active in CommitDetail, forward raw
        // key events to the search handler instead of routing through parse_key.
        if matches!(app.mode, AppMode::CommitDetail) && app.search_input_active {
            if app.mode.parse_key(event.clone()) == app::KeyCommand::ForceQuit {
                break;
            }
            if dismiss_status {
                app.clear_status_message();
            }
            let result = views::commit_detail::handle_search_event(event, &mut app);
            if matches!(result, AppAction::Quit) {
                app.should_quit = true;
            }
            if app.should_quit {
                break;
            }
            continue;
        }

        let action = app.mode.parse_key(event);

        if dismiss_status {
            app.clear_status_message();
        }

        // Ctrl+C: abort any in-progress rebase then quit immediately.
        if action == app::KeyCommand::ForceQuit {
            if let AppMode::RebaseConflict(ref state) = app.mode {
                let _ = git_repo.rebase_abort(state);
            }
            // Restore any auto-stashed changes so the user's work is not stranded.
            let _ = git_repo.autostash_restore();
            break;
        }

        // Ctrl-Z (Unix only): suspend the process, then redraw when resumed.
        #[cfg(unix)]
        if action == app::KeyCommand::Suspend {
            with_tui_suspended(terminal_guard.terminal(), kb_enhanced, terminal_bg, || {
                // SAFETY: raise() is async-signal-safe; SIGTSTP is POSIX.
                unsafe { libc::raise(libc::SIGTSTP) };
            })?;
            continue;
        }

        // Mode-first dispatch: each view module handles its own actions.
        let mode = app.mode.clone();
        let result = match mode {
            AppMode::Loading { .. } => AppAction::Handled,
            AppMode::CommitList => views::commit_list::handle_key(action, &mut app),
            AppMode::CommitDetail => views::commit_detail::handle_key(action, &mut app),
            AppMode::OperationSelect { .. } => {
                views::operation_select::handle_key(action, &mut app)
            }
            AppMode::SplitSelect { .. } => views::split_select::handle_key(action, &mut app),
            AppMode::SplitFilesSelect { .. } => {
                views::split_files_select::handle_key(action, &mut app)
            }
            AppMode::SplitHunksSelect { .. } => {
                views::split_hunks_select::handle_key(action, &mut app)
            }
            AppMode::SplitConfirm(_) => views::split_select::handle_confirm_key(action, &mut app),
            AppMode::DropConfirm(_) => views::drop::handle_confirm_key(action, &mut app),
            AppMode::AutofixupConfirm(_) => views::autofixup::handle_confirm_key(action, &mut app),
            AppMode::RebaseConflict(_) => views::conflict::handle_conflict_key(action, &mut app),
            AppMode::StashConflict(_) => {
                views::stash_conflict::handle_stash_conflict_key(action, &mut app)
            }
            AppMode::RecoverConfirm(_) => views::recover::handle_recover_key(action, &mut app),
            AppMode::SquashSelect { .. } => views::squash_select::handle_key(action, &mut app),
            AppMode::MoveSelect { .. } => views::move_select::handle_key(action, &mut app),
            AppMode::Help(_) => views::help::handle_key(action, &mut app),
        };

        let dispatch_result = dispatch_action(
            result,
            &mut app,
            &mut git_repo,
            &mut terminal_guard,
            kb_enhanced,
        )?;

        match dispatch_result {
            LoopAction::Continue => continue,
            LoopAction::Proceed => {}
            LoopAction::Reload | LoopAction::ReloadPreserving | LoopAction::ReloadSelecting(_) => {
                let preserve = matches!(dispatch_result, LoopAction::ReloadPreserving);
                let saved_idx = match dispatch_result {
                    LoopAction::ReloadSelecting(idx) => idx,
                    _ => app.selection_index,
                };
                match git_repo.head_oid() {
                    Err(e) => app.set_error_message(format!("Reload failed: {e}")),
                    Ok(head_oid) => {
                        let reference_oid = app.reference_oid.clone();
                        let include_ref = app.include_reference_oid;
                        let full = app.full_fragmap;
                        match load_with_progress(
                            &git_repo,
                            &mut terminal_guard,
                            &mut app,
                            &head_oid,
                            &reference_oid,
                            include_ref,
                            full,
                        ) {
                            Err(e) => app.set_error_message(format!("Reload failed: {e}")),
                            Ok(true)
                                if preserve
                                    || matches!(
                                        dispatch_result,
                                        LoopAction::ReloadSelecting(_)
                                    ) =>
                            {
                                app.selection_index =
                                    saved_idx.min(app.commits.len().saturating_sub(1));
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    terminal_guard.shutdown()?;
    Ok(())
}

/// On startup, detect an operation a previous run was killed in the middle of
/// (from the persisted journal) and surface a recovery prompt — or inform the
/// user when the journal can't be used.
fn check_journal_recovery(git_repo: &mut impl GitRepo, app: &mut AppState) {
    // Drop undo/redo history (and its gc-pin refs) left stale by external
    // history changes, so it doesn't clutter the journal or tools like gitk.
    let _ = git_repo.prune_stale_journal();

    match git_repo.read_journal() {
        Ok(JournalStatus::Recovered(state)) if state.edit_context.is_some() => {
            // A crashed Edit: the shell session cannot be resumed and the branch
            // may be anywhere the user left it, so the only safe action is to
            // restore the branch to its original tip. In-shell commits remain
            // reachable via the reflog and the pinned `orig` ref; uncommitted
            // in-shell changes are discarded by the restoring checkout.
            match git_repo.abort_edit() {
                Ok(()) => app.set_error_message(
                    "Recovered an interrupted Edit — restored the branch \
                     (in-shell commits remain in the reflog)",
                ),
                Err(e) => {
                    app.set_error_message(format!("Failed to recover an interrupted Edit: {e}"))
                }
            }
        }
        Ok(JournalStatus::Recovered(state)) => {
            // Only offer recovery when the branch is still where the interrupted
            // operation left it; otherwise the journal is stale (history changed
            // outside git-tailor) and resuming or aborting would be unsafe.
            let head_matches = git_repo
                .head_oid()
                .map(|head| head == state.new_tip_oid)
                .unwrap_or(false);
            if head_matches {
                app.enter_recover_confirm(*state);
            } else {
                let _ = git_repo.clear_journal();
                app.set_error_message(
                    "Discarded a stale interrupted-operation journal (branch has moved)",
                );
            }
        }
        Ok(JournalStatus::NewerVersion(v)) => {
            app.set_error_message(format!(
                "Ignoring a journal written by a newer git-tailor (format v{v}); \
                 upgrade git-tailor or remove .git/git-tailor/journal.json"
            ));
        }
        Ok(JournalStatus::Corrupt(e)) => {
            app.set_error_message(format!("Ignoring unreadable operation journal: {e}"));
        }
        Ok(JournalStatus::None) => {}
        Err(e) => {
            app.set_error_message(format!("Failed to read operation journal: {e}"));
        }
    }

    // A leftover auto-stash with no operation to recover (e.g. a crash after the
    // op finished but before the stash was reapplied) — restore it now. If it
    // conflicts (or a previous run already left markers), open the resolution
    // dialog so the user can finish or abort rather than being stuck.
    if !matches!(app.mode, AppMode::RecoverConfirm(_))
        && let Ok(AutostashRestore::Conflict { files }) = git_repo.autostash_restore()
    {
        app.enter_stash_conflict(StashConflictState {
            operation_label: "the operation".to_string(),
            conflicting_files: files,
            still_unresolved: false,
        });
    }
}

/// Wipe all git-tailor recovery state (`--clean-journal`) and report what was
/// removed on stdout. No TUI is started.
fn run_clean_journal(git_repo: &impl GitRepo) -> Result<()> {
    let summary = git_repo.clean_journal()?;
    let journal_note = if summary.journal_removed {
        " and the journal file"
    } else {
        ""
    };
    println!(
        "Cleaned git-tailor state: removed {} ref(s){journal_note}.",
        summary.refs_removed
    );
    Ok(())
}

/// Render the fragmap to stdout in static (non-TUI) mode and return.
fn run_static_output(git_repo: &impl GitRepo, commits: &[CommitInfo], cli: &Cli) -> Result<()> {
    let mut commit_diffs: Vec<CommitDiff> = commits
        .iter()
        .map(|c| {
            c.oid
                .as_oid()
                .map(|oid| git_repo.commit_diff_for_fragmap(oid))
                .transpose()
        })
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect();
    if commit_diffs.len() != commits.len() {
        anyhow::bail!("Failed to load diffs for all commits");
    }
    if let Some(d) = git_repo.staged_diff_for_fragmap()? {
        commit_diffs.push(d);
    }
    if let Some(d) = git_repo.unstaged_diff_for_fragmap()? {
        commit_diffs.push(d);
    }
    print!(
        "{}",
        git_tailor::static_views::fragmap::render(
            &commit_diffs,
            cli.full,
            !cli.no_color,
            cli.reverse,
            crossterm::terminal::size().ok().map(|(w, _)| w),
        )
    );
    Ok(())
}

/// Render a mode, recursively drawing its background first for overlay modes.
fn render_mode(
    mode: &AppMode,
    git_repo: &impl GitRepo,
    app: &mut AppState,
    frame: &mut ratatui::Frame,
) {
    if let Some(bg) = mode.background() {
        render_mode(&bg, git_repo, app, frame);
    }

    match mode {
        AppMode::Loading { .. } => views::loading::render(app, frame),
        AppMode::CommitList => views::commit_list::render(app, frame),
        AppMode::CommitDetail => views::main_view::render(git_repo, app, frame),
        AppMode::OperationSelect { .. } => views::operation_select::render(app, frame),
        AppMode::SplitSelect { .. } => views::split_select::render(app, frame),
        AppMode::SplitFilesSelect { .. } => views::split_files_select::render(app, frame),
        AppMode::SplitHunksSelect { .. } => views::split_hunks_select::render(app, frame),
        AppMode::SplitConfirm(_) => views::split_select::render_split_confirm(app, frame),
        AppMode::DropConfirm(_) => views::drop::render_drop_confirm(app, frame),
        AppMode::AutofixupConfirm(_) => views::autofixup::render_autofixup_confirm(app, frame),
        AppMode::RebaseConflict(_) => views::conflict::render_conflict(app, frame),
        AppMode::StashConflict(_) => views::stash_conflict::render_stash_conflict(app, frame),
        AppMode::RecoverConfirm(_) => views::recover::render_recover(app, frame),
        AppMode::SquashSelect { .. } => views::commit_list::render(app, frame),
        AppMode::MoveSelect { .. } => views::commit_list::render(app, frame),
        AppMode::Help(prev) => views::help::render(prev, app, frame),
    }
}
