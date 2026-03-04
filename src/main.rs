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

use anyhow::Result;
use clap::Parser;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use git_tailor::repo::{Git2Repo, GitRepo, RebaseOutcome};
use git_tailor::{
    app::{AppMode, AppState, SplitStrategy},
    editor, event, fragmap, mergetool, views, CommitDiff, CommitInfo,
};
use ratatui::{
    backend::CrosstermBackend,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Terminal,
};
use std::io;

/// Interactive TUI for working with Git commits.
#[derive(Parser)]
#[command(name = "gt")]
struct Cli {
    /// A commit-ish to use as the base reference (branch, tag, or hash).
    commit_ish: String,

    /// Display commits in reverse order (HEAD at top).
    #[arg(short, long)]
    reverse: bool,

    /// Show all hunk-group columns without deduplication.
    ///
    /// By default the hunk-group matrix merges columns whose set of touching
    /// commits is identical, producing a compact view. With this flag every
    /// raw hunk cluster gets its own column, which is useful for debugging
    /// the cluster layout.
    #[arg(short = 'f', long)]
    full: bool,
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
    let reference_oid = git_repo.find_reference_point(&cli.commit_ish)?;
    let head_oid = git_repo.head_oid()?;

    let commits = git_repo.list_commits(&head_oid, &reference_oid)?;

    // Exclude the merge-base commit - it's shared with the target branch
    // and must not be modified (squashed, moved, or split)
    let commits: Vec<CommitInfo> = commits
        .into_iter()
        .filter(|c| c.oid != reference_oid)
        .collect();

    // Handle edge case: HEAD is at merge-base (no commits on current branch)
    if commits.is_empty() {
        eprintln!(
            "No commits to display: HEAD is at the merge-base with '{}'",
            cli.commit_ish
        );
        eprintln!("The current branch has no commits beyond the common ancestor.");
        return Ok(());
    }

    enable_raw_mode()?;
    let mut stderr = io::stderr();
    execute!(stderr, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stderr);
    let mut terminal = Terminal::new(backend)?;

