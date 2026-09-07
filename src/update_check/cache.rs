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

use std::time::Duration;

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
    let _ = (age, interval);
    todo!("implemented in the follow-up commit")
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
}
