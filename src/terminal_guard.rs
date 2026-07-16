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
use std::io::{self, Write};

/// Owns the terminal-mode side effects done at startup, plus the `Terminal`
/// itself. On Drop the modes are restored unconditionally (errors ignored —
/// best-effort cleanup for the panic / early-return paths). The happy path
/// should call [`Self::shutdown`] explicitly so teardown errors propagate to
/// `main`.
pub(crate) struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<io::Stderr>>,
    kb_enhanced: bool,
    /// The RGB the terminal's default background was overridden to (OSC 11) at
    /// startup, or `None` when left untouched. Reset (OSC 111) on teardown and
    /// re-applied around external tools by the suspend/restore path.
    background: Option<(u8, u8, u8)>,
    /// `true` once shutdown has been performed; suppresses Drop's repeat work.
    finished: bool,
}

impl TerminalGuard {
    /// Mutable access to the underlying `Terminal`.
    pub(crate) fn terminal(&mut self) -> &mut Terminal<CrosstermBackend<io::Stderr>> {
        &mut self.terminal
    }

    /// Whether keyboard enhancement flags were pushed at startup.
    pub(crate) fn kb_enhanced(&self) -> bool {
        self.kb_enhanced
    }

    /// The terminal-background override in effect, if any.
    pub(crate) fn background(&self) -> Option<(u8, u8, u8)> {
        self.background
    }

    /// Run shutdown deterministically, propagating any error. After this call
    /// Drop becomes a no-op.
    pub(crate) fn shutdown(mut self) -> Result<()> {
        self.finished = true;
        teardown(self.kb_enhanced, self.background)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        // Panic / early-return path: errors cannot be reported meaningfully.
        let _ = teardown(self.kb_enhanced, self.background);
    }
}

/// Emit OSC 11 to override the terminal's *default* background, so its window
/// padding around the text grid matches the palette. Shared with the
/// suspend/restore path in `external_tool`.
pub(crate) fn set_terminal_background(
    w: &mut impl Write,
    (r, g, b): (u8, u8, u8),
) -> io::Result<()> {
    write!(w, "\x1b]11;#{r:02x}{g:02x}{b:02x}\x1b\\")?;
    w.flush()
}

/// Emit OSC 111 to reset the terminal's default background to the user's
/// configured value, undoing [`set_terminal_background`].
pub(crate) fn reset_terminal_background(w: &mut impl Write) -> io::Result<()> {
    write!(w, "\x1b]111\x1b\\")?;
    w.flush()
}

/// Reverse the side effects of [`setup_terminal`]. Writes to `io::stderr()`
/// directly, which addresses the same fd the `CrosstermBackend` wraps.
fn teardown(kb_enhanced: bool, background: Option<(u8, u8, u8)>) -> Result<()> {
    if background.is_some() {
        reset_terminal_background(&mut io::stderr())?;
    }
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
pub(crate) fn setup_terminal(background: Option<(u8, u8, u8)>) -> Result<TerminalGuard> {
    enable_raw_mode()?;
    let mut stderr = io::stderr();
    execute!(stderr, EnterAlternateScreen)?;
    // OSC 11: override the terminal's default background so its window padding
    // around the text grid matches the palette instead of showing the user's
    // (possibly light) theme background as a border.
    if let Some(rgb) = background {
        set_terminal_background(&mut stderr, rgb)?;
    }
    let kb_enhanced = crossterm::terminal::supports_keyboard_enhancement().unwrap_or(false);
    if kb_enhanced {
        execute!(
            stderr,
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        )?;
    }
    install_panic_hook(kb_enhanced, background);
    let backend = CrosstermBackend::new(stderr);
    let terminal = Terminal::new(backend)?;
    Ok(TerminalGuard {
        terminal,
        kb_enhanced,
        background,
        finished: false,
    })
}

/// Restore the terminal *before* the default panic handler runs, so the panic
/// message is printed on the main screen instead of being wiped when the
/// alternate screen is left during unwind (see [`TerminalGuard`]'s Drop).
fn install_panic_hook(kb_enhanced: bool, background: Option<(u8, u8, u8)>) {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = teardown(kb_enhanced, background);
        original_hook(info);
    }));
}
