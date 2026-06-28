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
mod external_tool;
mod loader;
mod terminal_guard;
mod update_check;

use anyhow::Result;
use clap::Parser;
use git_tailor::repo::{
    AutostashContinue, AutostashRestore, CommitOutcome, ConflictState, Git2Repo, GitRepo,
    JournalStatus, RebaseOutcome, StageOutcome, StashConflictState, UndoOutcome,
};
use git_tailor::{
    CommitDiff, CommitInfo, Oid,
    app::{self, AppAction, AppMode, AppState, SplitStrategy, SquashMode},
    editor, mergetool, views,
};

use crate::cli::Cli;
use crate::external_tool::with_tui_suspended;
use crate::loader::{load_initial_commits, load_with_progress, resolve_oid_bounds};
use crate::terminal_guard::setup_terminal;

/// Loop-control signal returned by [`dispatch_action`].
enum LoopAction {
    /// Skip the rest of this iteration (equivalent to `continue` in the loop).
    Continue,
    /// Fall through to the post-dispatch checks (e.g. `should_quit`).
    Proceed,
    /// Reload commits from HEAD, preserving the current selection index.
    ReloadPreserving,
    /// Reload commits from HEAD, resetting selection to the natural position.
    Reload,
}

/// Fetch HEAD OID from the repo, setting an error message and returning
/// `LoopAction::Continue` from the enclosing function if the call fails.
///
/// Only valid inside [`dispatch_action`].
macro_rules! get_head_oid_or_continue {
    ($git_repo:expr, $app:expr) => {
        match $git_repo.head_oid() {
            Ok(oid) => oid,
            Err(e) => {
                $app.set_error_message(format!("Failed to get HEAD: {e}"));
                return Ok(LoopAction::Continue);
            }
        }
    };
}

