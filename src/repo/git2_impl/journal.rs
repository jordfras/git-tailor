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

//! Persistent journal under the git dir (`<gitdir>/git-tailor/journal.json`).
//!
//! Holds two kinds of state in one versioned JSON document:
//!
//! - `in_progress`: an operation interrupted mid-conflict (process killed,
//!   terminal closed, hard crash), so the next launch can detect it and resume
//!   or abort. Crash detection keys off the *presence* of this record, **not**
//!   the file's existence — because the file also persists the undo stack across
//!   clean exits, a leftover file must not look like a crash.
//! - `undo` / `redo`: a stack of completed history-rewriting operations so they
//!   can be undone (and redone) even across restarts. Undo needs no per-op
//!   inverse — every gt mutation only moves the branch ref forward and leaves the
//!   old commits in the object database, so undo just restores the recorded
//!   pre-operation tip.
//!
//! Tips referenced by either stack are pinned under `refs/git-tailor/undo/*` so
//! `git gc` cannot prune them; the pins are kept in sync with the stacks and the
//! undo depth is capped so they cannot accumulate without bound.

use std::collections::BTreeSet;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::super::{ConflictState, JournalStatus, UndoOutcome};
use super::Git2Repo;
use super::reads;
use crate::Oid;

/// On-disk journal format version. Bump when the schema changes in a
/// non-additive way and add a matching arm in [`migrate`].
const JOURNAL_VERSION: u32 = 1;

/// Ref pinning the pre-operation branch tip of an *interrupted* operation so
/// `git gc` cannot prune the commits it still needs while it is paused.
const ORIG_REF: &str = "refs/git-tailor/orig";

/// Ref namespace pinning every tip referenced by the undo/redo stacks.
const UNDO_REF_PREFIX: &str = "refs/git-tailor/undo/";

/// Maximum number of undo records kept. Bounds how many old tips stay pinned
/// against `git gc`; pushing past it drops the oldest record (and its pin).
const MAX_UNDO_DEPTH: usize = 50;

/// One reversible operation: restoring `tip_before` undoes it, `tip_after` redoes it.
#[derive(Serialize, Deserialize, Clone)]
struct UndoRecord {
    label: String,
    tip_before: Oid,
    tip_after: Oid,
}

/// State of an auto-stash created for the in-flight operation.
///
/// Kept in the journal so the stash can be reapplied — or, on a conflicting
/// reapply, aborted back to `pre_op_tip` — even across a crash or restart.
#[derive(Serialize, Deserialize, Clone, Default)]
pub(super) struct AutostashRecord {
    /// OID of the stash commit holding the user's dirty changes.
    pub stash: Oid,
    /// Branch tip at the moment the stash was taken (before the operation
    /// advanced the ref). Aborting a conflicting reapply rewinds here; the stash
    /// re-applies cleanly there because that tip is the stash's own base.
    pub pre_op_tip: Oid,
    /// Set once the stash has been reapplied and left conflict markers in the
    /// working tree, so startup recovery does not reapply it a second time.
    pub applied_with_conflict: bool,
}

/// The full journal document.
///
/// All fields carry `#[serde(default)]` so that adding fields is an additive,
/// auto-upgradeable change that still parses older files.
#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct JournalDoc {
    version: u32,
    /// `Some` while an operation is paused/incomplete; `None` after a clean
    /// completion or abort.
    in_progress: Option<ConflictState>,
    /// Completed operations available to undo, oldest first (top = last).
    undo: Vec<UndoRecord>,
    /// Undone operations available to redo, oldest first (top = last).
    redo: Vec<UndoRecord>,
    /// Auto-stash created for the in-flight operation, to be reapplied when it
    /// completes or aborts (survives a crash so recovery can restore the user's
    /// working-tree changes).
    autostash: Option<AutostashRecord>,
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

/// Persist the document — removing the file when nothing is left to store — and
/// reconcile the undo pin refs with the stacks.
fn save(repo: &Git2Repo, doc: &mut JournalDoc) -> Result<()> {
    doc.version = JOURNAL_VERSION;
    if is_empty(doc) {
        let path = journal_path(repo);
        if let Err(e) = std::fs::remove_file(&path)
            && e.kind() != std::io::ErrorKind::NotFound
        {
            return Err(e).with_context(|| format!("failed to remove journal {}", path.display()));
        }
    } else {
        write_doc(repo, doc)?;
    }
    sync_undo_pins(repo, doc);
    Ok(())
}

/// Whether the document holds nothing worth persisting.
fn is_empty(doc: &JournalDoc) -> bool {
    doc.in_progress.is_none()
        && doc.undo.is_empty()
        && doc.redo.is_empty()
        && doc.autostash.is_none()
}

/// Read the recorded auto-stash, if any.
pub(super) fn autostash(repo: &Git2Repo) -> Result<Option<AutostashRecord>> {
    Ok(load_doc(repo)?.autostash)
}

/// Record (or clear, with `None`) the auto-stash for the in-flight operation.
pub(super) fn set_autostash(repo: &Git2Repo, record: Option<AutostashRecord>) -> Result<()> {
    let mut doc = load_doc(repo).unwrap_or_default();
    doc.autostash = record;
    save(repo, &mut doc)
}

/// Restore the undo/redo stacks to the state captured before a forward
/// operation, by dropping its just-recorded undo entry. Called when a
/// conflicting auto-stash reapply is aborted, so the reverted operation does
/// not linger in the undo history. The entry is removed only when it precisely
/// matches a forward operation that advanced `pre_op_tip` to the current
/// `discarded_tip`; undo/redo records (which move between stacks) do not match
/// and are left to the staleness check.
pub(super) fn drop_reverted_undo_record(
    repo: &Git2Repo,
    pre_op_tip: &Oid,
    discarded_tip: &Oid,
) -> Result<()> {
    let mut doc = load_doc(repo).unwrap_or_default();
    if doc
        .undo
        .last()
        .is_some_and(|r| &r.tip_before == pre_op_tip && &r.tip_after == discarded_tip)
    {
        doc.undo.pop();
        save(repo, &mut doc)?;
    }
    Ok(())
}

