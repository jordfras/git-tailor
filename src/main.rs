// TUI application entry point

use anyhow::Result;
use clap::Parser;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use git2::Repository;
use git_scissors::{app::AppState, event, fragmap, repo, views, CommitInfo};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;

/// Interactive TUI for working with Git commits.
#[derive(Parser)]
#[command(name = "git-scissors")]
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
    // Compute diffs for all commits
    let commit_diffs: Vec<_> = commits
        .iter()
        .filter_map(|commit| repo::commit_diff(&commit.oid).ok())
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
        terminal.draw(|frame| {
            views::commit_list::render(&app, frame);
        })?;

        let event = event::read()?;
        let action = event::parse_key_event(event);

        match action {
            event::AppAction::MoveUp if app.reverse => app.move_down(),
            event::AppAction::MoveDown if app.reverse => app.move_up(),
            event::AppAction::MoveUp => app.move_up(),
            event::AppAction::MoveDown => app.move_down(),
            event::AppAction::ScrollLeft => app.scroll_fragmap_left(),
            event::AppAction::ScrollRight => app.scroll_fragmap_right(),
            event::AppAction::Quit => app.should_quit = true,
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
