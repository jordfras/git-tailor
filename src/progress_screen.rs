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

//! The loading screen a long computation keeps up to date.

use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind};
use git_tailor::app::{AppMode, AppState, LoadingEscape};
use git_tailor::views;

use crate::terminal_guard::TerminalGuard;

/// How often the screen redraws while the message stays the same.
const RENDER_INTERVAL: Duration = Duration::from_millis(16);

/// Shows a long computation's progress and tells it whether to go on.
pub(crate) trait ShowProgress {
    /// Show `message` with `progress`, and return whether to go on: `false`
    /// once `escape`'s key is pressed, or when the screen cannot be drawn.
    fn show(
        &mut self,
        app: &mut AppState,
        message: &'static str,
        progress: Option<(usize, usize)>,
        escape: Option<LoadingEscape>,
    ) -> bool;
}

/// The loading screen, redrawn on a new message and otherwise at most every
/// [`RENDER_INTERVAL`], which is also how often it looks for the escape key.
pub(crate) struct ProgressScreen<'t> {
    terminal_guard: &'t mut TerminalGuard,
    title: &'static str,
    last_render: Option<(Instant, &'static str)>,
    render_error: Option<anyhow::Error>,
}

impl<'t> ProgressScreen<'t> {
    pub(crate) fn new(terminal_guard: &'t mut TerminalGuard, title: &'static str) -> Self {
        ProgressScreen {
            terminal_guard,
            title,
            last_render: None,
            render_error: None,
        }
    }

    /// The error drawing the screen ran into, once the computation is over.
    pub(crate) fn finish(self) -> anyhow::Result<()> {
        self.render_error.map_or(Ok(()), Err)
    }
}

impl ShowProgress for ProgressScreen<'_> {
    fn show(
        &mut self,
        app: &mut AppState,
        message: &'static str,
        progress: Option<(usize, usize)>,
        escape: Option<LoadingEscape>,
    ) -> bool {
        let now = Instant::now();
        if let Some((at, shown)) = self.last_render
            && shown == message
            && now.duration_since(at) < RENDER_INTERVAL
        {
            return true;
        }
        self.last_render = Some((now, message));
        app.mode = AppMode::Loading {
            title: self.title,
            message,
            progress,
            escape,
        };
        if let Err(e) = self
            .terminal_guard
            .terminal()
            .draw(|frame| views::loading::render(app, frame))
        {
            self.render_error = Some(e.into());
            return false;
        }
        !escape.is_some_and(pressed)
    }
}

/// Whether `escape`'s key is waiting to be read.
fn pressed(escape: LoadingEscape) -> bool {
    let Ok(true) = crossterm::event::poll(Duration::ZERO) else {
        return false;
    };
    let Ok(Event::Key(KeyEvent {
        code,
        kind: KeyEventKind::Press,
        ..
    })) = crossterm::event::read()
    else {
        return false;
    };
    match escape {
        LoadingEscape::Skip => matches!(code, KeyCode::Char('s') | KeyCode::Char('S')),
        LoadingEscape::Cancel => code == KeyCode::Esc,
    }
}
