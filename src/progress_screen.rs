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

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use git_tailor::app::{AppMode, AppState, LoadingEscape};
use git_tailor::views;

use crate::terminal_guard::TerminalGuard;

/// How often the screen redraws while the message stays the same.
const RENDER_INTERVAL: Duration = Duration::from_millis(16);

/// Shows a long computation's progress and tells it whether to go on.
pub(crate) trait ShowProgress {
    /// Show `message` with `progress`, and return whether to go on: `false`
    /// once `escape`'s key or Ctrl-C is pressed, or when the screen cannot be
    /// drawn.
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
    interrupted: bool,
}

impl<'t> ProgressScreen<'t> {
    pub(crate) fn new(terminal_guard: &'t mut TerminalGuard, title: &'static str) -> Self {
        ProgressScreen {
            terminal_guard,
            title,
            last_render: None,
            render_error: None,
            interrupted: false,
        }
    }

    /// Whether the computation stopped for Ctrl-C, which abandons what it was
    /// part of rather than only skipping this step.
    pub(crate) fn interrupted(&self) -> bool {
        self.interrupted
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
        let Some(key) = waiting_key() else {
            return true;
        };
        self.interrupted |= is_interrupt(&key);
        !stops(&key, escape)
    }
}

/// The key waiting to be read, if any.
fn waiting_key() -> Option<KeyEvent> {
    let Ok(true) = crossterm::event::poll(Duration::ZERO) else {
        return None;
    };
    match crossterm::event::read() {
        Ok(Event::Key(key)) => Some(key),
        _ => None,
    }
}

fn is_interrupt(key: &KeyEvent) -> bool {
    key.kind == KeyEventKind::Press
        && key.code == KeyCode::Char('c')
        && key.modifiers.contains(KeyModifiers::CONTROL)
}

/// Whether `key` stops a computation offering `escape`.
fn stops(key: &KeyEvent, escape: Option<LoadingEscape>) -> bool {
    if key.kind != KeyEventKind::Press {
        return false;
    }
    if is_interrupt(key) {
        return true;
    }
    match escape {
        Some(LoadingEscape::Skip) => matches!(key.code, KeyCode::Char('s') | KeyCode::Char('S')),
        Some(LoadingEscape::Cancel) => key.code == KeyCode::Esc,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ctrl-C stops the commit walk before it, so it stops this too, whether or
    /// not the phase offers a key of its own.
    #[test]
    fn ctrl_c_stops_a_computation() {
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        for escape in [None, Some(LoadingEscape::Skip), Some(LoadingEscape::Cancel)] {
            assert!(stops(&ctrl_c, escape), "{escape:?}");
        }
    }

    #[test]
    fn a_phase_stops_on_its_own_key_only() {
        let s = KeyEvent::from(KeyCode::Char('s'));
        let esc = KeyEvent::from(KeyCode::Esc);
        assert!(stops(&s, Some(LoadingEscape::Skip)));
        assert!(!stops(&esc, Some(LoadingEscape::Skip)));
        assert!(stops(&esc, Some(LoadingEscape::Cancel)));
        assert!(!stops(&s, None));
    }
}
