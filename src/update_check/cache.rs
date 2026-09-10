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

//! On-disk cache of the newest version seen on crates.io.
//!
//! The cache is a single file holding nothing but a version string; its age is
//! the file's mtime, so no timestamp is ever serialised.

use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Result, anyhow};
use etcetera::BaseStrategy;

/// How long a cached version is reused before crates.io is consulted again.
pub const DEFAULT_INTERVAL: Duration = Duration::from_secs(60 * 60 * 24);

/// Whether a cached version may be reused, given its age.
///
/// `age` is `None` when no cache file exists. A missing entry is deliberately
/// *not* fresh, so the first run after an install — or after clearing the cache
/// — actually reaches the network. Seeding the file and staying quiet for a
/// full interval instead (what `update-informer` did) makes the check
/// impossible to exercise by deleting the cache, which hides real failures.
pub fn is_fresh(age: Option<Duration>, interval: Duration) -> bool {
    match age {
        Some(age) => age < interval,
        None => false,
    }
}

/// The cache file for one crate.
pub struct VersionCache {
    path: PathBuf,
}

impl VersionCache {
    /// Place the cache in the platform's conventional cache directory:
    /// `~/.cache` (XDG), `~/Library/Caches`, or `%LOCALAPPDATA%`.
    pub fn new(crate_name: &str) -> Result<Self> {
        let base = etcetera::choose_base_strategy()
            .map_err(|e| anyhow!("cannot locate a cache directory: {e}"))?;
        let dir = base.cache_dir().join("git-tailor");
        fs::create_dir_all(&dir)?;
        Ok(Self {
            path: dir.join(format!("{crate_name}-latest-version")),
        })
    }

    #[cfg(test)]
    fn at(path: PathBuf) -> Self {
        Self { path }
    }

    /// Age of the entry, or `None` when it is missing. An mtime in the future
    /// (clock skew) also reads as `None`, so the next run re-checks and heals it.
    fn age(&self) -> Option<Duration> {
        let metadata = fs::metadata(&self.path).ok()?;
        metadata.modified().ok()?.elapsed().ok()
    }

    /// The cached version, if an entry exists and is younger than `interval`.
    pub fn read_fresh(&self, interval: Duration) -> Option<String> {
        if !is_fresh(self.age(), interval) {
            return None;
        }
        let version = fs::read_to_string(&self.path).ok()?;
        let version = version.trim().to_string();
        (!version.is_empty()).then_some(version)
    }

    /// Store `version`, the write itself becoming the entry's timestamp.
    pub fn write(&self, version: &str) -> io::Result<()> {
        fs::write(&self.path, version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: Duration = Duration::from_secs(60 * 60 * 24);

    #[test]
    fn missing_cache_is_never_fresh() {
        assert!(!is_fresh(None, DAY));
    }

    #[test]
    fn entry_younger_than_the_interval_is_fresh() {
        assert!(is_fresh(Some(Duration::from_secs(60)), DAY));
    }

    #[test]
    fn entry_older_than_the_interval_is_stale() {
        assert!(!is_fresh(Some(DAY + Duration::from_secs(1)), DAY));
    }

    #[test]
    fn entry_exactly_at_the_interval_is_stale() {
        assert!(!is_fresh(Some(DAY), DAY));
    }

    #[test]
    fn a_zero_interval_always_reaches_the_network() {
        assert!(!is_fresh(Some(Duration::ZERO), Duration::ZERO));
    }

    #[test]
    fn round_trips_a_version_through_the_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cache = VersionCache::at(dir.path().join("latest"));

        assert_eq!(cache.read_fresh(DAY), None, "nothing cached yet");
        cache.write("3.0.0").expect("write cache");
        assert_eq!(cache.read_fresh(DAY), Some("3.0.0".to_string()));
    }

    #[test]
    fn a_stale_entry_reads_as_absent() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cache = VersionCache::at(dir.path().join("latest"));
        cache.write("3.0.0").expect("write cache");

        // Just-written, so any non-zero age is already past a zero interval.
        assert_eq!(cache.read_fresh(Duration::ZERO), None);
    }

    #[test]
    fn an_empty_entry_reads_as_absent() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cache = VersionCache::at(dir.path().join("latest"));
        cache.write("   ").expect("write cache");

        assert_eq!(cache.read_fresh(DAY), None);
    }
}
