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

//! Startup recovery: work out what a previous run was killed in the middle of
//! and either undo it, offer to resume it, or say why neither is safe.
//!
//! Every decision here turns on whether the journal still describes the
//! repository in front of us. A record whose branch has moved on describes
//! something that happened before the user's later work, and acting on it would
//! take that work with it.

use git_tailor::app::{AppMode, AppState};
use git_tailor::repo::{AutostashRestore, GitRepo, InProgress, JournalStatus, StashConflictState};

/// On startup, detect an operation a previous run was killed in the middle of
/// (from the persisted journal) and surface a recovery prompt — or inform the
/// user when the journal can't be used.
pub(crate) fn check_journal_recovery(git_repo: &mut impl GitRepo, app: &mut AppState) {
    // Drop undo/redo history (and its gc-pin refs) left stale by external
    // history changes, so it doesn't clutter the journal or tools like gitk.
    let _ = git_repo.prune_stale_journal();

    match git_repo.read_journal() {
        Ok(JournalStatus::Recovered(record)) => match *record {
            InProgress::Edit(_) => {
                // A crashed Edit: the shell session cannot be resumed and the
                // branch may be anywhere the user left it, so the only safe action
                // is to restore the branch to its original tip. In-shell commits
                // remain reachable via the reflog and the pinned `orig` ref;
                // uncommitted in-shell changes are discarded by the restoring
                // checkout.
                match git_repo.abort_edit() {
                    Ok(()) => app.set_error_message(
                        "Recovered an interrupted Edit — restored the branch \
                         (in-shell commits remain in the reflog)",
                    ),
                    Err(e) => app
                        .set_error_message(format!("Failed to recover an interrupted Edit: {e:#}")),
                }
            }
            InProgress::WorktreeSquash(snapshot) => {
                // A squash of working-tree changes died between lifting them
                // into a temporary commit and folding it away. Unwinding is
                // lossless — the snapshot holds the exact pre-operation state,
                // so there is nothing to prompt about — but only while the
                // branch is still on that temporary commit. Anywhere else and
                // the repository has moved on without us, and the rewind would
                // take the user's later work with it.
                let label = snapshot.source.label().to_lowercase();
                let head_matches = git_repo
                    .head_oid()
                    .map(|head| head == snapshot.temp_oid)
                    .unwrap_or(false);
                if !head_matches {
                    let _ = git_repo.clear_journal();
                    app.set_error_message(
                        "Discarded a stale interrupted-operation journal (branch has moved)",
                    );
                } else {
                    match git_repo.abort_worktree_source(&snapshot) {
                        Ok(()) => app.set_error_message(format!(
                            "Recovered an interrupted squash of {label} — restored the branch"
                        )),
                        Err(e) => app.set_error_message(format!(
                            "Failed to recover an interrupted squash of {label}: {e:#}"
                        )),
                    }
                }
            }
            InProgress::Conflict(state) => {
                // Only offer recovery when the branch is still where the
                // interrupted operation left it; otherwise the journal is stale
                // (history changed outside git-tailor) and resuming or aborting
                // would be unsafe.
                let head_matches = git_repo
                    .head_oid()
                    .map(|head| head == state.new_tip_oid)
                    .unwrap_or(false);
                if head_matches {
                    app.enter_recover_confirm(*state);
                } else {
                    let _ = git_repo.clear_journal();
                    app.set_error_message(
                        "Discarded a stale interrupted-operation journal (branch has moved)",
                    );
                }
            }
        },
        Ok(JournalStatus::NewerVersion(v)) => {
            app.set_error_message(format!(
                "Ignoring a journal written by a newer git-tailor (format v{v}); \
                 upgrade git-tailor or remove .git/git-tailor/journal.json"
            ));
        }
        // Handled before the TUI starts (see main): this path bails out early,
        // so reaching here would be a bug. Nothing to do.
        Ok(JournalStatus::UpgradeInterrupted { .. }) => {}
        Ok(JournalStatus::Corrupt(e)) => {
            app.set_error_message(format!("Ignoring unreadable operation journal: {e}"));
        }
        Ok(JournalStatus::None) => {}
        Err(e) => {
            app.set_error_message(format!("Failed to read operation journal: {e:#}"));
        }
    }

    // A leftover auto-stash with no operation to recover (e.g. a crash after the
    // op finished but before the stash was reapplied) — restore it now. If it
    // conflicts (or a previous run already left markers), open the resolution
    // dialog so the user can finish or abort rather than being stuck.
    if !matches!(app.mode, AppMode::RecoverConfirm(_))
        && let Ok(AutostashRestore::Conflict { files }) = git_repo.autostash_restore()
    {
        app.enter_stash_conflict(StashConflictState {
            operation_label: "the operation".to_string(),
            conflicting_files: files,
            still_unresolved: false,
        });
    }
}
