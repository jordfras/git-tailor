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
mod terminal_guard;

use anyhow::Result;
use clap::Parser;
use git_tailor::repo::{ConflictState, Git2Repo, GitRepo, RebaseOutcome};
use git_tailor::{
    CommitDiff, CommitInfo, Oid, VirtualOid,
    app::{self, AppAction, AppMode, AppState, SplitStrategy, SquashMode},
    editor, fragmap,
    fragmap::SquashableScope,
    mergetool, views,
    views::theme::Theme,
};

use crate::cli::Cli;
use crate::external_tool::with_tui_suspended;
use crate::terminal_guard::setup_terminal;

/// Loop-control signal returned by [`dispatch_action`].
enum LoopAction {
    /// Skip the rest of this iteration (equivalent to `continue` in the loop).
    Continue,
    /// Fall through to the post-dispatch checks (e.g. `should_quit`).
    Proceed,
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
        .filter_map(|commit| {
            commit
                .oid
                .as_oid()
                .and_then(|oid| git_repo.commit_diff_for_fragmap(oid).ok())
        })
        .collect();

    // If we couldn't get all diffs, return None
    if commit_diffs.len() != regular_commits.len() {
        return None;
    }

    commit_diffs.extend_from_slice(extra_diffs);
    Some(fragmap::build_fragmap(&commit_diffs, !full))
}

/// Resolve the initial commit list, reference OID, and whether the reference
/// commit itself is included (true for `--all`, false for branch mode).
///
/// Returns `Ok(None)` when there are no commits to display; the caller should
/// exit cleanly. The user-facing explanation has already been printed in that
/// case.
fn load_initial_commits(
    git_repo: &impl GitRepo,
    cli: &Cli,
) -> Result<Option<(Vec<CommitInfo>, Oid, bool)>> {
    let head_oid = git_repo.head_oid()?;

    if cli.all {
        let root_oid = git_repo.root_commit_oid()?;
        let all_commits = git_repo.list_commits(&head_oid, &root_oid)?;
        if all_commits.is_empty() {
            eprintln!("No commits to display.");
            return Ok(None);
        }
        return Ok(Some((all_commits, root_oid, true)));
    }

    let base = cli.base.clone().unwrap_or_else(|| {
        git_repo
            .default_branch()
            .unwrap_or_else(|| "main".to_string())
    });
    let reference_oid = git_repo.find_reference_point(&base)?;
    let raw = git_repo.list_commits(&head_oid, &reference_oid)?;
    // Exclude the merge-base commit — it's shared with the target branch
    // and must not be modified (squashed, moved, or split).
    let commits: Vec<CommitInfo> = raw
        .into_iter()
        .filter(|c| c.oid != VirtualOid::Real(reference_oid.clone()))
        .collect();
    if commits.is_empty() {
        eprintln!(
            "No commits to display: HEAD is at the merge-base with '{}'",
            base
        );
        eprintln!("The current branch has no commits beyond the common ancestor.");
        return Ok(None);
    }
    Ok(Some((commits, reference_oid, false)))
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let git_repo = Git2Repo::open(std::env::current_dir()?)?;

    let Some((commits, reference_oid, include_reference_oid)) =
        load_initial_commits(&git_repo, &cli)?
    else {
        return Ok(());
    };

    if cli.static_output {
        return run_static_output(&git_repo, &commits, &cli);
    }

    let mut terminal_guard = setup_terminal()?;
    let kb_enhanced = terminal_guard.kb_enhanced();

    let mut app = init_app_state(
        &git_repo,
        &cli,
        commits,
        reference_oid,
        include_reference_oid,
    );

    loop {
        terminal_guard.terminal().draw(|frame| {
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

        match dispatch_action(
            result,
            &mut app,
            &git_repo,
            &mut terminal_guard,
            kb_enhanced,
        )? {
            LoopAction::Continue => continue,
            LoopAction::Proceed => {}
        }

        if app.should_quit {
            break;
        }
    }

    terminal_guard.shutdown()?;
    Ok(())
}

/// Handle the side effects requested by a view-module action. Returns whether
/// the caller should `continue` the event loop or fall through to the
/// post-dispatch checks.
fn dispatch_action(
    result: AppAction,
    app: &mut AppState,
    git_repo: &impl GitRepo,
    terminal_guard: &mut crate::terminal_guard::TerminalGuard,
    kb_enhanced: bool,
) -> Result<LoopAction> {
    match result {
        AppAction::Handled => {}
        AppAction::Quit => app.should_quit = true,
        AppAction::ReloadCommits => reload_commits(git_repo, app),
        AppAction::PrepareSplit {
            strategy,
            commit_oid,
        } => return handle_prepare_split(git_repo, app, strategy, commit_oid),
        AppAction::ExecuteSplit {
            strategy,
            commit_oid,
            head_oid,
        } => {
            execute_split(git_repo, app, strategy, &commit_oid, &head_oid);
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
    }
    Ok(LoopAction::Proceed)
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
    };
    match count_result {
        Err(e) => app.set_error_message(e.to_string()),
        Ok(count) if count > SPLIT_CONFIRM_THRESHOLD => {
            app.enter_split_confirm(strategy, commit_oid, head_oid, count);
        }
        Ok(_) => {
            execute_split(git_repo, app, strategy, &commit_oid, &head_oid);
        }
    }
    Ok(LoopAction::Proceed)
}

fn handle_execute_drop(
    git_repo: &impl GitRepo,
    app: &mut AppState,
    commit_oid: Oid,
    head_oid: Oid,
) -> Result<LoopAction> {
    let outcome = git_repo.drop_commit(&commit_oid, &head_oid);
    handle_rebase_outcome(git_repo, app, outcome, "Drop", "Commit dropped");
    Ok(LoopAction::Proceed)
}

fn handle_execute_move(
    git_repo: &impl GitRepo,
    app: &mut AppState,
    source_oid: Oid,
    insert_after_oid: Option<Oid>,
) -> Result<LoopAction> {
    let head_oid = get_head_oid_or_continue!(git_repo, app);
    let outcome = git_repo.move_commit(&source_oid, insert_after_oid.as_ref(), &head_oid);
    handle_rebase_outcome(git_repo, app, outcome, "Move", "Commit moved");
    Ok(LoopAction::Proceed)
}

fn handle_rebase_abort(
    git_repo: &impl GitRepo,
    app: &mut AppState,
    state: ConflictState,
) -> Result<LoopAction> {
    match git_repo.rebase_abort(&state) {
        Ok(()) => {
            reload_commits(git_repo, app);
            let label = state.operation_label.to_lowercase();
            app.set_success_message(format!("{} aborted", label.trim()));
        }
        Err(e) => {
            app.set_error_message(format!("Abort failed: {e}"));
        }
    }
    Ok(LoopAction::Proceed)
}

fn handle_rebase_continue(
    git_repo: &impl GitRepo,
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
                    reload_commits(git_repo, app);
                    app.set_error_message(format!("Editor error: {e}"));
                    return Ok(LoopAction::Continue);
                }
                Ok(msg) if msg.trim().is_empty() => {
                    let _ = git_repo.rebase_abort(&state);
                    reload_commits(git_repo, app);
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
        handle_rebase_outcome(git_repo, app, outcome, "Squash", success_msg);
        return Ok(LoopAction::Continue);
    }
    let success_msg = format!("Commit {} complete", state.operation_label.to_lowercase());
    let outcome = git_repo.rebase_continue(&state);
    handle_rebase_outcome(git_repo, app, outcome, "Continue", &success_msg);
    Ok(LoopAction::Proceed)
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
        Ok(new_message) if new_message == current_message => {}
        Ok(new_message) => match git_repo.reword_commit(&commit_oid, &new_message, &head_oid) {
            Ok(()) => reload_preserving_selection(git_repo, app),
            Err(e) => app.set_error_message(format!("Reword failed: {e}")),
        },
    }
    Ok(LoopAction::Proceed)
}

