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
//! - `undo` / `redo`: a stack of completed operations so they can be undone (and
//!   redone) even across restarts. Undo needs no per-op inverse: a
//!   history-rewriting op only moves the branch ref forward and leaves the old
//!   commits in the object database, so undo just restores the recorded
//!   pre-operation tip; an index-only op (stage/unstage all) leaves the ref alone
//!   and records the index *tree* OIDs before/after, so undo just restores the
//!   recorded tree into the index. Either way the recorded OIDs are a complete,
//!   diff-free snapshot of the pre-operation state.
//!
//! Tips and index trees referenced by either stack are pinned under
//! `refs/git-tailor/undo/*` so `git gc` cannot prune them; the pins are kept in
//! sync with the stacks and the undo depth is capped so they cannot accumulate
//! without bound.

use std::collections::BTreeSet;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::super::{InProgress, JournalCleanSummary, JournalStatus, LiftedRow, UndoOutcome};
use super::Git2Repo;
use super::reads;
use crate::Oid;

/// On-disk journal format version.
///
/// Bump when a document this version writes would no longer *parse* in the
/// version before it, and add a matching upgrade branch in [`read`]. A new
/// optional field on [`JournalDoc`] is safe, since an older reader skips what it
/// does not know; a new [`UndoRecord`] variant is not, because the enum is
/// tagged and an unknown tag fails the whole document — the reader then reports
/// a perfectly good journal as corrupt instead of as one from the future.
const JOURNAL_VERSION: u32 = 2;

/// Common namespace for every ref git-tailor writes. Single source of truth:
/// the leaf names below build on it, and `--clean-journal` finds every ref by
/// this prefix regardless of the journal contents. (Stable Rust can't
/// concatenate `&str` consts, so the full names are composed at use sites.)
const REF_NAMESPACE: &str = "refs/git-tailor/";

/// Leaf (under [`REF_NAMESPACE`]) of the ref pinning the pre-operation branch
/// tip of an *interrupted* operation so `git gc` cannot prune the commits it
/// still needs while it is paused.
const ORIG_REF_LEAF: &str = "orig";

/// Leaf (under [`REF_NAMESPACE`]) of the subdirectory pinning every tip
/// referenced by the undo/redo stacks.
const UNDO_REF_LEAF: &str = "undo/";

/// Leaf (under [`REF_NAMESPACE`]) of the subdirectory keeping the working tree
/// a discarded lift record still named, so the uncommitted work in it stays
/// reachable rather than becoming garbage the next `git gc` collects.
const RESCUE_REF_LEAF: &str = "rescue/";

/// Full name of the rescue ref holding `tree`. Named after the tree so rescuing
/// the same content twice is one ref rather than two.
pub(super) fn rescue_ref(tree: &Oid) -> String {
    format!("{REF_NAMESPACE}{RESCUE_REF_LEAF}{}", tree.short())
}

/// Full name of the in-progress `orig` pin.
fn orig_ref() -> String {
    format!("{REF_NAMESPACE}{ORIG_REF_LEAF}")
}

/// Maximum number of undo records kept. Bounds how many old tips stay pinned
/// against `git gc`; pushing past it drops the oldest record (and its pin).
const MAX_UNDO_DEPTH: usize = 50;

/// One reversible operation.
///
/// Tagged on `kind` so the flavours stay self-describing on disk.
#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "kind")]
enum UndoRecord {
    /// A history-rewriting op: restoring `tip_before` undoes it, `tip_after`
    /// redoes it. Undo/redo check the restored tip out (hard ref move).
    RefMove {
        label: String,
        tip_before: Oid,
        tip_after: Oid,
    },
    /// An index-only op (stage/unstage all): the ref stays at `head` while the
    /// index tree moves from `index_tree_before` to `index_tree_after`. Undo/redo
    /// restore the respective tree into the index without touching the ref or the
    /// working tree.
    IndexChange {
        label: String,
        head: Oid,
        index_tree_before: Oid,
        index_tree_after: Oid,
    },
    /// A new commit (commit-staged). Undo/redo is a *soft* ref move
    /// (`reset --soft`): the ref moves between `tip_before` (parent) and
    /// `tip_after` (the commit) while the index and working tree are left
    /// untouched, so the committed changes reappear as staged on undo.
    Commit {
        label: String,
        tip_before: Oid,
        tip_after: Oid,
    },
    /// A squash whose source was a working-tree row. Undo/redo are *mixed*
    /// resets (`reset --mixed`): the ref moves and the index tree is restored,
    /// but the working tree is left alone — a fold that ran clean only moved
    /// content between committed, staged and unstaged, so the files on disk are
    /// the same before and after.
    ///
    /// A fold the user resolved a conflict in is the exception: the resolution
    /// is what ends up on disk, and undo does not take it back off. The undo
    /// restores what was committed and what was staged, and the resolution stays
    /// as the unstaged difference from it.
    MixedReset {
        label: String,
        tip_before: Oid,
        tip_after: Oid,
        index_tree_before: Oid,
        index_tree_after: Oid,
    },
}

