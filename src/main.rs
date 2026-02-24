// TUI application entry point

use anyhow::Result;
use clap::Parser;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use git2::Repository;
use git_tailor::{
    app::{AppMode, AppState},
    event, fragmap, repo, views, CommitDiff, CommitInfo,
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
    git_repo: &Repository,
    regular_commits: &[CommitInfo],
    extra_diffs: &[CommitDiff],
    full: bool,
) -> Option<fragmap::FragMap> {
    let mut commit_diffs: Vec<CommitDiff> = regular_commits
        .iter()
        .filter_map(|commit| repo::commit_diff_for_fragmap(git_repo, &commit.oid).ok())
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

    let git_repo = repo::try_open_repo(std::env::current_dir()?)?;
    let reference_oid = repo::find_reference_point(&git_repo, &cli.commit_ish)?;
    let head_oid = git_repo
        .head()?
        .target()
        .ok_or_else(|| anyhow::anyhow!("HEAD does not point to a commit"))?;

    let commits = repo::list_commits(&git_repo, &head_oid.to_string(), &reference_oid)?;

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
    if let Some(d) = repo::staged_diff(&git_repo) {
        extra_diffs.push(d);
    }
    if let Some(d) = repo::unstaged_diff(&git_repo) {
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
        terminal.draw(|frame| match app.mode {
            AppMode::CommitList => views::commit_list::render(&mut app, frame),
            AppMode::CommitDetail => render_main_view(&git_repo, &mut app, frame),
            AppMode::Help => {
                // Render underlying view first (whatever was showing before help)
                let previous = app.previous_mode.unwrap_or(AppMode::CommitList);
                match previous {
                    AppMode::CommitList => views::commit_list::render(&mut app, frame),
                    AppMode::CommitDetail => render_main_view(&git_repo, &mut app, frame),
                    AppMode::Help => views::commit_list::render(&mut app, frame), // Fallback
                }
                // Render help dialog on top
                views::help::render(frame);
            }
        })?;

        let event = event::read()?;
        let action = event::parse_key_event(event);

        match action {
            event::AppAction::MoveUp => match app.mode {
                AppMode::CommitList if app.reverse => app.move_down(),
                AppMode::CommitList => app.move_up(),
                AppMode::CommitDetail => app.scroll_detail_up(),
                AppMode::Help => {} // Ignore in help mode
            },
            event::AppAction::MoveDown => match app.mode {
                AppMode::CommitList if app.reverse => app.move_up(),
                AppMode::CommitList => app.move_down(),
                AppMode::CommitDetail => app.scroll_detail_down(),
                AppMode::Help => {} // Ignore in help mode
            },
            event::AppAction::PageUp => match app.mode {
                AppMode::CommitList if app.reverse => app.page_down(app.commit_list_visible_height),
                AppMode::CommitList => app.page_up(app.commit_list_visible_height),
                AppMode::CommitDetail => app.scroll_detail_page_up(app.detail_visible_height),
                AppMode::Help => {} // Ignore in help mode
            },
            event::AppAction::PageDown => match app.mode {
                AppMode::CommitList if app.reverse => app.page_up(app.commit_list_visible_height),
                AppMode::CommitList => app.page_down(app.commit_list_visible_height),
                AppMode::CommitDetail => app.scroll_detail_page_down(app.detail_visible_height),
                AppMode::Help => {} // Ignore in help mode
            },
            event::AppAction::ScrollLeft => {
                if app.mode != AppMode::Help {
                    app.scroll_fragmap_left();
                }
            }
            event::AppAction::ScrollRight => {
                if app.mode != AppMode::Help {
                    app.scroll_fragmap_right();
                }
            }
            event::AppAction::ToggleDetail => {
                if app.mode != AppMode::Help {
                    app.toggle_detail_view();
                }
            }
            event::AppAction::ShowHelp => app.toggle_help(),
            event::AppAction::Reload => {
                if app.mode != AppMode::Help {
                    reload_commits(&git_repo, &mut app);
                }
            }
            event::AppAction::Quit => match app.mode {
                AppMode::Help => app.close_help(), // Close help dialog
                AppMode::CommitDetail => app.toggle_detail_view(), // Return to commit list
                AppMode::CommitList => app.should_quit = true, // Quit application
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
fn reload_commits(git_repo: &Repository, app: &mut AppState) {
    let head_oid = match git_repo.head().ok().and_then(|h| h.target()) {
        Some(oid) => oid.to_string(),
        None => return,
    };

    let commits = match repo::list_commits(git_repo, &head_oid, &app.reference_oid) {
        Ok(c) => c,
        Err(_) => return,
    };

    let commits: Vec<CommitInfo> = commits
        .into_iter()
        .filter(|c| c.oid != app.reference_oid)
        .collect();

    // Append staged/unstaged as synthetic rows, same as at startup.
    let mut extra_diffs: Vec<CommitDiff> = Vec::new();
    if let Some(d) = repo::staged_diff(git_repo) {
        extra_diffs.push(d);
    }
    if let Some(d) = repo::unstaged_diff(git_repo) {
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
fn render_main_view(git_repo: &Repository, app: &mut AppState, frame: &mut ratatui::Frame) {
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