    let mut app = AppState::with_commits(commits);
    app.reverse = cli.reverse;
    app.reference_oid = reference_oid;

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
            match app.mode.clone() {
                AppMode::CommitList => views::commit_list::render(&mut app, frame),
                AppMode::CommitDetail => render_main_view(&git_repo, &mut app, frame),
                AppMode::SplitSelect { .. } => {
                    views::commit_list::render(&mut app, frame);
                    views::split_select::render(&app, frame);
                }
                AppMode::SplitConfirm(_) => {
                    views::commit_list::render(&mut app, frame);
                    views::split_select::render_split_confirm(&app, frame);
                }
                AppMode::DropConfirm(_) => {
                    views::commit_list::render(&mut app, frame);
                    views::drop::render_drop_confirm(&app, frame);
                }
                AppMode::DropConflict(_) => {
                    views::commit_list::render(&mut app, frame);
                    views::drop::render_drop_conflict(&app, frame);
                }
                AppMode::Help(prev) => {
                    // Render underlying view first (whatever was showing before help)
                    match *prev {
                        AppMode::CommitList => views::commit_list::render(&mut app, frame),
                        AppMode::CommitDetail => render_main_view(&git_repo, &mut app, frame),
                        AppMode::Help(_)
                        | AppMode::SplitSelect { .. }
                        | AppMode::SplitConfirm(_)
                        | AppMode::DropConfirm(_)
                        | AppMode::DropConflict(_) => views::commit_list::render(&mut app, frame),
                    }
                    // Render help dialog on top
                    views::help::render(frame);
                }
            }
        })?;

        let event = event::read()?;
        let action = event::parse_key_event(event);

        app.clear_status_message();

        match action {
            event::AppAction::MoveUp => match app.mode.clone() {
                AppMode::CommitList if app.reverse => app.move_down(),
                AppMode::CommitList => app.move_up(),
                AppMode::CommitDetail => app.scroll_detail_up(),
                AppMode::SplitSelect { .. } => app.split_select_up(),
                AppMode::Help(_)
                | AppMode::SplitConfirm(_)
                | AppMode::DropConfirm(_)
                | AppMode::DropConflict(_) => {}
            },
            event::AppAction::MoveDown => match app.mode.clone() {
                AppMode::CommitList if app.reverse => app.move_up(),
                AppMode::CommitList => app.move_down(),
                AppMode::CommitDetail => app.scroll_detail_down(),
                AppMode::SplitSelect { .. } => app.split_select_down(),
                AppMode::Help(_)
                | AppMode::SplitConfirm(_)
                | AppMode::DropConfirm(_)
                | AppMode::DropConflict(_) => {}
            },
            event::AppAction::PageUp => match app.mode.clone() {
                AppMode::CommitList if app.reverse => app.page_down(app.commit_list_visible_height),
                AppMode::CommitList => app.page_up(app.commit_list_visible_height),
                AppMode::CommitDetail => app.scroll_detail_page_up(app.detail_visible_height),
                AppMode::Help(_)
                | AppMode::SplitSelect { .. }
                | AppMode::SplitConfirm(_)
                | AppMode::DropConfirm(_)
                | AppMode::DropConflict(_) => {}
            },
            event::AppAction::PageDown => match app.mode.clone() {
                AppMode::CommitList if app.reverse => app.page_up(app.commit_list_visible_height),
                AppMode::CommitList => app.page_down(app.commit_list_visible_height),
                AppMode::CommitDetail => app.scroll_detail_page_down(app.detail_visible_height),
                AppMode::Help(_)
                | AppMode::SplitSelect { .. }
                | AppMode::SplitConfirm(_)
                | AppMode::DropConfirm(_)
                | AppMode::DropConflict(_) => {}
            },
            event::AppAction::ScrollLeft => {
                if matches!(app.mode, AppMode::CommitList | AppMode::CommitDetail) {
                    app.scroll_fragmap_left();
                }
            }
            event::AppAction::ScrollRight => {
                if matches!(app.mode, AppMode::CommitList | AppMode::CommitDetail) {
                    app.scroll_fragmap_right();
                }
            }
            event::AppAction::ToggleDetail => {
                if matches!(app.mode, AppMode::CommitList | AppMode::CommitDetail) {
                    app.toggle_detail_view();
                }
            }
            event::AppAction::ShowHelp => app.toggle_help(),
            event::AppAction::Split => {
                if app.mode == AppMode::CommitList {
                    app.enter_split_select();
                }
            }
            event::AppAction::Drop => {
                if app.mode == AppMode::CommitList {
                    let commit = &app.commits[app.selection_index];
                    if commit.oid == "staged" || commit.oid == "unstaged" {
                        app.set_error_message("Cannot drop staged/unstaged changes");
                    } else {
                        let commit_oid = commit.oid.clone();
                        let commit_summary = commit.summary.clone();
                        let head_oid = match git_repo.head_oid() {
                            Ok(oid) => oid,
                            Err(e) => {
                                app.set_error_message(format!("Failed to get HEAD: {e}"));
                                continue;
                            }
                        };
                        app.enter_drop_confirm(commit_oid, commit_summary, head_oid);
                    }
                }
            }
            event::AppAction::Reword => {
                if app.mode == AppMode::CommitList {
                    let commit = &app.commits[app.selection_index];
                    if commit.oid == "staged" || commit.oid == "unstaged" {
                        app.set_error_message("Cannot reword staged/unstaged changes");
                    } else {
                        let commit_oid = commit.oid.clone();
                        let current_message = commit.message.clone();
                        let head_oid = match git_repo.head_oid() {
                            Ok(oid) => oid,
                            Err(e) => {
                                app.set_error_message(format!("Failed to get HEAD: {e}"));
                                continue;
                            }
                        };
                        let editor_result =
                            editor::edit_message_in_editor(&git_repo, &current_message);
                        // Force a full repaint — ratatui's buffer is stale after the editor
                        // temporarily owned the terminal.
                        terminal.clear()?;
                        match editor_result {
                            Err(e) => app.set_error_message(format!("Editor error: {e}")),
                            Ok(new_message) if new_message == current_message => {}
                            Ok(new_message) => {
                                let saved_index = app.selection_index;
                                match git_repo.reword_commit(&commit_oid, &new_message, &head_oid) {
                                    Ok(()) => {
                                        reload_commits(&git_repo, &mut app);
                                        app.selection_index =
                                            saved_index.min(app.commits.len().saturating_sub(1));
                                    }
                                    Err(e) => app.set_error_message(format!("Reword failed: {e}")),
                                }
                            }
                        }
                    }
                }
            }
            event::AppAction::Confirm => match app.mode.clone() {
                AppMode::SplitSelect { .. } => {
                    let strategy = app.selected_split_strategy();
                    let commit_oid = app.commits[app.selection_index].oid.clone();
                    let head_oid = match git_repo.head_oid() {
                        Ok(oid) => oid,
                        Err(e) => {
                            app.mode = AppMode::CommitList;
                            app.set_error_message(format!("Failed to get HEAD: {e}"));
                            continue;
                        }
                    };
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
                        Err(e) => {
                            app.mode = AppMode::CommitList;
                            app.set_error_message(e.to_string());
                        }
                        Ok(count) if count > SPLIT_CONFIRM_THRESHOLD => {
                            app.enter_split_confirm(strategy, commit_oid, head_oid, count);
                        }
                        Ok(_) => {
                            app.mode = AppMode::CommitList;
                            execute_split(&git_repo, &mut app, strategy, &commit_oid, &head_oid);
                        }
                    }
                }
                AppMode::SplitConfirm(_) => {
                    if let AppMode::SplitConfirm(pending) =
                        std::mem::replace(&mut app.mode, AppMode::CommitList)
                    {
                        let strategy = pending.strategy;
                        let commit_oid = pending.commit_oid;
                        let head_oid = pending.head_oid;
                        execute_split(&git_repo, &mut app, strategy, &commit_oid, &head_oid);
                    }
                }
                AppMode::DropConfirm(_) => {
                    if let AppMode::DropConfirm(pending) =
                        std::mem::replace(&mut app.mode, AppMode::CommitList)
                    {
                        let saved_index = app.selection_index;
                        match git_repo.drop_commit(&pending.commit_oid, &pending.head_oid) {
                            Ok(RebaseOutcome::Complete) => {
                                reload_commits(&git_repo, &mut app);
                                app.selection_index =
                                    saved_index.min(app.commits.len().saturating_sub(1));
                                app.set_success_message("Commit dropped");
                            }
                            Ok(RebaseOutcome::Conflict(state)) => {
                                app.enter_drop_conflict(state);
                            }
                            Err(e) => {
                                app.set_error_message(format!("Drop failed: {e}"));
                            }
                        }
                    }
                }
                AppMode::DropConflict(_) => {
                    if let AppMode::DropConflict(state) =
                        std::mem::replace(&mut app.mode, AppMode::CommitList)
                    {
                        let saved_index = app.selection_index;
                        match git_repo.drop_commit_continue(&state) {
                            Ok(RebaseOutcome::Complete) => {
                                reload_commits(&git_repo, &mut app);
                                app.selection_index =
                                    saved_index.min(app.commits.len().saturating_sub(1));
                                app.set_success_message("Commit dropped");
                            }
                            Ok(RebaseOutcome::Conflict(new_state)) => {
                                app.enter_drop_conflict(new_state);
                            }
                            Err(e) => {
                                app.set_error_message(format!("Continue failed: {e}"));
                            }
                        }
                    }
                }
                AppMode::CommitList | AppMode::CommitDetail => {
                    app.toggle_detail_view();
                }
                AppMode::Help(_) => {}
            },
            event::AppAction::Update => {
                if matches!(app.mode, AppMode::CommitList | AppMode::CommitDetail) {
                    reload_commits(&git_repo, &mut app);
                }
            }
            event::AppAction::Mergetool => {
                if let AppMode::DropConflict(ref state) = app.mode.clone() {
                    let result = mergetool::run_mergetool(&git_repo, &state.conflicting_files);
                    // Force a full repaint — ratatui's buffer is stale after the
                    // tool temporarily owned the terminal.
                    terminal.clear()?;
                    match result {
                        Ok(true) => {
                            // Refresh the conflict file list so the dialog reflects
                            // whatever the tool resolved.
                            let new_files = git_repo.read_conflicting_files();
                            app.mode = AppMode::DropConflict(git_tailor::repo::ConflictState {
                                conflicting_files: new_files,
                                still_unresolved: false,
                                ..state.clone()
                            });
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
            }
            event::AppAction::Quit => match app.mode.clone() {
                AppMode::Help(_) => app.close_help(),
                AppMode::SplitSelect { .. } => app.mode = AppMode::CommitList,
                AppMode::SplitConfirm(_) => app.cancel_split_confirm(),
                AppMode::DropConfirm(_) => app.cancel_drop_confirm(),
                AppMode::DropConflict(_) => {
                    if let AppMode::DropConflict(state) =
                        std::mem::replace(&mut app.mode, AppMode::CommitList)
                    {
                        match git_repo.drop_commit_abort(&state) {
                            Ok(()) => {
                                reload_commits(&git_repo, &mut app);
                                app.set_success_message("Drop aborted");
                            }
                            Err(e) => {
                                app.set_error_message(format!("Abort failed: {e}"));
                            }
                        }
                    }
                }
                AppMode::CommitDetail => app.toggle_detail_view(),
                AppMode::CommitList => app.should_quit = true,
            },
            event::AppAction::None => {}
        }

        if app.should_quit {
            break;
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    Ok(())
}

/// Number of output commits above which a split requires explicit confirmation.
const SPLIT_CONFIRM_THRESHOLD: usize = 5;

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
            Err(e) => app.set_error_message(e.to_string()),
        },
        SplitStrategy::PerHunk => match git_repo.split_commit_per_hunk(commit_oid, head_oid) {
            Ok(()) => reload_commits(git_repo, app),
            Err(e) => app.set_error_message(e.to_string()),
        },
        SplitStrategy::PerHunkGroup => {
            match git_repo.split_commit_per_hunk_group(commit_oid, head_oid, &app.reference_oid) {
                Ok(()) => reload_commits(git_repo, app),
                Err(e) => app.set_error_message(e.to_string()),
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
        .filter(|c| c.oid != app.reference_oid)
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

/// Render the main view with split screen (commit list on left, detail on right).
fn render_main_view(git_repo: &impl GitRepo, app: &mut AppState, frame: &mut ratatui::Frame) {
    let area = frame.area();
    let split_x = 72; // SHA(10) + sep(1) + title(60) + sep(1)
    let left_width = split_x.min(area.width);
    let right_width = area.width.saturating_sub(left_width);

    if right_width > 0 {
        let left_area = Rect {
            x: area.x,
            y: area.y,
            width: left_width,
            height: area.height,
        };
        let right_area = Rect {
            x: area.x + left_width,
            y: area.y,
            width: right_width,
            height: area.height,
        };

        // Temporarily hide fragmap so commit list renders without it
        let saved_fragmap = app.fragmap.take();
        views::commit_list::render_in_area(app, frame, left_area);
        app.fragmap = saved_fragmap;

        // Render separator between left and right
        let sep_height = area.height.saturating_sub(1); // exclude footer row
        let separator_spans: Vec<Line> = (0..sep_height)
            .map(|_| {
                Line::from(Span::styled(
                    "│",
                    Style::new().fg(Color::White).bg(Color::Blue),
                ))
            })
            .collect();
        let sep_area = Rect {
            x: left_area.x + left_width - 1,
            y: area.y,
            width: 1,
            height: sep_height,
        };
        frame.render_widget(Paragraph::new(separator_spans), sep_area);

        views::commit_detail::render(git_repo, frame, app, right_area);
    } else {
        // Screen too narrow, just show commit list
        views::commit_list::render(app, frame);
    }
}