impl UndoRecord {
    fn label(&self) -> &str {
        match self {
            UndoRecord::RefMove { label, .. }
            | UndoRecord::IndexChange { label, .. }
            | UndoRecord::Commit { label, .. }
            | UndoRecord::MixedReset { label, .. } => label,
        }
    }

    /// Whether undo/redo leave the working tree untouched (so the caller can skip
    /// the auto-stash dance that protects the working tree during `RefMove`
    /// checkouts). True for index-only ops and for commits (a soft ref move).
    fn skips_autostash(&self) -> bool {
        matches!(
            self,
            UndoRecord::IndexChange { .. }
                | UndoRecord::Commit { .. }
                | UndoRecord::MixedReset { .. }
        )
    }

    /// Where HEAD should sit while this op is *applied* (top of the undo stack):
    /// after the ref move, or unchanged for an index-only op.
    fn applied_head(&self) -> &Oid {
        match self {
            UndoRecord::RefMove { tip_after, .. }
            | UndoRecord::Commit { tip_after, .. }
            | UndoRecord::MixedReset { tip_after, .. } => tip_after,
            UndoRecord::IndexChange { head, .. } => head,
        }
    }

    /// Where HEAD should sit while this op is *undone* (top of the redo stack):
    /// back at the pre-op tip, or unchanged for an index-only op.
    fn unapplied_head(&self) -> &Oid {
        match self {
            UndoRecord::RefMove { tip_before, .. }
            | UndoRecord::Commit { tip_before, .. }
            | UndoRecord::MixedReset { tip_before, .. } => tip_before,
            UndoRecord::IndexChange { head, .. } => head,
        }
    }

    /// OIDs that must stay reachable (gc-pinned) while this record lives.
    fn pinned_oids(&self) -> Vec<&Oid> {
        match self {
            UndoRecord::RefMove {
                tip_before,
                tip_after,
                ..
            }
            | UndoRecord::Commit {
                tip_before,
                tip_after,
                ..
            } => vec![tip_before, tip_after],
            UndoRecord::IndexChange {
                index_tree_before,
                index_tree_after,
                ..
            } => vec![index_tree_before, index_tree_after],
            UndoRecord::MixedReset {
                tip_before,
                tip_after,
                index_tree_before,
                index_tree_after,
                ..
            } => vec![tip_before, tip_after, index_tree_before, index_tree_after],
        }
    }
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
    /// `Some` while an operation is paused/incomplete (a conflict awaiting
    /// resolution, or an Edit shell in progress); `None` after a clean
    /// completion or abort.
    in_progress: Option<InProgress>,
    /// Completed operations available to undo, oldest first (top = last).
    undo: Vec<UndoRecord>,
    /// Undone operations available to redo, oldest first (top = last).
    redo: Vec<UndoRecord>,
    /// Auto-stash created for the in-flight operation, to be reapplied when it
    /// completes or aborts (survives a crash so recovery can restore the user's
    /// working-tree changes).
    autostash: Option<AutostashRecord>,
    /// Pre-operation state of a squash whose source is a working-tree row. Like
    /// the auto-stash it is a sibling of `in_progress` rather than part of it,
    /// because it has to outlive the phase changes — `in_progress` becomes a
    /// conflict record the moment the squash conflicts, and the snapshot is
    /// still what abort and completion restore from.
    ///
    /// The key keeps the name the type had when v2 was written: a journal from an
    /// earlier build of this format must still read back, and `serde(default)`
    /// would turn a renamed key into a silently stranded temporary commit.
    worktree_source: Option<LiftedRow>,
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
        && doc.worktree_source.is_none()
}

/// Read the recorded working-tree-source snapshot, if any.
pub(super) fn worktree_source(repo: &Git2Repo) -> Result<Option<LiftedRow>> {
    Ok(load_doc(repo)?.worktree_source)
}

