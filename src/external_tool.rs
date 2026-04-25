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

//! Suspend/restore the TUI around external processes (editor, mergetool).

use crossterm::{
    event::{KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;

/// Suspend the TUI around `f`, then clear the terminal so the next draw
/// starts from a clean buffer. Used for editors, mergetool, and SIGTSTP.
pub fn with_tui_suspended<T>(
    terminal: &mut Terminal<CrosstermBackend<io::Stderr>>,
    kb_enhanced: bool,
    f: impl FnOnce() -> T,
) -> io::Result<T> {
    let result = tui_suspend_restore(kb_enhanced, f);
    terminal.clear()?;
    Ok(result)
}

/// Suspend the TUI, run `f`, then restore the TUI unconditionally.
///
/// Undoes the full setup done in `main` before calling `f` (leaves alternate
/// screen, disables raw mode, pops keyboard-enhancement flags), then mirrors
/// each step in reverse so the TUI is always left in a working state.
///
/// On Windows the console input mode is saved and restored explicitly because
/// crossterm's `enable_raw_mode()` reads the *current* mode and only clears
/// three bits. If the editor process changes the mode (e.g. sets
/// `ENABLE_VIRTUAL_TERMINAL_INPUT`), that change would persist and break arrow
/// key handling — arrow keys would arrive as escape-sequence characters
/// instead of virtual-key-code events.
fn tui_suspend_restore<T>(kb_enhanced: bool, f: impl FnOnce() -> T) -> T {
    #[cfg(windows)]
    let saved_mode = save_console_input_mode();

    if kb_enhanced {
        let _ = execute!(io::stderr(), PopKeyboardEnhancementFlags);
    }
    let _ = disable_raw_mode();
    let _ = execute!(io::stderr(), LeaveAlternateScreen);

    let result = f();

    let _ = execute!(io::stderr(), EnterAlternateScreen);

    // On Windows, restore the exact console input mode we saved rather than
    // relying on enable_raw_mode() which would preserve any mode changes
    // the editor made. On other platforms enable_raw_mode() uses saved
    // termios state and restores correctly.
    #[cfg(windows)]
    {
        if let Some(mode) = saved_mode {
            restore_console_input_mode(mode);
        } else {
            let _ = enable_raw_mode();
        }
    }
    #[cfg(not(windows))]
    {
        let _ = enable_raw_mode();
    }

    if kb_enhanced {
        let _ = execute!(
            io::stderr(),
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        );
    }
    result
}

/// Save the current Windows console input mode so it can be restored exactly
/// after an external process. Returns `None` if the console handle cannot be
/// obtained (e.g. when stdin is not a console).
#[cfg(windows)]
fn save_console_input_mode() -> Option<u32> {
    use winapi::um::{
        consoleapi::GetConsoleMode, processenv::GetStdHandle, winbase::STD_INPUT_HANDLE,
    };
    unsafe {
        let handle = GetStdHandle(STD_INPUT_HANDLE);
        if handle.is_null() || handle == winapi::um::handleapi::INVALID_HANDLE_VALUE {
            return None;
        }
        let mut mode: u32 = 0;
        if GetConsoleMode(handle, &mut mode) != 0 {
            Some(mode)
        } else {
            None
        }
    }
}

/// Restore a previously saved Windows console input mode.
#[cfg(windows)]
fn restore_console_input_mode(mode: u32) {
    use winapi::um::{
        consoleapi::SetConsoleMode, processenv::GetStdHandle, winbase::STD_INPUT_HANDLE,
    };
    unsafe {
        let handle = GetStdHandle(STD_INPUT_HANDLE);
        if !handle.is_null() && handle != winapi::um::handleapi::INVALID_HANDLE_VALUE {
            let _ = SetConsoleMode(handle, mode);
        }
    }
}