fn main() -> Result<()> {
    let cli = Cli::parse();

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

    let mut terminal_guard = setup_terminal()?;
    let kb_enhanced = terminal_guard.kb_enhanced();

    // Kick off the crates.io update check immediately so it can run during the
    // loading phase; its result is surfaced on a later keypress (see the loop).
    let mut update_poller = update_check::UpdatePoller::new();

    let mut app = AppState::new();
    app.reverse = cli.reverse;
    app.theme = cli.theme.unwrap_or_default();
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
            let mode = app.mode.clone();
            render_mode(&mode, &git_repo, &mut app, frame);
        })?;

        let event = app::read_event()?;

        if let Some(version) = update_poller.poll() {
            app.update_notice = Some(format!("Version {version} available"));
        }

        // When the search-input bar is active in CommitDetail, forward raw
        // key events to the search handler instead of routing through parse_key.
        if matches!(app.mode, AppMode::CommitDetail) && app.search_input_active {
            if app.mode.parse_key(event.clone()) == app::KeyCommand::ForceQuit {
                break;
            }
            app.clear_status_message();
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

        app.clear_status_message();

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
            with_tui_suspended(terminal_guard.terminal(), kb_enhanced, || {
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
            AppMode::SplitSelect { .. } => views::split_select::handle_key(action, &mut app),
            AppMode::SplitFileSelect { .. } => {
                views::split_file_select::handle_key(action, &mut app)
            }
            AppMode::SplitConfirm(_) => views::split_select::handle_confirm_key(action, &mut app),
            AppMode::DropConfirm(_) => views::drop::handle_confirm_key(action, &mut app),
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
            LoopAction::Reload | LoopAction::ReloadPreserving => {
                let preserve = matches!(dispatch_result, LoopAction::ReloadPreserving);
                let saved_idx = app.selection_index;
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
                            Ok(true) if preserve => {
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

/// Handle the side effects requested by a view-module action. Returns whether
/// the caller should `continue` the event loop or fall through to the
/// post-dispatch checks.
fn dispatch_action(
    result: AppAction,
    app: &mut AppState,
    git_repo: &mut impl GitRepo,
    terminal_guard: &mut crate::terminal_guard::TerminalGuard,
    kb_enhanced: bool,
) -> Result<LoopAction> {
    match result {
        AppAction::Handled => {}
        AppAction::Quit => app.should_quit = true,
        AppAction::ReloadCommits => return Ok(LoopAction::Reload),
        AppAction::PrepareSplit {
            strategy,
            commit_oid,
        } => return handle_prepare_split(git_repo, app, strategy, commit_oid),
        AppAction::ExecuteSplit {
            strategy,
            commit_oid,
            head_oid,
        } => {
            if execute_split(git_repo, app, strategy, &commit_oid, &head_oid) {
                return Ok(LoopAction::Reload);
            }
        }
        AppAction::PrepareSplitOutFile { commit_oid } => {
            return handle_prepare_split_out_file(git_repo, app, commit_oid);
        }
        AppAction::ExecuteSplitOutFile {
            commit_oid,
            file_path,
        } => return handle_execute_split_out_file(git_repo, app, commit_oid, file_path),
        AppAction::PrepareDropConfirm {
            commit_oid,
            commit_summary,
        } => {
            let head_oid = get_head_oid_or_continue!(git_repo, app);
            app.enter_drop_confirm(commit_oid, commit_summary, head_oid);
        }
        AppAction::ExecuteDrop {
            commit_oid,
            head_oid,
        } => return handle_execute_drop(git_repo, app, commit_oid, head_oid),
        AppAction::RebaseContinue(state) => {
            return handle_rebase_continue(git_repo, app, state, terminal_guard, kb_enhanced);
        }
        AppAction::RebaseAbort(state) => return handle_rebase_abort(git_repo, app, state),
        AppAction::RunMergetool {
            files,
            conflict_state,
        } => {
            return handle_run_mergetool(
                git_repo,
                app,
                files,
                conflict_state,
                terminal_guard,
                kb_enhanced,
            );
        }
        AppAction::RunEditor {
            files,
            conflict_state,
        } => {
            return handle_run_editor(
                git_repo,
                app,
                files,
                conflict_state,
                terminal_guard,
                kb_enhanced,
            );
        }
        AppAction::AutostashContinue => return handle_autostash_continue(git_repo, app),
        AppAction::AutostashAbort => return handle_autostash_abort(git_repo, app),
        AppAction::RunMergetoolForStash { files } => {
            return handle_run_stash_tool(git_repo, app, files, terminal_guard, kb_enhanced, true);
        }
        AppAction::RunEditorForStash { files } => {
            return handle_run_stash_tool(git_repo, app, files, terminal_guard, kb_enhanced, false);
        }
        AppAction::PrepareReword {
            commit_oid,
            current_message,
        } => {
            return handle_prepare_reword(
                git_repo,
                app,
                commit_oid,
                current_message,
                terminal_guard,
                kb_enhanced,
            );
        }
        AppAction::PrepareSquash {
            source_oid,
            target_oid,
            source_message,
            target_message,
            squash_mode,
        } => {
            return handle_prepare_squash(
                git_repo,
                app,
                source_oid,
                target_oid,
                source_message,
                target_message,
                squash_mode,
                terminal_guard,
                kb_enhanced,
            );
        }
        AppAction::ExecuteMove {
            source_oid,
            insert_after_oid,
        } => return handle_execute_move(git_repo, app, source_oid, insert_after_oid),
        AppAction::StageAll => {
            let outcome = git_repo.stage_all();
            return Ok(report_stage_outcome(
                app,
                outcome,
                "Staged all changes",
                "Nothing to stage",
            ));
        }
        AppAction::UnstageAll => {
            let outcome = git_repo.unstage_all();
            return Ok(report_stage_outcome(
                app,
                outcome,
                "Unstaged all changes",
                "Nothing to unstage",
            ));
        }
        AppAction::PrepareCommitStaged => {
            return handle_commit_staged(git_repo, app, terminal_guard, kb_enhanced);
        }
        AppAction::Undo => return handle_undo(git_repo, app),
        AppAction::Redo => return handle_redo(git_repo, app),
    }
    Ok(LoopAction::Proceed)
}

/// Report a stage-all / unstage-all outcome. A change triggers a reload so the
/// synthetic Staged/Unstaged rows refresh; a no-op leaves the view untouched.
fn report_stage_outcome(
    app: &mut AppState,
    outcome: Result<StageOutcome>,
    changed_msg: &str,
    noop_msg: &str,
) -> LoopAction {
    match outcome {
        Ok(StageOutcome::Changed) => {
            app.set_success_message(changed_msg.to_string());
            LoopAction::Reload
        }
        Ok(StageOutcome::NoOp) => {
            app.set_success_message(noop_msg.to_string());
            LoopAction::Proceed
        }
        Err(e) => {
            app.set_error_message(format!("{e}"));
            LoopAction::Proceed
        }
    }
}

/// Commit the staged changes: open the editor for a message (reusing the reword
/// editor flow), then create the commit. An empty message cancels.
fn handle_commit_staged(
    git_repo: &impl GitRepo,
    app: &mut AppState,
    terminal_guard: &mut crate::terminal_guard::TerminalGuard,
    kb_enhanced: bool,
) -> Result<LoopAction> {
    let editor_result = with_tui_suspended(terminal_guard.terminal(), kb_enhanced, || {
        editor::edit_message_in_editor(git_repo, "")
    })?;
    match editor_result {
        Err(e) => app.set_error_message(format!("Editor error: {e}")),
        Ok(message) if message.trim().is_empty() => {
            app.set_success_message("Commit cancelled: message is empty");
        }
        Ok(message) => match git_repo.commit_staged(&message) {
            Ok(CommitOutcome::Committed) => {
                app.set_success_message("Committed staged changes");
                return Ok(LoopAction::Reload);
            }
            Ok(CommitOutcome::NothingStaged) => app.set_error_message("Nothing staged to commit"),
            Err(e) => app.set_error_message(format!("Commit failed: {e}")),
        },
    }
    Ok(LoopAction::Proceed)
}

fn handle_undo(git_repo: &mut impl GitRepo, app: &mut AppState) -> Result<LoopAction> {
    // A working-tree-preserving undo (stage/unstage all, or a commit's soft
    // reset) restores the very state that auto-stash would squirrel away and
    // reapply — running the stash dance would negate it — so bypass it for those.
    let skip_autostash = git_repo.pending_undo_skips_autostash().unwrap_or(false);
    if !skip_autostash && let Err(e) = git_repo.autostash_save() {
        app.set_error_message(format!("Auto-stash failed: {e}"));
        return Ok(LoopAction::Proceed);
    }
    let outcome = git_repo.undo();
    let restored = if skip_autostash {
        Ok(AutostashRestore::Done)
    } else {
        git_repo.autostash_restore()
    };
    match outcome {
        Ok(UndoOutcome::Done { label }) => Ok(settle_autostash(
            app,
            restored,
            "Undo",
            &format!("Undid {}", label.to_lowercase()),
            LoopAction::Reload,
        )),
        Ok(UndoOutcome::Empty) => {
            app.set_error_message("Nothing to undo");
            Ok(LoopAction::Proceed)
        }
        Ok(UndoOutcome::Stale) => {
            app.set_error_message("Undo history no longer matches the branch — discarded");
            Ok(LoopAction::Reload)
        }
        Err(e) => {
            app.set_error_message(format!("Undo failed: {e}"));
            Ok(LoopAction::Proceed)
        }
    }
}

fn handle_redo(git_repo: &mut impl GitRepo, app: &mut AppState) -> Result<LoopAction> {
    // See handle_undo: skip the auto-stash dance for a working-tree-preserving redo.
    let skip_autostash = git_repo.pending_redo_skips_autostash().unwrap_or(false);
    if !skip_autostash && let Err(e) = git_repo.autostash_save() {
        app.set_error_message(format!("Auto-stash failed: {e}"));
        return Ok(LoopAction::Proceed);
    }
    let outcome = git_repo.redo();
    let restored = if skip_autostash {
        Ok(AutostashRestore::Done)
    } else {
        git_repo.autostash_restore()
    };
    match outcome {
        Ok(UndoOutcome::Done { label }) => Ok(settle_autostash(
            app,
            restored,
            "Redo",
            &format!("Redid {}", label.to_lowercase()),
            LoopAction::Reload,
        )),
        Ok(UndoOutcome::Empty) => {
            app.set_error_message("Nothing to redo");
            Ok(LoopAction::Proceed)
        }
        Ok(UndoOutcome::Stale) => {
            app.set_error_message("Redo history no longer matches the branch — discarded");
            Ok(LoopAction::Reload)
        }
        Err(e) => {
            app.set_error_message(format!("Redo failed: {e}"));
            Ok(LoopAction::Proceed)
        }
    }
}

/// Settle the auto-stash after an operation completed. On a clean reapply, show
/// `success` and return `done`. On a conflict, open the stash-conflict dialog
/// (Esc there aborts the whole operation). On an unexpected restore error, warn
/// the user that the stash was not restored.
///
/// `op_label` titles the conflict dialog (e.g. "Drop" → "after drop").
fn settle_autostash(
    app: &mut AppState,
    restored: Result<AutostashRestore>,
    op_label: &str,
    success: &str,
    done: LoopAction,
) -> LoopAction {
    match restored {
        Ok(AutostashRestore::Done) => {
            app.set_success_message(success.to_string());
            done
        }
        Ok(AutostashRestore::Conflict { files }) => {
            app.enter_stash_conflict(StashConflictState {
                operation_label: op_label.to_string(),
                conflicting_files: files,
                still_unresolved: false,
            });
            LoopAction::Continue
        }
        Err(e) => {
            app.set_error_message(format!("{success}; auto-stash NOT restored: {e}"));
            done
        }
    }
}

fn handle_prepare_split(
    git_repo: &impl GitRepo,
    app: &mut AppState,
    strategy: SplitStrategy,
    commit_oid: Oid,
) -> Result<LoopAction> {
    let head_oid = get_head_oid_or_continue!(git_repo, app);
    let count_result = match strategy {
        SplitStrategy::PerFile => git_repo.count_split_per_file(&commit_oid),
        SplitStrategy::PerHunk => git_repo.count_split_per_hunk(&commit_oid),
        SplitStrategy::PerHunkGroup => {
            git_repo.count_split_per_hunk_group(&commit_oid, &head_oid, &app.reference_oid)
        }
        // "Split out file" is dispatched to its own flow before reaching here.
        SplitStrategy::OutFile => unreachable!("OutFile uses PrepareSplitOutFile"),
    };
    match count_result {
        Err(e) => app.set_error_message(e.to_string()),
        Ok(count) if count > SPLIT_CONFIRM_THRESHOLD => {
            app.enter_split_confirm(strategy, commit_oid, head_oid, count);
        }
        Ok(_) => {
            if execute_split(git_repo, app, strategy, &commit_oid, &head_oid) {
                return Ok(LoopAction::Reload);
            }
        }
    }
    Ok(LoopAction::Proceed)
}

fn handle_prepare_split_out_file(
    git_repo: &impl GitRepo,
    app: &mut AppState,
    commit_oid: Oid,
) -> Result<LoopAction> {
    match git_repo.list_commit_files(&commit_oid) {
        Err(e) => app.set_error_message(e.to_string()),
        Ok(files) if files.len() < 2 => {
            app.set_error_message("Commit touches fewer than 2 files — nothing to split out");
        }
        Ok(files) => app.enter_split_file_select(commit_oid, files),
    }
    Ok(LoopAction::Proceed)
}

fn handle_execute_split_out_file(
    git_repo: &impl GitRepo,
    app: &mut AppState,
    commit_oid: Oid,
    file_path: String,
) -> Result<LoopAction> {
    let head_oid = get_head_oid_or_continue!(git_repo, app);
    match git_repo.split_commit_out_file(&commit_oid, &file_path, &head_oid) {
        Ok(()) => Ok(LoopAction::Reload),
        Err(e) => {
            app.set_error_message(format!("Split failed: {e}"));
            Ok(LoopAction::Proceed)
        }
    }
}

fn handle_execute_drop(
    git_repo: &mut impl GitRepo,
    app: &mut AppState,
    commit_oid: Oid,
    head_oid: Oid,
) -> Result<LoopAction> {
    if let Err(e) = git_repo.autostash_save() {
        app.set_error_message(format!("Auto-stash failed: {e}"));
        return Ok(LoopAction::Proceed);
    }
    let outcome = git_repo.drop_commit(&commit_oid, &head_oid);
    Ok(handle_rebase_outcome(
        git_repo,
        app,
        outcome,
        "Drop",
        "Commit dropped",
    ))
}

fn handle_execute_move(
    git_repo: &mut impl GitRepo,
    app: &mut AppState,
    source_oid: Oid,
    insert_after_oid: Option<Oid>,
) -> Result<LoopAction> {
    let head_oid = get_head_oid_or_continue!(git_repo, app);
    if let Err(e) = git_repo.autostash_save() {
        app.set_error_message(format!("Auto-stash failed: {e}"));
        return Ok(LoopAction::Proceed);
    }
    let outcome = git_repo.move_commit(&source_oid, insert_after_oid.as_ref(), &head_oid);
    Ok(handle_rebase_outcome(
        git_repo,
        app,
        outcome,
        "Move",
        "Commit moved",
    ))
}

fn handle_rebase_abort(
    git_repo: &mut impl GitRepo,
    app: &mut AppState,
    state: ConflictState,
) -> Result<LoopAction> {
    match git_repo.rebase_abort(&state) {
        Ok(()) => {
            let restored = git_repo.autostash_restore();
            let label = state.operation_label.to_lowercase();
            Ok(settle_autostash(
                app,
                restored,
                &state.operation_label,
                &format!("{} aborted", label.trim()),
                LoopAction::Reload,
            ))
        }
        Err(e) => {
            app.set_error_message(format!("Abort failed: {e}"));
            Ok(LoopAction::Proceed)
        }
    }
}

fn handle_rebase_continue(
    git_repo: &mut impl GitRepo,
    app: &mut AppState,
    state: ConflictState,
    terminal_guard: &mut crate::terminal_guard::TerminalGuard,
    kb_enhanced: bool,
) -> Result<LoopAction> {
    let _ = git_repo.auto_stage_resolved_conflicts(&state.conflicting_files);
    if let Some(ref ctx) = state.squash_context {
        let original_oid = state.original_branch_oid.clone();
        let ctx_clone = ctx.clone();
        let conflict_files = git_repo.read_conflicting_files();
        if !conflict_files.is_empty() {
            app.enter_rebase_conflict(git_tailor::repo::ConflictState {
                conflicting_files: conflict_files,
                still_unresolved: true,
                ..state
            });
            return Ok(LoopAction::Continue);
        }
        let final_msg = if ctx.squash_mode.keeps_target_message() {
            ctx.combined_message.clone()
        } else {
            let combined = ctx.combined_message.clone();
            let editor_result = with_tui_suspended(terminal_guard.terminal(), kb_enhanced, || {
                editor::edit_message_in_editor(git_repo, &combined)
            })?;
            match editor_result {
                Err(e) => {
                    let _ = git_repo.rebase_abort(&state);
                    let _ = git_repo.autostash_restore();
                    app.set_error_message(format!("Editor error: {e}"));
                    return Ok(LoopAction::Reload);
                }
                Ok(msg) if msg.trim().is_empty() => {
                    let _ = git_repo.rebase_abort(&state);
                    let _ = git_repo.autostash_restore();
                    let label = &state.operation_label;
                    app.set_error_message(format!("{label} aborted: empty commit message"));
                    return Ok(LoopAction::Continue);
                }
                Ok(msg) => msg,
            }
        };
        let success_msg = match ctx_clone.squash_mode {
            SquashMode::Fixup => "Commit fixed up",
            SquashMode::Squash => "Commits squashed",
        };
        let outcome = git_repo.squash_finalize(&ctx_clone, &final_msg, &original_oid);
        return Ok(handle_rebase_outcome(
            git_repo,
            app,
            outcome,
            "Squash",
            success_msg,
        ));
    }
    let success_msg = format!("Commit {} complete", state.operation_label.to_lowercase());
    let outcome = git_repo.rebase_continue(&state);
    Ok(handle_rebase_outcome(
        git_repo,
        app,
        outcome,
        "Continue",
        &success_msg,
    ))
}

fn handle_run_mergetool(
    git_repo: &impl GitRepo,
    app: &mut AppState,
    files: Vec<String>,
    conflict_state: ConflictState,
    terminal_guard: &mut crate::terminal_guard::TerminalGuard,
    kb_enhanced: bool,
) -> Result<LoopAction> {
    let result = with_tui_suspended(terminal_guard.terminal(), kb_enhanced, || {
        mergetool::run_mergetool(git_repo, &files)
    })?;
    match result {
        Ok(true) => {
            let new_files = git_repo.read_conflicting_files();
            app.mode = AppMode::RebaseConflict(Box::new(git_tailor::repo::ConflictState {
                conflicting_files: new_files,
                still_unresolved: false,
                ..conflict_state
            }));
            app.set_success_message(
                "Merge tool finished \u{2014} press Enter when done or Esc to abort",
            );
        }
        Ok(false) => {
            app.set_error_message("No merge tool configured (set merge.tool in git config)");
        }
        Err(e) => {
            app.set_error_message(format!("Merge tool failed: {e}"));
        }
    }
    Ok(LoopAction::Proceed)
}

fn handle_run_editor(
    git_repo: &impl GitRepo,
    app: &mut AppState,
    files: Vec<String>,
    conflict_state: ConflictState,
    terminal_guard: &mut crate::terminal_guard::TerminalGuard,
    kb_enhanced: bool,
) -> Result<LoopAction> {
    let workdir = git_repo.workdir();
    let result: anyhow::Result<()> =
        with_tui_suspended(terminal_guard.terminal(), kb_enhanced, || {
            let workdir =
                workdir.ok_or_else(|| anyhow::anyhow!("repository has no working directory"))?;
            for file_path in &files {
                editor::open_file_in_editor(git_repo, &workdir.join(file_path))?;
            }
            Ok(())
        })?;
    match result {
        Ok(()) => {
            let new_files = git_repo.read_conflicting_files();
            app.mode = AppMode::RebaseConflict(Box::new(git_tailor::repo::ConflictState {
                conflicting_files: new_files,
                still_unresolved: false,
                ..conflict_state
            }));
            app.set_success_message(
                "Editor finished \u{2014} press Enter when done or Esc to abort",
            );
        }
        Err(e) => {
            app.set_error_message(format!("Editor failed: {e}"));
        }
    }
    Ok(LoopAction::Proceed)
}

/// Run the merge tool (`use_mergetool`) or editor over the files conflicting in
/// an auto-stash reapply, then refresh the stash-conflict dialog.
fn handle_run_stash_tool(
    git_repo: &impl GitRepo,
    app: &mut AppState,
    files: Vec<String>,
    terminal_guard: &mut crate::terminal_guard::TerminalGuard,
    kb_enhanced: bool,
    use_mergetool: bool,
) -> Result<LoopAction> {
    let operation_label = match &app.mode {
        AppMode::StashConflict(s) => s.operation_label.clone(),
        _ => String::new(),
    };
    let tool = if use_mergetool {
        "Merge tool"
    } else {
        "Editor"
    };
    let workdir = git_repo.workdir();
    let result: anyhow::Result<bool> =
        with_tui_suspended(terminal_guard.terminal(), kb_enhanced, || {
            if use_mergetool {
                mergetool::run_mergetool(git_repo, &files)
            } else {
                let workdir = workdir
                    .ok_or_else(|| anyhow::anyhow!("repository has no working directory"))?;
                for file_path in &files {
                    editor::open_file_in_editor(git_repo, &workdir.join(file_path))?;
                }
                Ok(true)
            }
        })?;
    match result {
        Ok(true) => {
            let new_files = git_repo.read_conflicting_files();
            app.mode = AppMode::StashConflict(Box::new(StashConflictState {
                operation_label,
                conflicting_files: new_files,
                still_unresolved: false,
            }));
            app.set_success_message(format!(
                "{tool} finished \u{2014} press Enter when done or Esc to abort"
            ));
        }
        Ok(false) => {
            app.set_error_message("No merge tool configured (set merge.tool in git config)");
        }
        Err(e) => {
            app.set_error_message(format!("{tool} failed: {e}"));
        }
    }
    Ok(LoopAction::Proceed)
}

/// Finish a conflicting auto-stash reapply: drop the stash if everything is
/// resolved, otherwise refresh the dialog with the remaining conflicts.
fn handle_autostash_continue(
    git_repo: &mut impl GitRepo,
    app: &mut AppState,
) -> Result<LoopAction> {
    let operation_label = match &app.mode {
        AppMode::StashConflict(s) => s.operation_label.clone(),
        _ => String::new(),
    };
    match git_repo.autostash_conflict_continue() {
        Ok(AutostashContinue::Resolved) => {
            app.mode = AppMode::CommitList;
            app.set_success_message("Auto-stashed changes restored");
            Ok(LoopAction::ReloadPreserving)
        }
        Ok(AutostashContinue::StillUnresolved { files }) => {
            app.mode = AppMode::StashConflict(Box::new(StashConflictState {
                operation_label,
                conflicting_files: files,
                still_unresolved: true,
            }));
            Ok(LoopAction::Continue)
        }
        Err(e) => {
            app.set_error_message(format!("Failed to finish auto-stash: {e}"));
            Ok(LoopAction::Continue)
        }
    }
}

/// Abort a conflicting auto-stash reapply: rewind the whole operation and put
/// the user's original dirty changes back.
fn handle_autostash_abort(git_repo: &mut impl GitRepo, app: &mut AppState) -> Result<LoopAction> {
    let label = match &app.mode {
        AppMode::StashConflict(s) => s.operation_label.to_lowercase(),
        _ => String::new(),
    };
    match git_repo.autostash_conflict_abort() {
        Ok(()) => {
            app.mode = AppMode::CommitList;
            app.set_success_message(format!(
                "{} aborted \u{2014} changes restored",
                label.trim()
            ));
            Ok(LoopAction::Reload)
        }
        Err(e) => {
            app.set_error_message(format!("Abort failed: {e}"));
            Ok(LoopAction::Continue)
        }
    }
}

fn handle_prepare_reword(
    git_repo: &impl GitRepo,
    app: &mut AppState,
    commit_oid: Oid,
    current_message: String,
    terminal_guard: &mut crate::terminal_guard::TerminalGuard,
    kb_enhanced: bool,
) -> Result<LoopAction> {
    let head_oid = get_head_oid_or_continue!(git_repo, app);
    let editor_result = with_tui_suspended(terminal_guard.terminal(), kb_enhanced, || {
        editor::edit_message_in_editor(git_repo, &current_message)
    })?;
    match editor_result {
        Err(e) => app.set_error_message(format!("Editor error: {e}")),
        Ok(new_message) if new_message.trim().is_empty() => {
            app.set_success_message("Reword cancelled: message is empty");
        }
        Ok(new_message) if new_message == current_message => {
            app.set_success_message("No changes made");
        }
        Ok(new_message) => match git_repo.reword_commit(&commit_oid, &new_message, &head_oid) {
            Ok(()) => return Ok(LoopAction::ReloadPreserving),
            Err(e) => app.set_error_message(format!("Reword failed: {e}")),
        },
    }
    Ok(LoopAction::Proceed)
}

#[allow(clippy::too_many_arguments)]
fn handle_prepare_squash(
    git_repo: &mut impl GitRepo,
    app: &mut AppState,
    source_oid: Oid,
    target_oid: Oid,
    source_message: String,
    target_message: String,
    squash_mode: SquashMode,
    terminal_guard: &mut crate::terminal_guard::TerminalGuard,
    kb_enhanced: bool,
) -> Result<LoopAction> {
    let head_oid = get_head_oid_or_continue!(git_repo, app);
    if let Err(e) = git_repo.autostash_save() {
        app.set_error_message(format!("Auto-stash failed: {e}"));
        return Ok(LoopAction::Proceed);
    }
    let label = squash_mode.label();
    let message_for_context = if squash_mode.keeps_target_message() {
        target_message.clone()
    } else {
        format!("{target_message}\n\n{source_message}")
    };
    let combined = format!("{target_message}\n\n{source_message}");
    match git_repo.squash_try_combine(
        &source_oid,
        &target_oid,
        &message_for_context,
        squash_mode,
        &head_oid,
    ) {
        Ok(Some(conflict_state)) => {
            // Squash-tree conflict — defer restoring the auto-stash until the
            // user resolves and the squash finalizes (or aborts).
            app.enter_rebase_conflict(conflict_state);
            return Ok(LoopAction::Continue);
        }
        Err(e) => {
            let _ = git_repo.autostash_restore();
            app.set_error_message(format!("{label} failed: {e}"));
            return Ok(LoopAction::Continue);
        }
        Ok(None) => {}
    }
    let final_message = if squash_mode.keeps_target_message() {
        Some(target_message)
    } else {
        let editor_result = with_tui_suspended(terminal_guard.terminal(), kb_enhanced, || {
            editor::edit_message_in_editor(git_repo, &combined)
        })?;
        match editor_result {
            Err(e) => {
                let _ = git_repo.autostash_restore();
                app.set_error_message(format!("Editor error: {e}"));
                return Ok(LoopAction::Continue);
            }
            Ok(msg) if msg.trim().is_empty() => {
                let _ = git_repo.autostash_restore();
                app.set_error_message(format!("{label} aborted: empty commit message"));
                return Ok(LoopAction::Continue);
            }
            Ok(msg) => Some(msg),
        }
    };
    if let Some(msg) = final_message {
        let success_msg = match squash_mode {
            SquashMode::Fixup => "Commit fixed up",
            SquashMode::Squash => "Commits squashed",
        };
        let outcome = git_repo.squash_commits(&source_oid, &target_oid, &msg, &head_oid);
        return Ok(handle_rebase_outcome(
            git_repo,
            app,
            outcome,
            label,
            success_msg,
        ));
    }
    Ok(LoopAction::Proceed)
}

/// Render the fragmap to stdout in static (non-TUI) mode and return.
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

/// Number of output commits above which a split requires explicit confirmation.
const SPLIT_CONFIRM_THRESHOLD: usize = 5;

/// Reduce a rebase result to its UI side effect.
///
/// On success, sets a status message and returns `LoopAction::ReloadPreserving`
/// so the caller can trigger a reload with the selection preserved. On conflict,
/// enters conflict mode. On error, sets an error message.
fn handle_rebase_outcome(
    git_repo: &mut impl GitRepo,
    app: &mut AppState,
    outcome: anyhow::Result<RebaseOutcome>,
    op_label: &str,
    success_msg: &str,
) -> LoopAction {
    match outcome {
        Ok(RebaseOutcome::Complete) => {
            // Operation finished — reapply any auto-stash, opening the
            // resolution dialog if it conflicts.
            settle_autostash(
                app,
                git_repo.autostash_restore(),
                op_label,
                success_msg,
                LoopAction::ReloadPreserving,
            )
        }
        Ok(RebaseOutcome::Conflict(state)) => {
            // Defer the auto-stash restore until the conflict is resolved/aborted.
            app.enter_rebase_conflict(*state);
            LoopAction::Continue
        }
        Err(e) => {
            // The operation did not complete — restore the working tree.
            let _ = git_repo.autostash_restore();
            app.set_error_message(format!("{op_label} failed: {e}"));
            LoopAction::Proceed
        }
    }
}

/// Execute a split operation; returns true when a reload is needed.
fn execute_split(
    git_repo: &impl GitRepo,
    app: &mut AppState,
    strategy: SplitStrategy,
    commit_oid: &Oid,
    head_oid: &Oid,
) -> bool {
    match strategy {
        SplitStrategy::PerFile => match git_repo.split_commit_per_file(commit_oid, head_oid) {
            Ok(()) => true,
            Err(e) => {
                app.set_error_message(format!("Split failed: {e}"));
                false
            }
        },
        SplitStrategy::PerHunk => match git_repo.split_commit_per_hunk(commit_oid, head_oid) {
            Ok(()) => true,
            Err(e) => {
                app.set_error_message(format!("Split failed: {e}"));
                false
            }
        },
        SplitStrategy::PerHunkGroup => {
            match git_repo.split_commit_per_hunk_group(commit_oid, head_oid, &app.reference_oid) {
                Ok(()) => true,
                Err(e) => {
                    app.set_error_message(format!("Split failed: {e}"));
                    false
                }
            }
        }
        // "Split out file" is executed via handle_execute_split_out_file.
        SplitStrategy::OutFile => unreachable!("OutFile uses ExecuteSplitOutFile"),
    }
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
        AppMode::SplitSelect { .. } => views::split_select::render(app, frame),
        AppMode::SplitFileSelect { .. } => views::split_file_select::render(app, frame),
        AppMode::SplitConfirm(_) => views::split_select::render_split_confirm(app, frame),
        AppMode::DropConfirm(_) => views::drop::render_drop_confirm(app, frame),
        AppMode::RebaseConflict(_) => views::conflict::render_conflict(app, frame),
        AppMode::StashConflict(_) => views::stash_conflict::render_stash_conflict(app, frame),
        AppMode::RecoverConfirm(_) => views::recover::render_recover(app, frame),
        AppMode::SquashSelect { .. } => views::commit_list::render(app, frame),
        AppMode::MoveSelect { .. } => views::commit_list::render(app, frame),
        AppMode::Help(prev) => views::help::render(prev, app, frame),
    }
}

#[cfg(test)]
mod tests;