/// Record (or clear, with `None`) the working-tree-source snapshot for the
/// in-flight operation.
pub(super) fn set_worktree_source(repo: &Git2Repo, snapshot: Option<LiftedRow>) -> Result<()> {
    let mut doc = load_doc(repo).unwrap_or_default();
    doc.worktree_source = snapshot;
    save(repo, &mut doc)
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
    if doc.undo.last().is_some_and(|r| {
        matches!(
            r,
            UndoRecord::RefMove { tip_before, tip_after, .. }
                if tip_before == pre_op_tip && tip_after == discarded_tip
        )
    }) {
        doc.undo.pop();
        save(repo, &mut doc)?;
    }
    Ok(())
}

/// Drop every `refs/git-tailor/*` ref no longer justified by the journal, so
/// stale entries don't linger in tools like `gitk`. Run at startup.
///
/// - **Undo/redo history**: a *valid* stack — HEAD still at the position
///   git-tailor left it (`undo.last().tip_after`, or `redo.last().tip_before`
///   once everything is undone) — is preserved so undo/redo survives across
///   restarts; a stale one (HEAD moved outside git-tailor) is cleared. The
///   staleness check is skipped while an operation is paused mid-conflict
///   (`in_progress`), since HEAD then sits on a partial tip.
/// - **gc-pins**: the `undo/*` pins are always reconciled with the surviving
///   stacks — even while paused, since they are independent of the in-progress
///   operation — dropping any orphans. The `orig` pin belongs to a paused
///   operation, so it is dropped whenever there is none.
pub(super) fn prune_stale(repo: &Git2Repo) -> Result<()> {
    let mut doc = load_doc(repo).unwrap_or_default();

    // A fold in flight is an operation in progress too, even before it reaches a
    // conflict: its temporary commit moves the branch off the tip the undo stack
    // records, which is not history changing behind git-tailor's back.
    let mut cleared = false;
    if doc.in_progress.is_none() && doc.worktree_source.is_none() {
        if stacks_stale(repo, &doc)? {
            doc.undo.clear();
            doc.redo.clear();
            cleared = true;
        }
        // An `orig` pin without a paused operation is orphaned (e.g. left by a
        // crash between clearing the in-progress record and deleting the ref).
        delete_orig_ref(repo);
    }

    if cleared {
        // `save` removes the now-empty journal and reconciles the undo pins.
        save(repo, &mut doc)
    } else {
        sync_undo_pins(repo, &doc);
        Ok(())
    }
}

/// Whether the undo/redo stacks no longer match HEAD (history was changed
/// outside git-tailor). An empty history is never stale.
fn stacks_stale(repo: &Git2Repo, doc: &JournalDoc) -> Result<bool> {
    if doc.undo.is_empty() && doc.redo.is_empty() {
        return Ok(false);
    }
    let current = doc
        .undo
        .last()
        .map(UndoRecord::applied_head)
        .or_else(|| doc.redo.last().map(UndoRecord::unapplied_head));
    Ok(current != Some(&reads::head_oid(repo)?))
}

/// Recreate `refs/git-tailor/undo/*` so exactly the tips referenced by the
/// stacks are pinned against `git gc`. Best-effort: pin failures never abort the
/// caller (pins are only a gc optimisation).
fn sync_undo_pins(repo: &Git2Repo, doc: &JournalDoc) {
    if let Ok(refs) = repo
        .inner
        .references_glob(&format!("{REF_NAMESPACE}{UNDO_REF_LEAF}*"))
    {
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
        oids.extend(rec.pinned_oids());
    }
    // The snapshot's trees are loose objects no ref reaches.
    if let Some(snapshot) = &doc.worktree_source {
        oids.extend([
            &snapshot.tip_before,
            &snapshot.index_tree_before,
            &snapshot.worktree_tree,
            &snapshot.source_tree,
            &snapshot.temp_oid,
        ]);
    }
    for (i, oid) in oids.into_iter().enumerate() {
        let _ = repo.inner.reference(
            &format!("{REF_NAMESPACE}{UNDO_REF_LEAF}{i}"),
            git2::Oid::from(oid),
            true,
            "git-tailor: undo pin",
        );
    }
}

