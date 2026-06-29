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

//! RAII guard that owns the TUI terminal setup (raw mode, alternate screen,
//! keyboard enhancement flags) and restores them on Drop.

use anyhow::Result;
use crossterm::{
    event::{KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;

/// Owns the terminal-mode side effects done at startup, plus the `Terminal`
/// itself. On Drop the modes are restored unconditionally (errors ignored —
/// best-effort cleanup for the panic / early-return paths). The happy path
/// should call [`Self::shutdown`] explicitly so teardown errors propagate to
/// `main`.
pub struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<io::Stderr>>,
    kb_enhanced: bool,
    /// `true` once shutdown has been performed; suppresses Drop's repeat work.
    finished: bool,
}

impl TerminalGuard {
    /// Mutable access to the underlying `Terminal`.
    pub fn terminal(&mut self) -> &mut Terminal<CrosstermBackend<io::Stderr>> {
        &mut self.terminal
    }

    /// Whether keyboard enhancement flags were pushed at startup.
    pub fn kb_enhanced(&self) -> bool {
        self.kb_enhanced
    }

    /// Run shutdown deterministically, propagating any error. After this call
    /// Drop becomes a no-op.
    pub fn shutdown(mut self) -> Result<()> {
        self.finished = true;
        teardown(self.kb_enhanced)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        // Panic / early-return path: errors cannot be reported meaningfully.
        let _ = teardown(self.kb_enhanced);
    }
}

/// Reverse the side effects of [`setup_terminal`]. Writes to `io::stderr()`
/// directly, which addresses the same fd the `CrosstermBackend` wraps.
fn teardown(kb_enhanced: bool) -> Result<()> {
    if kb_enhanced {
        execute!(io::stderr(), PopKeyboardEnhancementFlags)?;
    }
    execute!(io::stderr(), LeaveAlternateScreen)?;
    disable_raw_mode()?;
    Ok(())
}

/// Enter raw mode + alternate screen and request keyboard enhancement when
/// supported. Returns a [`TerminalGuard`] that owns the constructed
/// `Terminal` and will restore all of these on Drop.
pub fn setup_terminal() -> Result<TerminalGuard> {
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
    install_panic_hook(kb_enhanced);
    let backend = CrosstermBackend::new(stderr);
    let terminal = Terminal::new(backend)?;
    Ok(TerminalGuard {
        terminal,
        kb_enhanced,
        finished: false,
    })
}

/// Restore the terminal *before* the default panic handler runs, so the panic
/// message is printed on the main screen instead of being wiped when the
/// alternate screen is left during unwind (see [`TerminalGuard`]'s Drop).
fn install_panic_hook(kb_enhanced: bool) {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = teardown(kb_enhanced);
        original_hook(info);
    }));
}
