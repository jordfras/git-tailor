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
                    // The changes this record describes are not on disk — the
                    // fold had not put them back yet — so discarding it silently
                    // would take uncommitted work with it. Keep the tree it
                    // named, before the journal that pins it goes away.
                    let rescued = git_repo.rescue_lifted_row(&snapshot).ok().flatten();
                    let _ = git_repo.clear_journal();
                    app.set_error_message(match rescued {
                        Some(kept) => format!(
                            "Discarded a stale interrupted-operation journal (branch has \
                             moved); the working tree it recorded is kept at {kept}"
                        ),
                        None => "Discarded a stale interrupted-operation journal (branch has \
                                 moved)"
                            .to_string(),
                    });
                } else {
                    match git_repo.restore_lifted_row(&snapshot) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock_repo::{MockRepo, make_conflict_state, mock_lifted_row};
    use git_tailor::Oid;
    use git_tailor::repo::{ConflictState, EditInProgress, LiftedRow, WorktreeSource};

    /// The OID `MockRepo::head_oid` reports.
    fn mock_head() -> Oid {
        Oid::from("a".repeat(40))
    }

    /// A snapshot whose temporary commit is where the branch actually is — an
    /// operation this run can still recover.
    fn live_snapshot(source: WorktreeSource) -> LiftedRow {
        LiftedRow {
            source,
            temp_oid: mock_head(),
            ..mock_lifted_row()
        }
    }

    /// A paused conflict whose new tip is where the branch actually is.
    fn conflict_at_head() -> ConflictState {
        ConflictState {
            new_tip_oid: mock_head(),
            ..make_conflict_state()
        }
    }

    fn recover(repo: &mut MockRepo) -> AppState {
        let mut app = AppState::default();
        check_journal_recovery(repo, &mut app);
        app
    }

    #[test]
    fn an_interrupted_fold_still_on_its_temporary_commit_is_unwound() {
        let mut repo = MockRepo {
            journal: Some(InProgress::WorktreeSquash(live_snapshot(
                WorktreeSource::Staged,
            ))),
            ..Default::default()
        };
        let app = recover(&mut repo);

        assert_eq!(repo.restore_lifted_calls.get(), 1);
        assert_eq!(repo.clear_journal_calls.get(), 0);
        assert_eq!(
            app.status.message.as_deref(),
            Some("Recovered an interrupted squash of staged changes — restored the branch")
        );
    }

    /// The row the fold started from names itself in the report, so the user can
    /// tell which half of their work just came back.
    #[test]
    fn an_unwound_fold_names_the_row_it_started_from() {
        let mut repo = MockRepo {
            journal: Some(InProgress::WorktreeSquash(live_snapshot(
                WorktreeSource::Unstaged,
            ))),
            ..Default::default()
        };
        let app = recover(&mut repo);

        assert_eq!(
            app.status.message.as_deref(),
            Some("Recovered an interrupted squash of unstaged changes — restored the branch")
        );
    }

    /// Unwinding is only lossless while the branch is still on the temporary
    /// commit. Anywhere else, the repository has moved on without us and the
    /// rewind would take the user's later work with it.
    #[test]
    fn an_interrupted_fold_whose_branch_has_moved_is_discarded() {
        let mut repo = MockRepo {
            journal: Some(InProgress::WorktreeSquash(mock_lifted_row())),
            ..Default::default()
        };
        let app = recover(&mut repo);

        assert_eq!(
            repo.restore_lifted_calls.get(),
            0,
            "a record that no longer describes the repository must not be applied"
        );
        assert_eq!(repo.clear_journal_calls.get(), 1);
        assert_eq!(
            app.status.message.as_deref(),
            Some("Discarded a stale interrupted-operation journal (branch has moved)")
        );
    }

    /// Discarding the record throws away the only reference to the working tree
    /// it named, so the report says where that tree was kept.
    #[test]
    fn a_discarded_fold_names_where_its_working_tree_went() {
        let mut repo = MockRepo {
            journal: Some(InProgress::WorktreeSquash(mock_lifted_row())),
            rescued_ref: Some("refs/git-tailor/rescue/eeeeeeee".to_string()),
            ..Default::default()
        };
        let app = recover(&mut repo);

        assert_eq!(
            app.status.message.as_deref(),
            Some(
                "Discarded a stale interrupted-operation journal (branch has moved); \
                 the working tree it recorded is kept at refs/git-tailor/rescue/eeeeeeee"
            )
        );
    }

    /// HEAD that cannot be read is not a match, so the record is discarded
    /// rather than applied on a guess.
    #[test]
    fn an_interrupted_fold_is_discarded_when_head_cannot_be_read() {
        let mut repo = MockRepo {
            head_ok: false,
            journal: Some(InProgress::WorktreeSquash(live_snapshot(
                WorktreeSource::Staged,
            ))),
            ..Default::default()
        };
        let app = recover(&mut repo);

        assert_eq!(repo.restore_lifted_calls.get(), 0);
        assert_eq!(repo.clear_journal_calls.get(), 1);
        assert!(app.status.is_error);
    }

    #[test]
    fn a_failed_fold_recovery_reports_the_underlying_cause() {
        let mut repo = MockRepo {
            journal: Some(InProgress::WorktreeSquash(live_snapshot(
                WorktreeSource::Staged,
            ))),
            restore_lifted_ok: false,
            ..Default::default()
        };
        let app = recover(&mut repo);

        let message = app.status.message.unwrap();
        assert!(
            message.starts_with("Failed to recover an interrupted squash of staged changes:"),
            "got {message:?}"
        );
        assert!(
            message.contains("ref is locked"),
            "the underlying cause must survive: {message:?}"
        );
    }

    #[test]
    fn an_interrupted_edit_restores_the_branch() {
        let mut repo = MockRepo {
            journal: Some(InProgress::Edit(EditInProgress {
                original_branch_oid: mock_head(),
                ..Default::default()
            })),
            ..Default::default()
        };
        let app = recover(&mut repo);

        assert_eq!(
            app.status.message.as_deref(),
            Some(
                "Recovered an interrupted Edit — restored the branch \
                 (in-shell commits remain in the reflog)"
            )
        );
    }

    /// A paused conflict is the one case the user is asked about, because
    /// resuming it needs their resolution.
    #[test]
    fn a_paused_conflict_still_at_its_tip_is_offered_for_recovery() {
        let mut repo = MockRepo {
            journal: Some(InProgress::Conflict(Box::new(conflict_at_head()))),
            ..Default::default()
        };
        let app = recover(&mut repo);

        assert!(matches!(app.mode, AppMode::RecoverConfirm(_)));
        assert_eq!(repo.clear_journal_calls.get(), 0);
    }

    #[test]
    fn a_paused_conflict_whose_branch_has_moved_is_discarded() {
        let mut repo = MockRepo {
            journal: Some(InProgress::Conflict(Box::new(make_conflict_state()))),
            ..Default::default()
        };
        let app = recover(&mut repo);

        assert!(!matches!(app.mode, AppMode::RecoverConfirm(_)));
        assert_eq!(repo.clear_journal_calls.get(), 1);
        assert_eq!(
            app.status.message.as_deref(),
            Some("Discarded a stale interrupted-operation journal (branch has moved)")
        );
    }
}