/// Record `record` as the in-progress operation and pin the original branch tip.
pub(super) fn set_in_progress(repo: &Git2Repo, record: &InProgress) -> Result<()> {
    let mut doc = load_doc(repo).unwrap_or_default();
    doc.in_progress = Some(record.clone());
    save(repo, &mut doc)?;

    let orig = git2::Oid::from(record.original_branch_oid());
    repo.inner
        .reference(&orig_ref(), orig, true, "git-tailor: journal in-progress")?;
    Ok(())
}

/// Read the in-progress operation record, if any.
pub(super) fn in_progress(repo: &Git2Repo) -> Result<Option<InProgress>> {
    Ok(load_doc(repo)?.in_progress)
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

/// Abandon whatever operation was in flight: the paused record *and* the
/// working-tree snapshot. Used when a journal is discarded as unusable, where
/// leaving the snapshot behind would keep telling later operations that a dirty
/// working tree is accounted for. Distinct from
/// [`clear_in_progress`], which only ends one phase of an operation that is
/// still running.
pub(super) fn discard_in_flight(repo: &Git2Repo) -> Result<()> {
    let mut doc = load_doc(repo).unwrap_or_default();
    doc.in_progress = None;
    doc.worktree_source = None;
    save(repo, &mut doc)?;
    delete_orig_ref(repo);
    Ok(())
}

fn delete_orig_ref(repo: &Git2Repo) {
    if let Ok(mut r) = repo.inner.find_reference(&orig_ref()) {
        let _ = r.delete();
    }
}

/// Remove all git-tailor recovery state: every ref under `refs/git-tailor/`
/// (undo pins and the in-progress `orig` pin) and the on-disk journal file.
///
/// Refs are discovered by namespace rather than from the journal, so stray refs
/// are removed even when the journal is missing, corrupt, or out of sync — this
/// is the manual escape hatch behind `--clean-journal`.
pub(super) fn clean(repo: &Git2Repo) -> Result<JournalCleanSummary> {
    let mut refs = repo
        .inner
        .references()
        .context("failed to enumerate references")?;
    let names: Vec<String> = refs
        .names()
        .filter_map(|n| n.ok())
        .filter(|name| name.starts_with(REF_NAMESPACE))
        .map(|name| name.to_string())
        .collect();
    let mut refs_removed = 0;
    for name in names {
        if let Ok(mut r) = repo.inner.find_reference(&name)
            && r.delete().is_ok()
        {
            refs_removed += 1;
        }
    }

    let path = journal_path(repo);
    let journal_removed = match std::fs::remove_file(&path) {
        Ok(()) => true,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => {
            return Err(e).with_context(|| format!("failed to remove journal {}", path.display()));
        }
    };
    // A temp file may linger if a write was interrupted; drop it too, then
    // remove the now-hopefully-empty directory (best-effort).
    let _ = std::fs::remove_file(path.with_extension("json.tmp"));
    let _ = std::fs::remove_dir(journal_dir(repo));

    Ok(JournalCleanSummary {
        refs_removed,
        journal_removed,
    })
}

/// Push a completed history-rewriting operation onto the undo stack.
pub(super) fn record_undo(
    repo: &Git2Repo,
    label: &str,
    tip_before: &Oid,
    tip_after: &Oid,
) -> Result<()> {
    push_undo(
        repo,
        UndoRecord::RefMove {
            label: label.to_string(),
            tip_before: tip_before.clone(),
            tip_after: tip_after.clone(),
        },
    )
}

/// Push a completed index-only operation (stage/unstage all) onto the undo stack.
pub(super) fn record_index_undo(
    repo: &Git2Repo,
    label: &str,
    head: &Oid,
    index_tree_before: &Oid,
    index_tree_after: &Oid,
) -> Result<()> {
    push_undo(
        repo,
        UndoRecord::IndexChange {
            label: label.to_string(),
            head: head.clone(),
            index_tree_before: index_tree_before.clone(),
            index_tree_after: index_tree_after.clone(),
        },
    )
}

/// Push a completed commit-staged operation onto the undo stack.
pub(super) fn record_commit_undo(
    repo: &Git2Repo,
    label: &str,
    tip_before: &Oid,
    tip_after: &Oid,
) -> Result<()> {
    push_undo(
        repo,
        UndoRecord::Commit {
            label: label.to_string(),
            tip_before: tip_before.clone(),
            tip_after: tip_after.clone(),
        },
    )
}

/// What a completed working-tree-sourced squash moved, for
/// [`record_mixed_undo`].
///
/// Named fields because all four are OIDs, and swapping a before for an after
/// would still typecheck while recording an undo entry that runs backwards.
pub(super) struct MixedUndo<'a> {
    pub tip_before: &'a Oid,
    pub tip_after: &'a Oid,
    pub index_tree_before: &'a Oid,
    pub index_tree_after: &'a Oid,
}

