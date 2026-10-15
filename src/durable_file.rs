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

//! Replacing a small file so that the machine losing power cannot lose it.
//!
//! Writing through a temp file and renaming it into place makes the replacement
//! atomic but not durable: the bytes and the directory entry may both still be
//! in the page cache when the power goes, and a file that comes back missing
//! reads as "there was nothing here". So the temp file is synced before the
//! rename, and the directory after it.
//!
//! Every operation is also retried briefly on Windows, where antivirus and the
//! search indexer take short-lived handles that make it fail.

use anyhow::{Context, Result};
use std::ffi::OsString;
use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Replace `path`'s contents with `bytes`, durably.
///
/// The parent directory must exist. A crash at any point leaves either the
/// previous contents or all of the new ones.
pub fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = temp_path(path);
    let file = retry(|| File::create(&tmp))
        .with_context(|| format!("failed to create {}", tmp.display()))?;
    let result = fill_and_rename(file, &tmp, path, bytes);
    if result.is_err() {
        let _ = retry(|| std::fs::remove_file(&tmp));
    }
    result
}

/// Fill `file`, open at `tmp`, with `bytes`, flush it, and rename it over
/// `path`.
fn fill_and_rename(mut file: File, tmp: &Path, path: &Path, bytes: &[u8]) -> Result<()> {
    file.write_all(bytes)
        .with_context(|| format!("failed to write {}", tmp.display()))?;
    sync_file(&file).with_context(|| format!("failed to flush {} to disk", tmp.display()))?;
    drop(file);

    retry(|| std::fs::rename(tmp, path))
        .with_context(|| format!("failed to move {} into place", tmp.display()))?;
    sync_parent_dir(path)
}

/// Remove `path`, durably. Returns whether there was a file to remove.
pub fn remove(path: &Path) -> Result<bool> {
    match retry(|| std::fs::remove_file(path)) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e).with_context(|| format!("failed to remove {}", path.display())),
    }
    sync_parent_dir(path)?;
    Ok(true)
}

/// Read `path`, or `None` when there is no such file.
pub fn read(path: &Path) -> Result<Option<Vec<u8>>> {
    match retry(|| std::fs::read(path)) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("failed to read {}", path.display())),
    }
}

/// The scratch path [`write`] renames into place. Exposed so that a leftover
/// from a process that was killed between the write and the rename can be
/// cleaned up by whoever cleans up after that file.
pub fn temp_path(path: &Path) -> PathBuf {
    let mut name = OsString::from(path.file_name().unwrap_or_default());
    name.push(".tmp");
    path.with_file_name(name)
}

/// Flush the directory entry naming `path`, so that the *name* survives the
/// power cut and not only the bytes behind it.
///
/// [`write`] and [`remove`] do this for the file they touch. A caller that
/// creates a *directory* has to do it for the directory: its entry is no more
/// durable than any other, and what it holds cannot outlive it.
pub fn sync_parent_dir(path: &Path) -> Result<()> {
    let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) else {
        return Ok(());
    };
    sync_dir(dir).with_context(|| format!("failed to flush {} to disk", dir.display()))
}

#[cfg(unix)]
fn sync_dir(dir: &Path) -> io::Result<()> {
    match File::open(dir) {
        Ok(handle) => sync_file(&handle),
        // A mount that will not open a directory, or one no longer there.
        Err(e)
            if matches!(
                e.raw_os_error(),
                Some(libc::EACCES | libc::EPERM | libc::ENOENT)
            ) =>
        {
            Ok(())
        }
        Err(e) => ok_if_unflushable(e),
    }
}

/// On Apple targets `fsync` leaves the data in the drive's own cache, so the
/// file or directory asks for `F_FULLFSYNC` first, and falls back to `fsync`
/// where a mount refuses it.
#[cfg(target_vendor = "apple")]
fn sync_file(file: &File) -> io::Result<()> {
    use std::os::fd::AsRawFd;

    full_flush_or_plain(
        || match unsafe { libc::fcntl(file.as_raw_fd(), libc::F_FULLFSYNC) } {
            -1 => Err(io::Error::last_os_error()),
            _ => Ok(()),
        },
        || fsync(file),
    )
}

