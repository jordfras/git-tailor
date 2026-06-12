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

//! Background check for a newer crates.io release.
//!
//! [`UpdatePoller`] spawns a detached thread that asks crates.io whether a newer
//! version of this crate exists (via `update-informer`, which caches its result
//! on disk and hits the network at most once per day). The TUI then drains the
//! result on the next keypress with [`UpdatePoller::poll`], so the network I/O
//! never blocks rendering or input.

use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;

use update_informer::{Check, registry};

/// Spawn the background version check, returning a receiver that yields the new
/// version string exactly once if an update is available. When no update exists
/// (or the check fails) nothing is sent and the sender is dropped, disconnecting
/// the channel.
fn spawn() -> Receiver<String> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let name = env!("CARGO_PKG_NAME");
        let version = env!("CARGO_PKG_VERSION");
        let informer = update_informer::new(registry::Crates, name, version);
        if let Ok(Some(new_version)) = informer.check_version() {
            // Receiver may already be gone if the user quit; ignore send errors.
            let _ = tx.send(new_version.to_string());
        }
    });
    rx
}

/// Polls the background update check without ever blocking.
pub struct UpdatePoller {
    rx: Option<Receiver<String>>,
}

impl UpdatePoller {
    /// Start the background check immediately.
    pub fn new() -> Self {
        Self { rx: Some(spawn()) }
    }

    /// Return `Some(version)` the first time the check reports an available
    /// update, then `None` forever after. Returns `None` while the check is
    /// still running or once it has finished without an update. Never blocks.
    pub fn poll(&mut self) -> Option<String> {
        let rx = self.rx.as_ref()?;
        match rx.try_recv() {
            Ok(version) => {
                self.rx = None;
                Some(version)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.rx = None;
                None
            }
        }
    }
}

impl Default for UpdatePoller {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a poller backed by a caller-supplied channel so the state machine
    /// can be tested without touching the network.
    fn poller_with(rx: Receiver<String>) -> UpdatePoller {
        UpdatePoller { rx: Some(rx) }
    }

    #[test]
    fn poll_returns_none_while_pending() {
        let (_tx, rx) = mpsc::channel::<String>();
        let mut poller = poller_with(rx);
        assert_eq!(poller.poll(), None);
        // Sender still alive, so still pending on a second call.
        assert_eq!(poller.poll(), None);
    }

    #[test]
    fn poll_yields_version_once_then_none() {
        let (tx, rx) = mpsc::channel();
        tx.send("1.2.3".to_string()).unwrap();
        let mut poller = poller_with(rx);
        assert_eq!(poller.poll(), Some("1.2.3".to_string()));
        // Consumed; subsequent polls are a cheap no-op.
        assert_eq!(poller.poll(), None);
        assert_eq!(poller.poll(), None);
    }

    #[test]
    fn poll_returns_none_when_check_finishes_without_update() {
        let (tx, rx) = mpsc::channel::<String>();
        drop(tx); // Thread ended without sending → disconnected.
        let mut poller = poller_with(rx);
        assert_eq!(poller.poll(), None);
        assert_eq!(poller.poll(), None);
    }
}