/// Push a completed working-tree-sourced squash onto the undo stack.
pub(super) fn record_mixed_undo(repo: &Git2Repo, label: &str, moved: MixedUndo<'_>) -> Result<()> {
    push_undo(
        repo,
        UndoRecord::MixedReset {
            label: label.to_string(),
            tip_before: moved.tip_before.clone(),
            tip_after: moved.tip_after.clone(),
            index_tree_before: moved.index_tree_before.clone(),
            index_tree_after: moved.index_tree_after.clone(),
        },
    )
}

/// Append a record to the undo stack, clearing the redo stack (a new action
/// invalidates redo) and capping the depth.
fn push_undo(repo: &Git2Repo, record: UndoRecord) -> Result<()> {
    let mut doc = load_doc(repo).unwrap_or_default();
    doc.undo.push(record);
    doc.redo.clear();
    while doc.undo.len() > MAX_UNDO_DEPTH {
        doc.undo.remove(0);
    }
    save(repo, &mut doc)
}

/// Whether the next undo leaves the working tree untouched, so the caller can
/// skip the auto-stash dance that would otherwise stash away (and reapply) the
/// very state it restores.
pub(super) fn pending_undo_skips_autostash(repo: &Git2Repo) -> Result<bool> {
    Ok(load_doc(repo)?
        .undo
        .last()
        .is_some_and(UndoRecord::skips_autostash))
}

/// Whether the next redo leaves the working tree untouched. See
/// [`pending_undo_skips_autostash`].
pub(super) fn pending_redo_skips_autostash(repo: &Git2Repo) -> Result<bool> {
    Ok(load_doc(repo)?
        .redo
        .last()
        .is_some_and(UndoRecord::skips_autostash))
}

/// Undo the most recent operation, moving its record to the redo stack. A
/// history-rewriting op restores its pre-operation tip; an index-only op restores
/// its pre-operation index tree; a commit soft-resets to its parent.
pub(super) fn apply_undo(repo: &Git2Repo) -> Result<UndoOutcome> {
    let mut doc = load_doc(repo).unwrap_or_default();
    let Some(record) = doc.undo.last().cloned() else {
        return Ok(UndoOutcome::Empty);
    };

    match &record {
        UndoRecord::RefMove {
            tip_before,
            tip_after,
            label,
        } => {
            if !revert_ref(repo, &mut doc, tip_after, tip_before, "undo", label)? {
                return Ok(UndoOutcome::Stale);
            }
        }
        UndoRecord::IndexChange {
            head,
            index_tree_before,
            index_tree_after,
            ..
        } => {
            if !revert_index(repo, &mut doc, head, index_tree_after, index_tree_before)? {
                return Ok(UndoOutcome::Stale);
            }
        }
        UndoRecord::Commit {
            tip_before,
            tip_after,
            label,
        } => {
            if !revert_soft(repo, &mut doc, tip_after, tip_before, "undo", label)? {
                return Ok(UndoOutcome::Stale);
            }
        }
        UndoRecord::MixedReset {
            tip_before,
            tip_after,
            index_tree_before,
            index_tree_after,
            label,
        } => {
            if !revert_mixed(
                repo,
                &mut doc,
                MixedRevert {
                    expected_tip: tip_after,
                    expected_index: index_tree_after,
                    target_tip: tip_before,
                    target_index: index_tree_before,
                },
                "undo",
                label,
            )? {
                return Ok(UndoOutcome::Stale);
            }
        }
    }

    doc.undo.pop();
    doc.redo.push(record.clone());
    save(repo, &mut doc)?;
    Ok(UndoOutcome::Done {
        label: record.label().to_string(),
    })
}