#[cfg(all(unix, not(target_vendor = "apple")))]
fn sync_file(file: &File) -> io::Result<()> {
    fsync(file).or_else(ok_if_unflushable)
}

#[cfg(unix)]
fn fsync(file: &File) -> io::Result<()> {
    use std::os::fd::AsRawFd;

    match unsafe { libc::fsync(file.as_raw_fd()) } {
        0 => Ok(()),
        _ => Err(io::Error::last_os_error()),
    }
}

/// `full`, or `plain` when `full` could not have happened here. Never after a
/// real failure: that may have dropped the pages, and a second flush would then
/// report them as written.
#[cfg(all(unix, any(test, target_vendor = "apple")))]
fn full_flush_or_plain(
    full: impl FnOnce() -> io::Result<()>,
    plain: impl FnOnce() -> io::Result<()>,
) -> io::Result<()> {
    match full() {
        Err(e) if cannot_be_flushed(&e) => plain().or_else(ok_if_unflushable),
        settled => settled,
    }
}

#[cfg(not(unix))]
fn sync_file(file: &File) -> io::Result<()> {
    file.sync_all().or_else(ok_if_unflushable)
}

/// A flush that could never have happened here is not a failure.
fn ok_if_unflushable(e: io::Error) -> io::Result<()> {
    if cannot_be_flushed(&e) {
        Ok(())
    } else {
        Err(e)
    }
}

/// Whether a failure means the flush could never have happened here, rather
/// than that it was attempted and failed: a mount that does not support it —
/// some network and FUSE mounts do not. Such a write has landed and only its
/// durability is weaker, so reporting it as failed would call a write that
/// succeeded a failure.
///
/// Any other error is a flush that failed, and written to the page cache is
/// exactly what a crash discards.
#[cfg(unix)]
fn cannot_be_flushed(e: &io::Error) -> bool {
    // A list rather than a pattern: ENOTSUP and EOPNOTSUPP are one value on
    // Linux and two on Apple targets.
    const UNFLUSHABLE: [i32; 4] = [libc::EINVAL, libc::ENOTSUP, libc::EOPNOTSUPP, libc::ENOTTY];
    e.raw_os_error()
        .is_some_and(|code| UNFLUSHABLE.contains(&code))
}

#[cfg(not(any(unix, windows)))]
fn cannot_be_flushed(_e: &io::Error) -> bool {
    false
}

/// A file system that does not support flushing a file.
#[cfg(windows)]
fn cannot_be_flushed(e: &io::Error) -> bool {
    const ERROR_INVALID_FUNCTION: i32 = 1;
    const ERROR_NOT_SUPPORTED: i32 = 50;
    matches!(
        e.raw_os_error(),
        Some(ERROR_INVALID_FUNCTION | ERROR_NOT_SUPPORTED)
    )
}

/// Windows cannot open a directory through `std::fs` at all — `CreateFile`
/// needs `FILE_FLAG_BACKUP_SEMANTICS`, which std does not pass — so the rename
/// goes unflushed there.
#[cfg(not(unix))]
fn sync_dir(_dir: &Path) -> io::Result<()> {
    Ok(())
}

/// Doubling waits between attempts, 255 ms of patience in total: long enough
/// for a scanner to let go of the file, short enough that a real permission
/// error still fails while the user is still looking at the screen.
const RETRY_DELAYS_MS: [u64; 8] = [1, 2, 4, 8, 16, 32, 64, 128];

fn retry<T>(attempt: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    retry_while(attempt, is_held_open_elsewhere)
}

/// The retry loop, with both the attempt and the classification as parameters,
/// so that a test can exercise it on any platform with an error of its own.
fn retry_while<T>(
    mut attempt: impl FnMut() -> io::Result<T>,
    worth_retrying: impl Fn(&io::Error) -> bool,
) -> io::Result<T> {
    for delay in RETRY_DELAYS_MS {
        match attempt() {
            Err(e) if worth_retrying(&e) => std::thread::sleep(Duration::from_millis(delay)),
            settled => return settled,
        }
    }
    attempt()
}

