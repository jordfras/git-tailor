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
//! [`UpdatePoller`] spawns a detached thread that asks crates.io whether a
//! newer version of this crate exists, reusing a cached answer for a day so
//! only the first run of the day touches the network. The TUI drains the result
//! on the next keypress with [`UpdatePoller::poll`], so the network I/O never
//! blocks rendering or input.
//!
//! The check is hand-rolled rather than delegated to `update-informer` because
//! that crate never sets ureq's `root_certs`, so it always validates TLS
//! against the compiled-in Mozilla roots. Behind a TLS-inspecting corporate
//! proxy — whose private CA is only ever installed in the system trust store —
//! every check then dies with `UnknownIssuer`, and the failure is
//! indistinguishable from being up to date. Its only TLS lever is a
//! `native-tls` feature that would add an OpenSSL build dependency. Issuing the
//! single request here instead lets [`crates_io`] pick the platform verifier,
//! and lets [`cache`] check on the first run rather than seeding itself and
//! staying silent for a full interval.

mod cache;
mod crates_io;

use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::Duration;

use anyhow::{Result, bail};

use crate::update_check::cache::{DEFAULT_INTERVAL, VersionCache};

/// What a finished check found.
enum CheckOutcome {
    UpdateAvailable(String),
    UpToDate,
    Failed(String),
}

/// The newest version on crates.io, served from the cache while it is fresh.
fn latest_version(crate_name: &str, interval: Duration) -> Result<String> {
    let cache = VersionCache::new(crate_name)?;
    if let Some(cached) = cache.read_fresh(interval) {
        return Ok(cached);
    }

    let latest = crates_io::latest_stable_version(crate_name)?;
    // A failed write only costs one extra request next time.
    let _ = cache.write(&latest);
    Ok(latest)
}

/// `Some(latest)` when `latest` is a newer release than `current`. Versions
/// that do not parse as semver yield `None` rather than a spurious notice.
fn newer_release(current: &str, latest: &str) -> Option<String> {
    let current = semver::Version::parse(current).ok()?;
    let candidate = semver::Version::parse(latest).ok()?;
    (candidate > current).then(|| latest.to_string())
}

fn check(crate_name: &str, current_version: &str, interval: Duration) -> CheckOutcome {
    match latest_version(crate_name, interval) {
        Ok(latest) => match newer_release(current_version, &latest) {
            Some(version) => CheckOutcome::UpdateAvailable(version),
            None => CheckOutcome::UpToDate,
        },
        Err(error) => CheckOutcome::Failed(format!("{error:#}")),
    }
}

/// Run the check synchronously and report what happened, bypassing the cache so
/// the answer always reflects the network right now. This is the only place the
/// failure reason is surfaced — the TUI stays silent so a firewalled or offline
/// machine is never nagged.
pub fn run_once() -> Result<()> {
    let current = env!("CARGO_PKG_VERSION");
    match check(env!("CARGO_PKG_NAME"), current, Duration::ZERO) {
        CheckOutcome::UpdateAvailable(version) => {
            println!("Version {version} available (running {current}).");
            Ok(())
        }
        CheckOutcome::UpToDate => {
            println!("git-tailor {current} is up to date.");
            Ok(())
        }
        CheckOutcome::Failed(error) => bail!("update check failed: {error}"),
    }
}

fn spawn() -> Receiver<CheckOutcome> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let outcome = check(
            env!("CARGO_PKG_NAME"),
            env!("CARGO_PKG_VERSION"),
            DEFAULT_INTERVAL,
        );
        // Receiver may already be gone if the user quit; ignore send errors.
        let _ = tx.send(outcome);
    });
    rx
}

/// Polls the background update check without ever blocking.
pub struct UpdatePoller {
    rx: Option<Receiver<CheckOutcome>>,
}

impl UpdatePoller {
    /// Start the background check immediately.
    pub fn new() -> Self {
        Self { rx: Some(spawn()) }
    }

    /// Return `Some(version)` the first time the check reports an available
    /// update, then `None` forever after. Returns `None` while the check is
    /// still running, and for an up-to-date or failed check. Never blocks.
    pub fn poll(&mut self) -> Option<String> {
        let rx = self.rx.as_ref()?;
        match rx.try_recv() {
            Ok(CheckOutcome::UpdateAvailable(version)) => {
                self.rx = None;
                Some(version)
            }
            Ok(CheckOutcome::UpToDate | CheckOutcome::Failed(_)) => {
                self.rx = None;
                None
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
    fn poller_with(rx: Receiver<CheckOutcome>) -> UpdatePoller {
        UpdatePoller { rx: Some(rx) }
    }

    #[test]
    fn poll_returns_none_while_pending() {
        let (_tx, rx) = mpsc::channel::<CheckOutcome>();
        let mut poller = poller_with(rx);
        assert_eq!(poller.poll(), None);
        // Sender still alive, so still pending on a second call.
        assert_eq!(poller.poll(), None);
    }

    #[test]
    fn poll_yields_version_once_then_none() {
        let (tx, rx) = mpsc::channel();
        tx.send(CheckOutcome::UpdateAvailable("1.2.3".to_string()))
            .unwrap();
        let mut poller = poller_with(rx);
        assert_eq!(poller.poll(), Some("1.2.3".to_string()));
        // Consumed; subsequent polls are a cheap no-op.
        assert_eq!(poller.poll(), None);
        assert_eq!(poller.poll(), None);
    }

    #[test]
    fn poll_returns_none_when_up_to_date() {
        let (tx, rx) = mpsc::channel();
        tx.send(CheckOutcome::UpToDate).unwrap();
        let mut poller = poller_with(rx);
        assert_eq!(poller.poll(), None);
    }

    #[test]
    fn a_failed_check_stays_silent() {
        let (tx, rx) = mpsc::channel();
        tx.send(CheckOutcome::Failed("no route to host".to_string()))
            .unwrap();
        let mut poller = poller_with(rx);
        assert_eq!(poller.poll(), None);
    }

    #[test]
    fn poll_returns_none_when_thread_died_without_sending() {
        let (tx, rx) = mpsc::channel::<CheckOutcome>();
        drop(tx);
        let mut poller = poller_with(rx);
        assert_eq!(poller.poll(), None);
        assert_eq!(poller.poll(), None);
    }

    #[test]
    fn a_higher_release_is_offered() {
        assert_eq!(newer_release("2.0.0", "3.0.0"), Some("3.0.0".to_string()));
        assert_eq!(newer_release("2.9.9", "2.10.0"), Some("2.10.0".to_string()));
    }

    #[test]
    fn the_same_or_older_release_is_not_offered() {
        assert_eq!(newer_release("3.0.0", "3.0.0"), None);
        assert_eq!(newer_release("3.0.0", "2.0.0"), None);
    }

    #[test]
    fn a_prerelease_of_the_running_version_is_not_an_update() {
        assert_eq!(newer_release("3.0.0", "3.0.0-rc.1"), None);
    }

    #[test]
    fn unparseable_versions_are_not_offered() {
        assert_eq!(newer_release("2.0.0", "not-a-version"), None);
        assert_eq!(newer_release("nightly", "3.0.0"), None);
    }
}