/// Redo the most recently undone operation, moving its record back to the undo
/// stack: restore its post-operation tip, or its post-operation index tree.
pub(super) fn apply_redo(repo: &Git2Repo) -> Result<UndoOutcome> {
    let mut doc = load_doc(repo).unwrap_or_default();
    let Some(record) = doc.redo.last().cloned() else {
        return Ok(UndoOutcome::Empty);
    };

    match &record {
        UndoRecord::RefMove {
            tip_before,
            tip_after,
            label,
        } => {
            if !revert_ref(repo, &mut doc, tip_before, tip_after, "redo", label)? {
                return Ok(UndoOutcome::Stale);
            }
        }
        UndoRecord::IndexChange {
            head,
            index_tree_before,
            index_tree_after,
            ..
        } => {
            if !revert_index(repo, &mut doc, head, index_tree_before, index_tree_after)? {
                return Ok(UndoOutcome::Stale);
            }
        }
        UndoRecord::Commit {
            tip_before,
            tip_after,
            label,
        } => {
            if !revert_soft(repo, &mut doc, tip_before, tip_after, "redo", label)? {
                return Ok(UndoOutcome::Stale);
            }
        }
        UndoRecord::MixedReset {
            tip_before,
            tip_after,
            index_tree_before,
            index_tree_after,
            label,
        } => {
            if !revert_mixed(
                repo,
                &mut doc,
                MixedRevert {
                    expected_tip: tip_before,
                    expected_index: index_tree_before,
                    target_tip: tip_after,
                    target_index: index_tree_after,
                },
                "redo",
                label,
            )? {
                return Ok(UndoOutcome::Stale);
            }
        }
    }

    doc.redo.pop();
    doc.undo.push(record.clone());
    save(repo, &mut doc)?;
    Ok(UndoOutcome::Done {
        label: record.label().to_string(),
    })
}

/// Move the branch ref from `expected` to `target` and check it out. Returns
/// `false` (after clearing the now-stale stacks) when HEAD has drifted from
/// `expected` — history was changed outside git-tailor.
fn revert_ref(
    repo: &Git2Repo,
    doc: &mut JournalDoc,
    expected: &Oid,
    target: &Oid,
    verb: &str,
    label: &str,
) -> Result<bool> {
    if &reads::head_oid(repo)? != expected {
        clear_stacks(repo, doc)?;
        return Ok(false);
    }
    repo.check_no_dirty_state()?;
    restore_tip(repo, target, verb, label)?;
    Ok(true)
}

/// Restore `target` index tree, expecting the index to currently hold
/// `expected`. Returns `false` (after clearing the now-stale stacks) when the
/// ref has drifted from `head` or the index no longer matches `expected` — the
/// user staged something else outside this undo history.
fn revert_index(
    repo: &Git2Repo,
    doc: &mut JournalDoc,
    head: &Oid,
    expected: &Oid,
    target: &Oid,
) -> Result<bool> {
    if &reads::head_oid(repo)? != head || &current_index_tree(repo)? != expected {
        clear_stacks(repo, doc)?;
        return Ok(false);
    }
    restore_index(repo, target)
}

/// Soft-move the branch ref from `expected` to `target` (`reset --soft`),
/// leaving the index and working tree untouched, so a committed change reappears
/// as staged on undo. Returns `false` (after clearing the now-stale stacks) when
/// HEAD has drifted from `expected`.
fn revert_soft(
    repo: &Git2Repo,
    doc: &mut JournalDoc,
    expected: &Oid,
    target: &Oid,
    verb: &str,
    label: &str,
) -> Result<bool> {
    if &reads::head_oid(repo)? != expected {
        clear_stacks(repo, doc)?;
        return Ok(false);
    }
    repo.advance_branch_ref(
        git2::Oid::from(target),
        &format!("git-tailor: {verb} {}", label.to_lowercase()),
    )?;
    Ok(true)
}

/// Where a mixed reset is coming from and going to.
struct MixedRevert<'a> {
    expected_tip: &'a Oid,
    expected_index: &'a Oid,
    target_tip: &'a Oid,
    target_index: &'a Oid,
}

/// Move the branch ref *and* the index (`reset --mixed`), leaving the working
/// tree alone. Returns `false` (after clearing the now-stale stacks) when either
/// the ref or the index has drifted — as for an index-only record, a mismatch
/// means the user changed things outside this history, and forcing the recorded
/// state over it would discard their work.
fn revert_mixed(
    repo: &Git2Repo,
    doc: &mut JournalDoc,
    revert: MixedRevert<'_>,
    verb: &str,
    label: &str,
) -> Result<bool> {
    if &reads::head_oid(repo)? != revert.expected_tip
        || &current_index_tree(repo)? != revert.expected_index
    {
        clear_stacks(repo, doc)?;
        return Ok(false);
    }
    repo.advance_branch_ref(
        git2::Oid::from(revert.target_tip),
        &format!("git-tailor: {verb} {}", label.to_lowercase()),
    )?;
    restore_index(repo, revert.target_index)?;
    Ok(true)
}

