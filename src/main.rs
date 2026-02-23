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
    event, fragmap, repo, views, CommitInfo,
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
}

/// Compute fragmap from commits.
///
/// Fetches diffs for all commits and builds the fragmap visualization data.
/// Returns None if any step fails (gracefully handles errors).
fn compute_fragmap(commits: &[CommitInfo]) -> Option<fragmap::FragMap> {
    // Use zero-context diffs so each logical change is its own hunk,
    // matching the original fragmap's fine-grained span tracking
    let commit_diffs: Vec<_> = commits
        .iter()
        .filter_map(|commit| repo::commit_diff_for_fragmap(&commit.oid).ok())
        .collect();

    // If we couldn't get all diffs, return None
    if commit_diffs.len() != commits.len() {
        return None;
    }

    // Build fragmap
    Some(fragmap::build_fragmap(&commit_diffs))
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let reference_oid = repo::find_reference_point(&cli.commit_ish)?;
    let git_repo = Repository::open(".")?;
    let head_oid = git_repo
        .head()?
        .target()
        .ok_or_else(|| anyhow::anyhow!("HEAD does not point to a commit"))?;

    let commits = repo::list_commits(&head_oid.to_string(), &reference_oid)?;

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

    // Compute fragmap for visualization
    let fragmap = compute_fragmap(&commits);

    enable_raw_mode()?;
    let mut stderr = io::stderr();
    execute!(stderr, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stderr);
    let mut terminal = Terminal::new(backend)?;

    let mut app = AppState::with_commits(commits);
    app.reverse = cli.reverse;
    app.fragmap = fragmap;

    loop {
        terminal.draw(|frame| match app.mode {
            AppMode::CommitList => views::commit_list::render(&mut app, frame),
            AppMode::CommitDetail => render_main_view(&mut app, frame),
            AppMode::Help => {
                // Render underlying view first (whatever was showing before help)
                let previous = app.previous_mode.unwrap_or(AppMode::CommitList);
                match previous {
                    AppMode::CommitList => views::commit_list::render(&mut app, frame),
                    AppMode::CommitDetail => render_main_view(&mut app, frame),
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

/// Render the main view with split screen (commit list on left, detail on right).
fn render_main_view(app: &mut AppState, frame: &mut ratatui::Frame) {
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

        views::commit_detail::render(frame, app, right_area);
    } else {
        // Screen too narrow, just show commit list
        views::commit_list::render(app, frame);
    }
}
