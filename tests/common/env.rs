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

//! Exclusive use of the process environment, for the tests that set it or
//! depend on what it lacks.

use std::ffi::OsString;
use std::sync::{Mutex, MutexGuard};

static ENV_LOCK: Mutex<()> = Mutex::new(());

/// What git reads an identity from besides the configuration.
const IDENTITY_VARS: [&str; 7] = [
    "GIT_AUTHOR_NAME",
    "GIT_AUTHOR_EMAIL",
    "GIT_AUTHOR_DATE",
    "GIT_COMMITTER_NAME",
    "GIT_COMMITTER_EMAIL",
    "GIT_COMMITTER_DATE",
    "EMAIL",
];

/// A test's exclusive use of the environment, taken before it does anything
/// else: building a repository reads the environment too. The identity the
/// runner may have exported is cleared, and every variable the turn changes
/// gets its old value back when it ends, so no test sees what another left.
pub struct EnvTurn {
    saved: Vec<(&'static str, Option<OsString>)>,
    _lock: MutexGuard<'static, ()>,
}

impl EnvTurn {
    pub fn take() -> Self {
        let mut turn = Self {
            saved: Vec::new(),
            _lock: ENV_LOCK
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        };
        for name in IDENTITY_VARS {
            turn.save(name);
            // SAFETY: see `set`.
            unsafe { std::env::remove_var(name) };
        }
        turn
    }

    pub fn set(&mut self, vars: &[(&'static str, &str)]) {
        for &(name, value) in vars {
            self.save(name);
            // SAFETY: every test in a binary that uses this holds its turn
            // from its first line to its last, so no other thread reads the
            // environment while it changes. Under nextest, each test is a
            // process of its own.
            unsafe { std::env::set_var(name, value) };
        }
    }

    /// Keep what `name` held before this turn first changed it.
    fn save(&mut self, name: &'static str) {
        if !self.saved.iter().any(|&(saved, _)| saved == name) {
            self.saved.push((name, std::env::var_os(name)));
        }
    }
}

impl Drop for EnvTurn {
    fn drop(&mut self) {
        for (name, value) in &self.saved {
            // SAFETY: as in `set`; the turn is still held.
            unsafe {
                match value {
                    Some(value) => std::env::set_var(name, value),
                    None => std::env::remove_var(name),
                }
            }
        }
    }
}