#[allow(clippy::too_many_arguments)]
fn handle_prepare_squash(
    git_repo: &impl GitRepo,
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
            app.enter_rebase_conflict(conflict_state);
            return Ok(LoopAction::Continue);
        }
        Err(e) => {
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
                app.set_error_message(format!("Editor error: {e}"));
                return Ok(LoopAction::Continue);
            }
            Ok(msg) if msg.trim().is_empty() => {
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
        handle_rebase_outcome(git_repo, app, outcome, label, success_msg);
    }
    Ok(LoopAction::Proceed)
}

/// Render the fragmap to stdout in static (non-TUI) mode and return.
fn run_static_output(git_repo: &impl GitRepo, commits: &[CommitInfo], cli: &Cli) -> Result<()> {
    let mut commit_diffs: Vec<CommitDiff> = commits
        .iter()
        .filter_map(|c| {
            c.oid
                .as_oid()
                .and_then(|oid| git_repo.commit_diff_for_fragmap(oid).ok())
        })
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
    Ok(())
}

/// Build the initial `AppState`: apply CLI options, append staged/unstaged
/// pseudo-commits as synthetic rows, compute the initial fragmap, and pick the
/// initial selection.
fn init_app_state(
    git_repo: &impl GitRepo,
    cli: &Cli,
    commits: Vec<CommitInfo>,
    reference_oid: Oid,
    include_reference_oid: bool,
) -> AppState {
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
    app.fragmap = compute_fragmap(git_repo, &app.commits[..n_regular], &extra_diffs, cli.full);
    app.selection_index = select_initial_index(&app.commits);
    app
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
    commit_oid: &Oid,
    head_oid: &Oid,
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
    if let Some(i) = commits.iter().rposition(|c| c.oid == VirtualOid::Unstaged) {
        return i;
    }
    if let Some(i) = commits.iter().rposition(|c| c.oid == VirtualOid::Staged) {
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
        .filter(|c| {
            app.include_reference_oid || c.oid != VirtualOid::Real(app.reference_oid.clone())
        })
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

#[cfg(test)]
mod tests;
