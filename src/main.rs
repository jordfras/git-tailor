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

use anyhow::Result;
use clap::Parser;
use crossterm::{
    event::{KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use git_tailor::repo::{Git2Repo, GitRepo, RebaseOutcome};
use git_tailor::{
    CommitDiff, CommitInfo,
    app::{self, AppAction, AppMode, AppState, SplitStrategy},
    editor, fragmap,
    fragmap::SquashableScope,
    mergetool, views,
    views::theme::Theme,
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;

use crate::cli::Cli;

/// Fetch HEAD OID from the repo, setting an error message and continuing the
/// event loop if the call fails.
///
/// Only valid inside the `loop { … }` in `main`.
macro_rules! get_head_oid_or_continue {
    ($git_repo:expr, $app:expr) => {
        match $git_repo.head_oid() {
            Ok(oid) => oid,
            Err(e) => {
                $app.set_error_message(format!("Failed to get HEAD: {e}"));
                continue;
            }
        }
    };
}

/// Suspend the TUI, run `f`, then restore the TUI unconditionally.
///
/// Undoes the full setup done in `main` before calling `f` (leaves alternate
/// screen, disables raw mode, pops keyboard-enhancement flags), then mirrors
/// each step in reverse so the TUI is always left in a working state.
///
/// On Windows the console input mode is saved and restored explicitly because
/// crossterm's `enable_raw_mode()` reads the *current* mode and only clears
/// three bits. If the editor process changes the mode (e.g. sets
/// `ENABLE_VIRTUAL_TERMINAL_INPUT`), that change would persist and break arrow
/// key handling — arrow keys would arrive as escape-sequence characters
/// instead of virtual-key-code events.
fn with_external_process<T>(kb_enhanced: bool, f: impl FnOnce() -> T) -> T {
    #[cfg(windows)]
    let saved_mode = save_console_input_mode();

    if kb_enhanced {
        let _ = execute!(io::stderr(), PopKeyboardEnhancementFlags);
    }
    let _ = disable_raw_mode();
    let _ = execute!(io::stderr(), LeaveAlternateScreen);

    let result = f();

    let _ = execute!(io::stderr(), EnterAlternateScreen);

    // On Windows, restore the exact console input mode we saved rather than
    // relying on enable_raw_mode() which would preserve any mode changes
    // the editor made. On other platforms enable_raw_mode() uses saved
    // termios state and restores correctly.
    #[cfg(windows)]
    {
        if let Some(mode) = saved_mode {
            restore_console_input_mode(mode);
        } else {
            let _ = enable_raw_mode();
        }
    }
    #[cfg(not(windows))]
    {
        let _ = enable_raw_mode();
    }

    if kb_enhanced {
        let _ = execute!(
            io::stderr(),
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        );
    }
    result
}

/// Run a function that takes over the terminal (editor, mergetool), then
/// clear the terminal so the next TUI draw starts from a clean buffer.
fn run_external_tool<T>(
    terminal: &mut Terminal<CrosstermBackend<io::Stderr>>,
    kb_enhanced: bool,
    f: impl FnOnce() -> T,
) -> io::Result<T> {
    let result = with_external_process(kb_enhanced, f);
    terminal.clear()?;
    Ok(result)
}

/// Save the current Windows console input mode so it can be restored exactly
/// after an external process. Returns `None` if the console handle cannot be
/// obtained (e.g. when stdin is not a console).
#[cfg(windows)]
fn save_console_input_mode() -> Option<u32> {
    use winapi::um::{
        consoleapi::GetConsoleMode, processenv::GetStdHandle, winbase::STD_INPUT_HANDLE,
    };
    unsafe {
        let handle = GetStdHandle(STD_INPUT_HANDLE);
        if handle.is_null() || handle == winapi::um::handleapi::INVALID_HANDLE_VALUE {
            return None;
        }
        let mut mode: u32 = 0;
        if GetConsoleMode(handle, &mut mode) != 0 {
            Some(mode)
        } else {
            None
        }
    }
}

/// Restore a previously saved Windows console input mode.
#[cfg(windows)]
fn restore_console_input_mode(mode: u32) {
    use winapi::um::{
        consoleapi::SetConsoleMode, processenv::GetStdHandle, winbase::STD_INPUT_HANDLE,
    };
    unsafe {
        let handle = GetStdHandle(STD_INPUT_HANDLE);
        if !handle.is_null() && handle != winapi::um::handleapi::INVALID_HANDLE_VALUE {
            let _ = SetConsoleMode(handle, mode);
        }
    }
}

/// Compute fragmap from a list of regular commits plus any pre-computed extra diffs.
///
/// Extra diffs are for synthetic pseudo-commits (staged/unstaged working-tree
/// changes) whose diff cannot be fetched by OID. They are appended at the end
/// of the regular commit diffs so the fragmap matrix rows match the ordering in
/// `AppState::commits`.
fn compute_fragmap(
    git_repo: &impl GitRepo,
    regular_commits: &[CommitInfo],
    extra_diffs: &[CommitDiff],
    full: bool,
) -> Option<fragmap::FragMap> {
    let mut commit_diffs: Vec<CommitDiff> = regular_commits
        .iter()
        .filter_map(|commit| git_repo.commit_diff_for_fragmap(&commit.oid).ok())
        .collect();

    // If we couldn't get all diffs, return None
    if commit_diffs.len() != regular_commits.len() {
        return None;
    }

    commit_diffs.extend_from_slice(extra_diffs);
    Some(fragmap::build_fragmap(&commit_diffs, !full))
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let git_repo = Git2Repo::open(std::env::current_dir()?)?;
    let head_oid = git_repo.head_oid()?;

    let (commits, reference_oid, include_reference_oid) = if cli.all {
        let root_oid = git_repo.root_commit_oid()?;
        let all_commits = git_repo.list_commits(&head_oid, &root_oid)?;
        if all_commits.is_empty() {
            eprintln!("No commits to display.");
            return Ok(());
        }
        (all_commits, root_oid, true)
    } else {
        let base = cli.base.unwrap_or_else(|| {
            git_repo
                .default_branch()
                .unwrap_or_else(|| "main".to_string())
        });
        let reference_oid = git_repo.find_reference_point(&base)?;
        let raw = git_repo.list_commits(&head_oid, &reference_oid)?;
        // Exclude the merge-base commit — it's shared with the target branch
        // and must not be modified (squashed, moved, or split).
        let commits: Vec<CommitInfo> = raw.into_iter().filter(|c| c.oid != reference_oid).collect();
        if commits.is_empty() {
            eprintln!(
                "No commits to display: HEAD is at the merge-base with '{}'",
                base
            );
            eprintln!("The current branch has no commits beyond the common ancestor.");
            return Ok(());
        }
        (commits, reference_oid, false)
    };

    if cli.static_output {
        let mut commit_diffs: Vec<CommitDiff> = commits
            .iter()
            .filter_map(|c| git_repo.commit_diff_for_fragmap(&c.oid).ok())
            .collect();
        if commit_diffs.len() != commits.len() {
            anyhow::bail!("Failed to load diffs for all commits");
        }
        if let Some(d) = git_repo.staged_diff() {
            commit_diffs.push(d);
        }
        if let Some(d) = git_repo.unstaged_diff() {
            commit_diffs.push(d);
        }
        print!(
            "{}",
            git_tailor::static_views::fragmap::render(
                &commit_diffs,
                cli.full,
                !cli.no_color,
                cli.reverse,
                cli.squashable_scope.unwrap_or(SquashableScope::Commit),
                crossterm::terminal::size().ok().map(|(w, _)| w),
            )
        );
        return Ok(());
    }

    enable_raw_mode()?;
    let mut stderr = io::stderr();
    execute!(stderr, EnterAlternateScreen)?;
    let kb_enhanced = crossterm::terminal::supports_keyboard_enhancement().unwrap_or(false);
    if kb_enhanced {
        execute!(
            stderr,
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        )?;
    }
    let backend = CrosstermBackend::new(stderr);
    let mut terminal = Terminal::new(backend)?;

    let mut app = AppState::with_commits(commits);
    app.reverse = cli.reverse;
    app.squashable_scope = cli.squashable_scope.unwrap_or(SquashableScope::Group);
    app.theme = cli.theme.unwrap_or(Theme::Plain);
    app.reference_oid = reference_oid;
    app.include_reference_oid = include_reference_oid;

    // Append staged/unstaged working-tree changes as synthetic rows at the
    // bottom of the commit list (newest position). Recompute fragmap with
    // the extra diffs so their hunk overlap with commits is visible.
    let mut extra_diffs: Vec<CommitDiff> = Vec::new();
    if let Some(d) = git_repo.staged_diff() {
        extra_diffs.push(d);
    }
    if let Some(d) = git_repo.unstaged_diff() {
        extra_diffs.push(d);
    }
    let n_regular = app.commits.len();
    for d in &extra_diffs {
        app.commits.push(d.commit.clone());
    }
    app.full_fragmap = cli.full;
    app.fragmap = compute_fragmap(&git_repo, &app.commits[..n_regular], &extra_diffs, cli.full);
    app.selection_index = select_initial_index(&app.commits);

    loop {
        terminal.draw(|frame| {
            let mode = app.mode.clone();
            render_mode(&mode, &git_repo, &mut app, frame);
        })?;

        let event = app::read_event()?;

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
            break;
        }

        // Mode-first dispatch: each view module handles its own actions.
        let mode = app.mode.clone();
        let result = match mode {
            AppMode::CommitList => views::commit_list::handle_key(action, &mut app),
            AppMode::CommitDetail => views::commit_detail::handle_key(action, &mut app),
            AppMode::SplitSelect { .. } => views::split_select::handle_key(action, &mut app),
            AppMode::SplitConfirm(_) => views::split_select::handle_confirm_key(action, &mut app),
            AppMode::DropConfirm(_) => views::drop::handle_confirm_key(action, &mut app),
            AppMode::RebaseConflict(_) => views::conflict::handle_conflict_key(action, &mut app),
            AppMode::SquashSelect { .. } => views::squash_select::handle_key(action, &mut app),
            AppMode::MoveSelect { .. } => views::move_select::handle_key(action, &mut app),
            AppMode::Help(_) => views::help::handle_key(action, &mut app),
        };

        // Handle side effects that require git operations or terminal access.
        match result {
            AppAction::Handled => {}
            AppAction::Quit => app.should_quit = true,
            AppAction::ReloadCommits => reload_commits(&git_repo, &mut app),
            AppAction::PrepareSplit {
                strategy,
                commit_oid,
            } => {
                let head_oid = get_head_oid_or_continue!(git_repo, app);
                let count_result = match strategy {
                    SplitStrategy::PerFile => git_repo.count_split_per_file(&commit_oid),
                    SplitStrategy::PerHunk => git_repo.count_split_per_hunk(&commit_oid),
                    SplitStrategy::PerHunkGroup => git_repo.count_split_per_hunk_group(
                        &commit_oid,
                        &head_oid,
                        &app.reference_oid,
                    ),
                };
                match count_result {
                    Err(e) => app.set_error_message(e.to_string()),
                    Ok(count) if count > SPLIT_CONFIRM_THRESHOLD => {
                        app.enter_split_confirm(strategy, commit_oid, head_oid, count);
                    }
                    Ok(_) => {
                        execute_split(&git_repo, &mut app, strategy, &commit_oid, &head_oid);
                    }
                }
            }
            AppAction::ExecuteSplit {
                strategy,
                commit_oid,
                head_oid,
            } => {
                execute_split(&git_repo, &mut app, strategy, &commit_oid, &head_oid);
            }
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
            } => {
                let outcome = git_repo.drop_commit(&commit_oid, &head_oid);
                handle_rebase_outcome(&git_repo, &mut app, outcome, "Drop", "Commit dropped");
            }
            AppAction::RebaseContinue(state) => {
                // Auto-stage files the user resolved in an external editor
                // so that the index reflects the working-tree state.
                let _ = git_repo.auto_stage_resolved_conflicts(&state.conflicting_files);

                // Squash-time tree conflict: the user has resolved the
                // combined tree. For squash, open the editor; for fixup,
                // use the stored target message directly.
                if let Some(ref ctx) = state.squash_context {
                    let original_oid = state.original_branch_oid.clone();
                    let ctx_clone = ctx.clone();

                    // Check that the index is actually resolved before
                    // launching the editor.
                    let conflict_files = git_repo.read_conflicting_files();
                    if !conflict_files.is_empty() {
                        app.enter_rebase_conflict(git_tailor::repo::ConflictState {
                            conflicting_files: conflict_files,
                            still_unresolved: true,
                            ..state
                        });
                        continue;
                    }

                    // Fixup: skip the editor and use the stored target message.
                    // Squash: open the editor with the combined message.
                    let final_msg = if ctx.is_fixup {
                        ctx.combined_message.clone()
                    } else {
                        let combined = ctx.combined_message.clone();
                        let editor_result = run_external_tool(&mut terminal, kb_enhanced, || {
                            editor::edit_message_in_editor(&git_repo, &combined)
                        })?;
                        match editor_result {
                            Err(e) => {
                                let _ = git_repo.rebase_abort(&state);
                                reload_commits(&git_repo, &mut app);
                                app.set_error_message(format!("Editor error: {e}"));
                                continue;
                            }
                            Ok(msg) if msg.trim().is_empty() => {
                                let _ = git_repo.rebase_abort(&state);
                                reload_commits(&git_repo, &mut app);
                                let label = &state.operation_label;
                                app.set_error_message(format!(
                                    "{label} aborted: empty commit message"
                                ));
                                continue;
                            }
                            Ok(msg) => msg,
                        }
                    };

                    let success_msg = if ctx_clone.is_fixup {
                        "Commit fixed up"
                    } else {
                        "Commits squashed"
                    };
                    let outcome = git_repo.squash_finalize(&ctx_clone, &final_msg, &original_oid);
                    handle_rebase_outcome(&git_repo, &mut app, outcome, "Squash", success_msg);
                    continue;
                }

                let success_msg =
                    format!("Commit {} complete", state.operation_label.to_lowercase());
                let outcome = git_repo.rebase_continue(&state);
                handle_rebase_outcome(&git_repo, &mut app, outcome, "Continue", &success_msg);
            }
            AppAction::RebaseAbort(state) => match git_repo.rebase_abort(&state) {
                Ok(()) => {
                    reload_commits(&git_repo, &mut app);
                    let label = state.operation_label.to_lowercase();
                    app.set_success_message(format!("{} aborted", label.trim()));
                }
                Err(e) => {
                    app.set_error_message(format!("Abort failed: {e}"));
                }
            },
            AppAction::RunMergetool {
                files,
                conflict_state,
            } => {
                let result = run_external_tool(&mut terminal, kb_enhanced, || {
                    mergetool::run_mergetool(&git_repo, &files)
                })?;
                match result {
                    Ok(true) => {
                        let new_files = git_repo.read_conflicting_files();
                        app.mode =
                            AppMode::RebaseConflict(Box::new(git_tailor::repo::ConflictState {
                                conflicting_files: new_files,
                                still_unresolved: false,
                                ..conflict_state
                            }));
                        app.set_success_message(
                            "Merge tool finished — press Enter when done or Esc to abort",
                        );
                    }
                    Ok(false) => {
                        app.set_error_message(
                            "No merge tool configured (set merge.tool in git config)",
                        );
                    }
                    Err(e) => {
                        app.set_error_message(format!("Merge tool failed: {e}"));
                    }
                }
            }
            AppAction::RunEditor {
                files,
                conflict_state,
            } => {
                let workdir = git_repo.workdir();
                let result: anyhow::Result<()> =
                    run_external_tool(&mut terminal, kb_enhanced, || {
                        let workdir = workdir.ok_or_else(|| {
                            anyhow::anyhow!("repository has no working directory")
                        })?;
                        for file_path in &files {
                            editor::open_file_in_editor(&git_repo, &workdir.join(file_path))?;
                        }
                        Ok(())
                    })?;
                match result {
                    Ok(()) => {
                        let new_files = git_repo.read_conflicting_files();
                        app.mode =
                            AppMode::RebaseConflict(Box::new(git_tailor::repo::ConflictState {
                                conflicting_files: new_files,
                                still_unresolved: false,
                                ..conflict_state
                            }));
                        app.set_success_message(
                            "Editor finished — press Enter when done or Esc to abort",
                        );
                    }
                    Err(e) => {
                        app.set_error_message(format!("Editor failed: {e}"));
                    }
                }
            }
            AppAction::PrepareReword {
                commit_oid,
                current_message,
            } => {
                let head_oid = get_head_oid_or_continue!(git_repo, app);
                let editor_result = run_external_tool(&mut terminal, kb_enhanced, || {
                    editor::edit_message_in_editor(&git_repo, &current_message)
                })?;
                match editor_result {
                    Err(e) => app.set_error_message(format!("Editor error: {e}")),
                    Ok(new_message) if new_message == current_message => {}
                    Ok(new_message) => {
                        match git_repo.reword_commit(&commit_oid, &new_message, &head_oid) {
                            Ok(()) => reload_preserving_selection(&git_repo, &mut app),
                            Err(e) => app.set_error_message(format!("Reword failed: {e}")),
                        }
                    }
                }
            }
            AppAction::PrepareSquash {
                source_oid,
                target_oid,
                source_message,
                target_message,
                is_fixup,
            } => {
                let head_oid = get_head_oid_or_continue!(git_repo, app);

                let label = if is_fixup { "Fixup" } else { "Squash" };
                // For fixup, use only the target message; for squash use the
                // combined message so it is shown in the editor.
                let message_for_context = if is_fixup {
                    target_message.clone()
                } else {
                    format!("{target_message}\n\n{source_message}")
                };
                let combined = format!("{target_message}\n\n{source_message}");

                // Try the tree combination first. If it conflicts, let the
                // user resolve before opening the editor (T080).
                match git_repo.squash_try_combine(
                    &source_oid,
                    &target_oid,
                    &message_for_context,
                    is_fixup,
                    &head_oid,
                ) {
                    Ok(Some(conflict_state)) => {
                        app.enter_rebase_conflict(conflict_state);
                        continue;
                    }
                    Err(e) => {
                        app.set_error_message(format!("{label} failed: {e}"));
                        continue;
                    }
                    Ok(None) => {}
                }

                // Determine the final commit message: fixup keeps target's
                // message as-is; squash opens the editor.
                let final_message = if is_fixup {
                    Some(target_message)
                } else {
                    let editor_result = run_external_tool(&mut terminal, kb_enhanced, || {
                        editor::edit_message_in_editor(&git_repo, &combined)
                    })?;
                    match editor_result {
                        Err(e) => {
                            app.set_error_message(format!("Editor error: {e}"));
                            continue;
                        }
                        Ok(msg) if msg.trim().is_empty() => {
                            app.set_error_message(format!("{label} aborted: empty commit message"));
                            continue;
                        }
                        Ok(msg) => Some(msg),
                    }
                };

                if let Some(msg) = final_message {
                    let success_msg = if is_fixup {
                        "Commit fixed up"
                    } else {
                        "Commits squashed"
                    };
                    let outcome =
                        git_repo.squash_commits(&source_oid, &target_oid, &msg, &head_oid);
                    handle_rebase_outcome(&git_repo, &mut app, outcome, label, success_msg);
                }
            }
            AppAction::ExecuteMove {
                source_oid,
                insert_after_oid,
            } => {
                let head_oid = get_head_oid_or_continue!(git_repo, app);
                let outcome = git_repo.move_commit(&source_oid, &insert_after_oid, &head_oid);
                handle_rebase_outcome(&git_repo, &mut app, outcome, "Move", "Commit moved");
            }
        }

        if app.should_quit {
            break;
        }
    }

    disable_raw_mode()?;
    if kb_enhanced {
        execute!(terminal.backend_mut(), PopKeyboardEnhancementFlags)?;
    }
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    Ok(())
}

/// Number of output commits above which a split requires explicit confirmation.
const SPLIT_CONFIRM_THRESHOLD: usize = 5;

/// Reduce a rebase result to its UI side effect: reload + success message,
/// enter conflict mode, or display an error. The selection index is preserved
/// across the reload so the user stays focused on the same row.
fn handle_rebase_outcome(
    git_repo: &impl GitRepo,
    app: &mut AppState,
    outcome: anyhow::Result<RebaseOutcome>,
    op_label: &str,
    success_msg: &str,
) {
    match outcome {
        Ok(RebaseOutcome::Complete) => {
            reload_preserving_selection(git_repo, app);
            app.set_success_message(success_msg.to_string());
        }
        Ok(RebaseOutcome::Conflict(state)) => {
            app.enter_rebase_conflict(*state);
        }
        Err(e) => {
            app.set_error_message(format!("{op_label} failed: {e}"));
        }
    }
}

/// Reload commits from the repository, then restore the previous selection
/// index clamped to the new list bounds.
fn reload_preserving_selection(git_repo: &impl GitRepo, app: &mut AppState) {
    let saved = app.selection_index;
    reload_commits(git_repo, app);
    app.selection_index = saved.min(app.commits.len().saturating_sub(1));
}

/// Execute a split operation and reload commits on success.
fn execute_split(
    git_repo: &impl GitRepo,
    app: &mut AppState,
    strategy: SplitStrategy,
    commit_oid: &str,
    head_oid: &str,
) {
    match strategy {
        SplitStrategy::PerFile => match git_repo.split_commit_per_file(commit_oid, head_oid) {
            Ok(()) => reload_commits(git_repo, app),
            Err(e) => app.set_error_message(format!("Split failed: {e}")),
        },
        SplitStrategy::PerHunk => match git_repo.split_commit_per_hunk(commit_oid, head_oid) {
            Ok(()) => reload_commits(git_repo, app),
            Err(e) => app.set_error_message(format!("Split failed: {e}")),
        },
        SplitStrategy::PerHunkGroup => {
            match git_repo.split_commit_per_hunk_group(commit_oid, head_oid, &app.reference_oid) {
                Ok(()) => reload_commits(git_repo, app),
                Err(e) => app.set_error_message(format!("Split failed: {e}")),
            }
        }
    }
}

/// Choose the initial selection index for a commit list:
/// unstaged row if present, else staged row if present, else the last commit.
fn select_initial_index(commits: &[CommitInfo]) -> usize {
    if let Some(i) = commits.iter().rposition(|c| c.oid == "unstaged") {
        return i;
    }
    if let Some(i) = commits.iter().rposition(|c| c.oid == "staged") {
        return i;
    }
    commits.len().saturating_sub(1)
}

/// Reload commits from HEAD down to the stored reference OID, then recompute the fragmap.
///
/// Keeps the current selection clamped to the new list bounds. Resets
/// detail scroll so a stale offset does not exceed the new content height.
fn reload_commits(git_repo: &impl GitRepo, app: &mut AppState) {
    let head_oid = match git_repo.head_oid() {
        Ok(oid) => oid,
        Err(_) => return,
    };

    let commits = match git_repo.list_commits(&head_oid, &app.reference_oid) {
        Ok(c) => c,
        Err(_) => return,
    };

    let commits: Vec<CommitInfo> = commits
        .into_iter()
        .filter(|c| app.include_reference_oid || c.oid != app.reference_oid)
        .collect();

    // Append staged/unstaged as synthetic rows, same as at startup.
    let mut extra_diffs: Vec<CommitDiff> = Vec::new();
    if let Some(d) = git_repo.staged_diff() {
        extra_diffs.push(d);
    }
    if let Some(d) = git_repo.unstaged_diff() {
        extra_diffs.push(d);
    }

    let n_regular = commits.len();
    let mut commits = commits;
    for d in &extra_diffs {
        commits.push(d.commit.clone());
    }

    let fragmap = compute_fragmap(
        git_repo,
        &commits[..n_regular],
        &extra_diffs,
        app.full_fragmap,
    );

    app.selection_index = select_initial_index(&commits);
    app.commits = commits;
    app.fragmap = fragmap;
    app.fragmap_scroll_offset = 0;
    app.detail_scroll_offset = 0;
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
        AppMode::CommitList => views::commit_list::render(app, frame),
        AppMode::CommitDetail => views::main_view::render(git_repo, app, frame),
        AppMode::SplitSelect { .. } => views::split_select::render(app, frame),
        AppMode::SplitConfirm(_) => views::split_select::render_split_confirm(app, frame),
        AppMode::DropConfirm(_) => views::drop::render_drop_confirm(app, frame),
        AppMode::RebaseConflict(_) => views::conflict::render_conflict(app, frame),
        AppMode::SquashSelect { .. } => views::commit_list::render(app, frame),
        AppMode::MoveSelect { .. } => views::commit_list::render(app, frame),
        AppMode::Help(_) => views::help::render(app, frame),
    }
}