/// Whether `e` is Windows refusing because another process has the file open,
/// as antivirus and the search indexer briefly do.
#[cfg(windows)]
fn is_held_open_elsewhere(e: &io::Error) -> bool {
    const ERROR_ACCESS_DENIED: i32 = 5;
    const ERROR_SHARING_VIOLATION: i32 = 32;
    matches!(
        e.raw_os_error(),
        Some(ERROR_ACCESS_DENIED | ERROR_SHARING_VIOLATION)
    )
}

/// Unix replaces or removes a file whoever has it open.
#[cfg(not(windows))]
fn is_held_open_elsewhere(_e: &io::Error) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn transient() -> io::Error {
        io::Error::from(io::ErrorKind::PermissionDenied)
    }

    fn permanent() -> io::Error {
        io::Error::from(io::ErrorKind::NotFound)
    }

    fn is_permission_denied(e: &io::Error) -> bool {
        e.kind() == io::ErrorKind::PermissionDenied
    }

    #[test]
    fn a_successful_attempt_is_made_once() {
        let attempts = Cell::new(0);

        let result = retry_while(
            || {
                attempts.set(attempts.get() + 1);
                Ok(())
            },
            is_permission_denied,
        );

        assert!(result.is_ok());
        assert_eq!(attempts.get(), 1);
    }

    /// The antivirus case, which no test can produce for real: the handle is
    /// released while git-tailor waits, and the write goes through without the
    /// caller ever hearing about it.
    #[test]
    fn a_transient_failure_is_waited_out() {
        let attempts = Cell::new(0);

        let result = retry_while(
            || {
                attempts.set(attempts.get() + 1);
                if attempts.get() < 3 {
                    return Err(transient());
                }
                Ok(())
            },
            is_permission_denied,
        );

        assert!(result.is_ok());
        assert_eq!(attempts.get(), 3);
    }

    /// A path git-tailor may not write is not going to become writable in a
    /// quarter of a second, and waiting to say so freezes the display.
    #[test]
    fn a_permanent_failure_is_reported_at_once() {
        let attempts = Cell::new(0);

        let result = retry_while(
            || {
                attempts.set(attempts.get() + 1);
                Err::<(), _>(permanent())
            },
            is_permission_denied,
        );

        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::NotFound);
        assert_eq!(attempts.get(), 1);
    }

    #[test]
    fn a_failure_that_never_clears_gives_up_and_reports_it() {
        let attempts = Cell::new(0);

        let result = retry_while(
            || {
                attempts.set(attempts.get() + 1);
                Err::<(), _>(transient())
            },
            is_permission_denied,
        );

        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(attempts.get(), RETRY_DELAYS_MS.len() + 1);
    }

    #[cfg(windows)]
    #[test]
    fn a_file_system_that_cannot_flush_is_not_a_failed_write() {
        assert!(ok_if_unflushable(io::Error::from_raw_os_error(1)).is_ok());
        assert!(ok_if_unflushable(io::Error::from_raw_os_error(50)).is_ok());
        // ERROR_ACCESS_DENIED
        assert!(ok_if_unflushable(io::Error::from_raw_os_error(5)).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn only_a_file_held_open_elsewhere_is_retried() {
        assert!(is_held_open_elsewhere(&io::Error::from_raw_os_error(5)));
        assert!(is_held_open_elsewhere(&io::Error::from_raw_os_error(32)));
        // ERROR_FILE_NOT_FOUND
        assert!(!is_held_open_elsewhere(&io::Error::from_raw_os_error(2)));
    }

    /// Beside the file, not inside a directory of its own: the rename has to
    /// stay on one filesystem to be atomic, and cleanup has to be able to name
    /// the leftover.
    #[test]
    fn the_temp_file_sits_beside_its_destination() {
        assert_eq!(
            temp_path(Path::new("/repo/.git/git-tailor/journal.json")),
            Path::new("/repo/.git/git-tailor/journal.json.tmp")
        );
    }

    #[test]
    fn a_write_replaces_the_previous_contents_and_leaves_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal.json");

        write(&path, b"first").unwrap();
        write(&path, b"second").unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), b"second");
        assert!(!temp_path(&path).exists());
    }

    /// The second removal is not an error, and says it found nothing — which is
    /// how a caller reports what it cleaned up.
    #[test]
    fn a_removal_says_whether_there_was_a_file_to_remove() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal.json");

        write(&path, b"something").unwrap();

        assert!(remove(&path).unwrap());
        assert!(!remove(&path).unwrap());
        assert!(!path.exists());
    }

    #[test]
    fn a_missing_file_reads_as_absent_rather_than_as_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal.json");

        assert_eq!(read(&path).unwrap(), None);

        write(&path, b"something").unwrap();

        assert_eq!(read(&path).unwrap().as_deref(), Some(&b"something"[..]));
    }

    /// The distinction finding its way into a caller: `None` means nothing was
    /// recorded, and a file that is there but cannot be read must not say that.
    #[cfg(unix)]
    #[test]
    fn an_unreadable_file_is_an_error_rather_than_absent() {
        use std::fs::Permissions;
        use std::os::unix::fs::PermissionsExt;

        // Root reads it whatever the mode says, so there would be nothing to see.
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal.json");
        write(&path, b"something").unwrap();
        std::fs::set_permissions(&path, Permissions::from_mode(0o000)).unwrap();

        let e = read(&path).unwrap_err();

        assert!(
            format!("{e:#}").contains("journal.json"),
            "the error should name the file: {e:#}"
        );
    }

    #[cfg(unix)]
    fn unsupported() -> io::Result<()> {
        Err(io::Error::from_raw_os_error(libc::ENOTSUP))
    }

    #[cfg(unix)]
    #[test]
    fn a_full_flush_the_mount_refuses_falls_back_to_a_plain_one() {
        let plain = Cell::new(false);

        let result = full_flush_or_plain(unsupported, || {
            plain.set(true);
            Ok(())
        });

        assert!(result.is_ok());
        assert!(plain.get());
    }

    /// The pages a failed flush dropped would be reported as written by the
    /// next one.
    #[cfg(unix)]
    #[test]
    fn a_full_flush_that_failed_is_not_retried_as_a_plain_one() {
        let plain = Cell::new(false);

        let result = full_flush_or_plain(
            || Err(io::Error::from_raw_os_error(libc::EIO)),
            || {
                plain.set(true);
                Ok(())
            },
        );

        assert_eq!(result.unwrap_err().raw_os_error(), Some(libc::EIO));
        assert!(!plain.get());
    }

    #[cfg(unix)]
    #[test]
    fn a_mount_that_flushes_nothing_has_still_written() {
        assert!(full_flush_or_plain(unsupported, unsupported).is_ok());
    }

    #[test]
    fn a_failed_write_leaves_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal.json");
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("occupied"), b"").unwrap();

        write(&path, b"something").unwrap_err();

        assert!(!temp_path(&path).exists());
    }

    /// The write has landed by then; only its durability is weaker.
    #[cfg(unix)]
    #[test]
    fn a_mount_that_cannot_flush_is_not_a_failed_write() {
        assert!(ok_if_unflushable(io::Error::from_raw_os_error(libc::ENOTSUP)).is_ok());
        assert!(ok_if_unflushable(io::Error::from_raw_os_error(libc::EINVAL)).is_ok());
        assert!(ok_if_unflushable(io::Error::from_raw_os_error(libc::EIO)).is_err());
        assert!(ok_if_unflushable(io::Error::from_raw_os_error(libc::EPERM)).is_err());
    }

    #[test]
    fn a_write_into_a_missing_directory_names_the_file_it_could_not_create() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("not-created-yet/journal.json");

        let e = write(&path, b"something").unwrap_err();

        assert!(
            format!("{e:#}").contains("journal.json.tmp"),
            "the error should name the temp file: {e:#}"
        );
    }
}
