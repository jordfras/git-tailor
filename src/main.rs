// TUI application entry point

use anyhow::Result;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use git2::Repository;
use git_scissors::{app::AppState, event, repo, views};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() != 2 {
        anyhow::bail!("Usage: git-scissors <commit-ish>");
    }

    let commit_ish = &args[1];

    let reference_oid = repo::find_reference_point(commit_ish)?;
    let git_repo = Repository::open(".")?;
    let head_oid = git_repo
        .head()?
        .target()
        .ok_or_else(|| anyhow::anyhow!("HEAD does not point to a commit"))?;

    let commits = repo::list_commits(&head_oid.to_string(), &reference_oid)?;

    enable_raw_mode()?;
    let mut stderr = io::stderr();
    execute!(stderr, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stderr);
    let mut terminal = Terminal::new(backend)?;

    let mut app = AppState::with_commits(commits);

    loop {
        terminal.draw(|frame| {
            views::commit_list::render(&app, frame);
        })?;

        let event = event::read()?;
        let action = event::parse_key_event(event);

        match action {
            event::AppAction::MoveUp => app.move_up(),
            event::AppAction::MoveDown => app.move_down(),
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
