// TUI application entry point

use anyhow::Result;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use git_scissors::{app::AppState, event, views};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;

fn main() -> Result<()> {
    enable_raw_mode()?;
    let mut stderr = io::stderr();
    execute!(stderr, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stderr);
    let mut terminal = Terminal::new(backend)?;

    let mut app = AppState::new();

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
