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

//! Persistent crash-safety journal.
//!
//! Records an in-progress rebase operation's [`ConflictState`] as a versioned
//! JSON document under the git dir (`<gitdir>/git-tailor/journal.json`) so that
//! an operation interrupted mid-conflict (process killed, terminal closed, hard
//! crash) can be detected and resumed — or aborted — on the next launch.
//!
//! Crash detection keys off the presence of an `in_progress` record, **not** the
//! file's existence: the same document is intended to also hold a persistent
//! undo/redo stack in the future, so a journal left after a *clean* exit must
//! not look like a crash.

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::super::{ConflictState, JournalStatus};
use super::Git2Repo;

/// On-disk journal format version. Bump when the schema changes in a
/// non-additive way and add a matching arm in [`migrate`].
const JOURNAL_VERSION: u32 = 1;

/// Ref pinning the pre-operation branch tip (and therefore every commit the
/// interrupted operation still needs) so `git gc` cannot prune them while the
/// operation is paused.
const ORIG_REF: &str = "refs/git-tailor/orig";

/// The full journal document.
///
/// `in_progress` and any future fields carry `#[serde(default)]` so that adding
/// fields (e.g. a future undo/redo stack) is an additive, auto-upgradeable
/// change that still parses older files.
#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct JournalDoc {
    version: u32,
    /// `Some` while an operation is paused/incomplete; `None` after a clean
    /// completion or abort.
    in_progress: Option<ConflictState>,
}

/// Just the version field, parsed first so a newer-format file can be rejected
/// without depending on the rest of the (possibly unknown) schema.
#[derive(Deserialize)]
struct VersionHeader {
    #[serde(default)]
    version: u32,
}

fn journal_dir(repo: &Git2Repo) -> PathBuf {
    repo.git_dir().join("git-tailor")
}

fn journal_path(repo: &Git2Repo) -> PathBuf {
    journal_dir(repo).join("journal.json")
}

/// Load the current document, returning an empty default when the file is absent.
fn load_doc(repo: &Git2Repo) -> Result<JournalDoc> {
    let path = journal_path(repo);
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .with_context(|| format!("failed to parse journal at {}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(JournalDoc::default()),
        Err(e) => Err(e).with_context(|| format!("failed to read journal at {}", path.display())),
    }
}

/// Atomically write the document (temp file + rename).
fn write_doc(repo: &Git2Repo, doc: &JournalDoc) -> Result<()> {
    let dir = journal_dir(repo);
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create journal dir {}", dir.display()))?;
    let path = journal_path(repo);
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_vec_pretty(doc)?;
    std::fs::write(&tmp, &json)
        .with_context(|| format!("failed to write journal temp {}", tmp.display()))?;
    std::fs::rename(&tmp, &path)
        .with_context(|| format!("failed to finalize journal {}", path.display()))?;
    Ok(())
}

/// Record `state` as the in-progress operation and pin the original branch tip.
pub(super) fn set_in_progress(repo: &Git2Repo, state: &ConflictState) -> Result<()> {
    let mut doc = load_doc(repo).unwrap_or_default();
    doc.version = JOURNAL_VERSION;
    doc.in_progress = Some(state.clone());
    write_doc(repo, &doc)?;

    let orig = git2::Oid::from(&state.original_branch_oid);
    repo.inner
        .reference(ORIG_REF, orig, true, "git-tailor: journal in-progress")?;
    Ok(())
}

/// Clear the in-progress record after a clean completion or abort.
///
/// Removes the file entirely when nothing else remains to persist (a future
/// undo stack would instead keep it while non-empty), and drops the pin ref.
pub(super) fn clear_in_progress(repo: &Git2Repo) -> Result<()> {
    let mut doc = load_doc(repo).unwrap_or_default();
    doc.in_progress = None;

    if is_empty(&doc) {
        let path = journal_path(repo);
        if let Err(e) = std::fs::remove_file(&path)
            && e.kind() != std::io::ErrorKind::NotFound
        {
            return Err(e).with_context(|| format!("failed to remove journal {}", path.display()));
        }
    } else {
        doc.version = JOURNAL_VERSION;
        write_doc(repo, &doc)?;
    }

    delete_orig_ref(repo);
    Ok(())
}

/// Whether the document holds nothing worth persisting.
fn is_empty(doc: &JournalDoc) -> bool {
    doc.in_progress.is_none()
}

fn delete_orig_ref(repo: &Git2Repo) {
    if let Ok(mut r) = repo.inner.find_reference(ORIG_REF) {
        let _ = r.delete();
    }
}

/// Read the journal and classify it for the startup recovery flow.
pub(super) fn read(repo: &Git2Repo) -> JournalStatus {
    let path = journal_path(repo);
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return JournalStatus::None,
        Err(e) => return JournalStatus::Corrupt(format!("failed to read journal: {e}")),
    };

    // Check the version before parsing the payload so a newer schema is rejected
    // cleanly rather than mis-parsed.
    let header: VersionHeader = match serde_json::from_slice(&bytes) {
        Ok(h) => h,
        Err(e) => return JournalStatus::Corrupt(format!("invalid journal JSON: {e}")),
    };
    if header.version > JOURNAL_VERSION {
        return JournalStatus::NewerVersion(header.version);
    }

    let doc: JournalDoc = match serde_json::from_slice(&bytes) {
        Ok(d) => d,
        Err(e) => return JournalStatus::Corrupt(format!("invalid journal JSON: {e}")),
    };

    // Auto-upgrade older versions and persist the upgraded document.
    if header.version < JOURNAL_VERSION {
        match migrate(doc) {
            Ok(upgraded) => {
                let _ = write_doc(repo, &upgraded);
                return classify(upgraded);
            }
            Err(e) => return JournalStatus::Corrupt(format!("journal migration failed: {e}")),
        }
    }

    classify(doc)
}

fn classify(doc: JournalDoc) -> JournalStatus {
    match doc.in_progress {
        Some(state) => JournalStatus::Recovered(Box::new(state)),
        None => JournalStatus::None,
    }
}

/// Upgrade an older-version document to the current schema.
///
/// Only v1 exists today, so this is a pass-through; future version bumps add
/// step-wise migration arms here keyed on `doc.version`.
fn migrate(mut doc: JournalDoc) -> Result<JournalDoc> {
    doc.version = JOURNAL_VERSION;
    Ok(doc)
}
