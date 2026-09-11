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

//! One git-tailor at a time per working tree.
//!
//! The journal records an operation as in progress so a crash can be recovered
//! from. Nothing in that record says whether the process that wrote it is still
//! alive, so a second git-tailor started while the first sits on a conflict
//! dialog reads a *live* operation as a crashed one — and "recovering" it
//! rewinds the branch under the instance still working on it, which then
//! completes against state that moved beneath it.
//!
//! An advisory lock held for the session supplies the missing fact. The usual
//! objection to lock files — a crash leaves one behind and the next run is stuck
//! — does not apply: the operating system drops this one when the process dies,
//! however it dies. So holding it means "someone is running here now", and
//! *failing* to take it is the only thing that has to be reported.
//!
//! Per working tree, not per repository: the journal is per working tree, so two
//! linked worktrees may run at once and only share what git itself serializes.

use anyhow::{Context, Result};
use std::fs::{File, TryLockError};
use std::path::Path;

/// Held for as long as git-tailor is running in a working tree.
///
/// Dropping it releases the lock; so does the process exiting for any reason,
/// which is the point.
pub(crate) struct SessionLock {
    /// Kept solely to hold the lock; dropping the file releases it. `None` when
    /// the filesystem could not provide one — see [`LockRefusal::Unavailable`].
    _file: Option<File>,
}

/// Why a session lock could not be taken.
#[derive(Debug)]
pub(crate) enum LockRefusal {
    /// Another git-tailor holds it. The only answer is to wait for it.
    Busy,
    /// The lock could not be created or taken at all — a read-only `.git`, or a
    /// filesystem that does not implement locking, which some network and FUSE
    /// mounts still do not.
    ///
    /// Not a reason to stop: without a lock git-tailor behaves exactly as it did
    /// before there was one. Refusing to run would trade "no protection" for "no
    /// tool", which is the worse of the two.
    Unavailable(anyhow::Error),
}

impl SessionLock {
    /// Stands in for a lock that could not be taken, so the caller has one
    /// value to hold either way.
    pub(crate) fn unlocked() -> Self {
        Self { _file: None }
    }

    /// Take the lock for the working tree whose git directory is `git_dir`.
    pub(crate) fn acquire(git_dir: &Path) -> Result<Self, LockRefusal> {
        let dir = git_dir.join("git-tailor");
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create {}", dir.display()))
            .map_err(LockRefusal::Unavailable)?;
        let path = dir.join("session.lock");
        let file = File::create(&path)
            .with_context(|| format!("failed to open {}", path.display()))
            .map_err(LockRefusal::Unavailable)?;

        match file.try_lock() {
            Ok(()) => Ok(Self { _file: Some(file) }),
            Err(TryLockError::WouldBlock) => Err(LockRefusal::Busy),
            Err(TryLockError::Error(e)) => Err(LockRefusal::Unavailable(
                anyhow::Error::from(e).context("failed to lock the git-tailor session file"),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole point: the second attempt is refused, not queued and not
    /// silently granted.
    #[test]
    fn a_second_acquire_is_refused_while_the_first_is_held() {
        let dir = tempfile::tempdir().unwrap();
        let first = SessionLock::acquire(dir.path()).expect("the first must take it");

        match SessionLock::acquire(dir.path()) {
            Err(LockRefusal::Busy) => {}
            Ok(_) => panic!("the second acquire must be refused while the first is held"),
            Err(LockRefusal::Unavailable(e)) => panic!("expected Busy, got {e:#}"),
        }

        // Released by dropping — and, in the case that matters, by the process
        // dying, which is why a crash cannot strand this lock.
        drop(first);
        SessionLock::acquire(dir.path()).expect("the lock must be free once released");
    }

    /// A directory that cannot hold a lock file must not stop git-tailor: a
    /// read-only `.git`, or a filesystem without locking, degrades to the
    /// behaviour from before the lock existed rather than to no tool at all.
    #[test]
    fn an_unavailable_lock_is_reported_as_such_not_as_busy() {
        let missing = std::path::Path::new("/proc/self/no/such/place");
        match SessionLock::acquire(missing) {
            Err(LockRefusal::Unavailable(_)) => {}
            Err(LockRefusal::Busy) => {
                panic!("a directory we cannot write is not another git-tailor")
            }
            Ok(_) => panic!("expected the attempt to fail"),
        }
    }

    /// Separate working trees keep separate locks, so linked worktrees can run
    /// at the same time.
    #[test]
    fn separate_working_trees_do_not_block_each_other() {
        let one = tempfile::tempdir().unwrap();
        let two = tempfile::tempdir().unwrap();
        let _first = SessionLock::acquire(one.path()).unwrap();
        SessionLock::acquire(two.path()).expect("a different working tree must be unaffected");
    }
}