/// Recreate `refs/git-tailor/undo/*` so exactly the tips referenced by the
/// stacks are pinned against `git gc`. Best-effort: pin failures never abort the
/// caller (pins are only a gc optimisation).
fn sync_undo_pins(repo: &Git2Repo, doc: &JournalDoc) {
    if let Ok(refs) = repo.inner.references_glob(&format!("{UNDO_REF_PREFIX}*")) {
        let names: Vec<String> = refs
            .filter_map(|r| r.ok())
            .filter_map(|r| r.name().ok().map(String::from))
            .collect();
        for name in names {
            if let Ok(mut r) = repo.inner.find_reference(&name) {
                let _ = r.delete();
            }
        }
    }

    let mut oids: BTreeSet<&Oid> = BTreeSet::new();
    for rec in doc.undo.iter().chain(doc.redo.iter()) {
        oids.insert(&rec.tip_before);
        oids.insert(&rec.tip_after);
    }
    for (i, oid) in oids.into_iter().enumerate() {
        let _ = repo.inner.reference(
            &format!("{UNDO_REF_PREFIX}{i}"),
            git2::Oid::from(oid),
            true,
            "git-tailor: undo pin",
        );
    }
}

/// Record `state` as the in-progress operation and pin the original branch tip.
pub(super) fn set_in_progress(repo: &Git2Repo, state: &ConflictState) -> Result<()> {
    let mut doc = load_doc(repo).unwrap_or_default();
    doc.in_progress = Some(state.clone());
    save(repo, &mut doc)?;

    let orig = git2::Oid::from(&state.original_branch_oid);
    repo.inner
        .reference(ORIG_REF, orig, true, "git-tailor: journal in-progress")?;
    Ok(())
}

/// Clear the in-progress record after a clean completion or abort, keeping any
/// undo/redo stack intact, and drop the in-progress pin ref.
pub(super) fn clear_in_progress(repo: &Git2Repo) -> Result<()> {
    let mut doc = load_doc(repo).unwrap_or_default();
    doc.in_progress = None;
    save(repo, &mut doc)?;
    delete_orig_ref(repo);
    Ok(())
}

fn delete_orig_ref(repo: &Git2Repo) {
    if let Ok(mut r) = repo.inner.find_reference(ORIG_REF) {
        let _ = r.delete();
    }
}

/// Push a completed operation onto the undo stack, clearing the redo stack (a
/// new action invalidates redo) and capping the depth.
pub(super) fn record_undo(
    repo: &Git2Repo,
    label: &str,
    tip_before: &Oid,
    tip_after: &Oid,
) -> Result<()> {
    let mut doc = load_doc(repo).unwrap_or_default();
    doc.undo.push(UndoRecord {
        label: label.to_string(),
        tip_before: tip_before.clone(),
        tip_after: tip_after.clone(),
    });
    doc.redo.clear();
    while doc.undo.len() > MAX_UNDO_DEPTH {
        doc.undo.remove(0);
    }
    save(repo, &mut doc)
}

/// Undo the most recent operation: restore its pre-operation tip and move the
/// record to the redo stack.
pub(super) fn apply_undo(repo: &Git2Repo) -> Result<UndoOutcome> {
    let mut doc = load_doc(repo).unwrap_or_default();
    let Some(record) = doc.undo.last().cloned() else {
        return Ok(UndoOutcome::Empty);
    };

    // The branch must still be where this operation left it; otherwise history
    // was changed outside git-tailor and the stack is no longer valid.
    if reads::head_oid(repo)? != record.tip_after {
        doc.undo.clear();
        doc.redo.clear();
        save(repo, &mut doc)?;
        return Ok(UndoOutcome::Stale);
    }

    repo.check_no_dirty_state()?;
    restore_tip(repo, &record.tip_before, "undo", &record.label)?;

    doc.undo.pop();
    doc.redo.push(record.clone());
    save(repo, &mut doc)?;
    Ok(UndoOutcome::Done {
        label: record.label,
    })
}

/// Redo the most recently undone operation: restore its post-operation tip and
/// move the record back to the undo stack.
pub(super) fn apply_redo(repo: &Git2Repo) -> Result<UndoOutcome> {
    let mut doc = load_doc(repo).unwrap_or_default();
    let Some(record) = doc.redo.last().cloned() else {
        return Ok(UndoOutcome::Empty);
    };

    if reads::head_oid(repo)? != record.tip_before {
        doc.undo.clear();
        doc.redo.clear();
        save(repo, &mut doc)?;
        return Ok(UndoOutcome::Stale);
    }

    repo.check_no_dirty_state()?;
    restore_tip(repo, &record.tip_after, "redo", &record.label)?;

    doc.redo.pop();
    doc.undo.push(record.clone());
    save(repo, &mut doc)?;
    Ok(UndoOutcome::Done {
        label: record.label,
    })
}

/// Point the current branch at `target` and check it out.
fn restore_tip(repo: &Git2Repo, target: &Oid, verb: &str, label: &str) -> Result<()> {
    // The working tree currently reflects the tip we're moving away from, so
    // capture it before advancing — checkout_head needs it to remove files the
    // restored tip no longer contains.
    let prev_tip = reads::head_oid(repo)?;
    let oid = git2::Oid::from(target);
    repo.advance_branch_ref(oid, &format!("git-tailor: {verb} {}", label.to_lowercase()))?;
    repo.checkout_head(&prev_tip)
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