fn clear_stacks(repo: &Git2Repo, doc: &mut JournalDoc) -> Result<()> {
    doc.undo.clear();
    doc.redo.clear();
    save(repo, doc)
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

/// Tree OID of the on-disk index, without mutating it. Used to snapshot the
/// staged state and to validate it before an index undo/redo.
pub(super) fn current_index_tree(repo: &Git2Repo) -> Result<Oid> {
    let mut index = repo.inner.index().context("failed to open index")?;
    index.read(true).context("failed to refresh index")?;
    Ok(index
        .write_tree()
        .context("failed to snapshot index tree")?
        .into())
}

/// Reset the index to `target` tree, leaving the branch ref and working tree
/// untouched. Always returns `Ok(true)` so callers can treat it as the
/// non-stale arm of `revert_index`.
fn restore_index(repo: &Git2Repo, target: &Oid) -> Result<bool> {
    repo.set_index_tree(git2::Oid::from(target))?;
    Ok(true)
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

    // Older versions: read the version-specific shape and upgrade. A v1
    // in-progress record (the old flat ConflictState) isn't representable in the
    // current schema. If one is present we must NOT rewrite the file — leaving it
    // as-is lets an older git-tailor still finish the operation — and we surface
    // it as UpgradeInterrupted. Otherwise the upgrade is lossless: undo/redo/
    // autostash carry over and the file is rewritten to the current version.
    // See CHANGELOG.
    if header.version < JOURNAL_VERSION {
        let old: JournalDocV1 = match serde_json::from_slice(&bytes) {
            Ok(d) => d,
            Err(e) => return JournalStatus::Corrupt(format!("invalid journal JSON: {e}")),
        };
        if let Some(op) = old.interrupted_op_label() {
            return JournalStatus::UpgradeInterrupted { op };
        }
        let upgraded = migrate_v1(old);
        let _ = write_doc(repo, &upgraded);
        return classify(upgraded);
    }

    let doc: JournalDoc = match serde_json::from_slice(&bytes) {
        Ok(d) => d,
        Err(e) => return JournalStatus::Corrupt(format!("invalid journal JSON: {e}")),
    };
    classify(doc)
}

fn classify(doc: JournalDoc) -> JournalStatus {
    match doc.in_progress {
        Some(record) => JournalStatus::Recovered(Box::new(record)),
        // A snapshot with no paused operation means the squash died between
        // lifting the working-tree row into its temporary commit and reaching a
        // conflict or completion. Surface it as its own recoverable phase.
        None => match doc.worktree_source {
            Some(snapshot) => {
                JournalStatus::Recovered(Box::new(InProgress::WorktreeSquash(snapshot)))
            }
            None => JournalStatus::None,
        },
    }
}

/// The v1 journal shape, read only to migrate forward. `in_progress` (the old
/// flat `ConflictState`) is captured untyped purely to detect an interrupted
/// operation and name it; `undo`/`redo`/`autostash` — unchanged in v2 — carry
/// over when there is nothing paused.
#[derive(Deserialize, Default)]
#[serde(default)]
struct JournalDocV1 {
    in_progress: Option<serde_json::Value>,
    undo: Vec<UndoRecord>,
    redo: Vec<UndoRecord>,
    autostash: Option<AutostashRecord>,
}

impl JournalDocV1 {
    /// The label of an operation left paused in this v1 journal, if any. The v1
    /// in-progress record (a flat `ConflictState`, also used as the Edit marker)
    /// carries `operation_label`; fall back to a generic name if it is absent.
    fn interrupted_op_label(&self) -> Option<String> {
        let record = self.in_progress.as_ref()?;
        if record.is_null() {
            return None;
        }
        Some(
            record
                .get("operation_label")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("an operation")
                .to_string(),
        )
    }
}

/// Upgrade a v1 document to the current schema, dropping any in-progress record.
fn migrate_v1(old: JournalDocV1) -> JournalDoc {
    JournalDoc {
        version: JOURNAL_VERSION,
        in_progress: None,
        undo: old.undo,
        redo: old.redo,
        autostash: old.autostash,
        worktree_source: None,
    }
}
