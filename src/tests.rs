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

use super::*;
use git_tailor::app::SquashMode;
use git_tailor::repo::{ConflictState, GitRepo, RebaseOutcome, SquashContext};
use git_tailor::{CommitDiff, CommitInfo};

/// Minimal `GitRepo` stub for testing terminal-free dispatch helpers.
struct MockRepo {
    head_ok: bool,
    drop_ok: bool,
    move_ok: bool,
    autofixup_ok: bool,
    autofixup_conflicts: bool,
    abort_ok: bool,
    autostash_restore_ok: bool,
    count_per_file: usize,
    count_ok: bool,
    stage_ok: bool,
    stage_changed: bool,
    undo_skips_autostash: bool,
    redo_skips_autostash: bool,
    /// Counts `autostash_save` invocations so tests can assert the working-tree-
    /// preserving undo/redo paths skip the stash dance.
    autostash_save_calls: std::cell::Cell<usize>,
}

impl Default for MockRepo {
    fn default() -> Self {
        Self {
            head_ok: true,
            drop_ok: true,
            move_ok: true,
            autofixup_ok: true,
            autofixup_conflicts: false,
            abort_ok: true,
            autostash_restore_ok: true,
            count_per_file: 0,
            count_ok: true,
            stage_ok: true,
            stage_changed: true,
            undo_skips_autostash: false,
            redo_skips_autostash: false,
            autostash_save_calls: std::cell::Cell::new(0),
        }
    }
}

fn mock_stage_outcome(ok: bool, changed: bool) -> anyhow::Result<git_tailor::repo::StageOutcome> {
    if !ok {
        return Err(anyhow::anyhow!("stage failed"));
    }
    Ok(if changed {
        git_tailor::repo::StageOutcome::Changed
    } else {
        git_tailor::repo::StageOutcome::NoOp
    })
}

impl GitRepo for MockRepo {
    fn head_oid(&self) -> anyhow::Result<Oid> {
        if self.head_ok {
            Ok(Oid::from("a".repeat(40)))
        } else {
            Err(anyhow::anyhow!("head error"))
        }
    }
    fn list_commits(&self, _: &Oid, _: &Oid) -> anyhow::Result<Vec<CommitInfo>> {
        Ok(vec![])
    }
    fn staged_diff(&self, _context_lines: u32) -> anyhow::Result<Option<CommitDiff>> {
        Ok(None)
    }
    fn staged_diff_for_fragmap(&self) -> anyhow::Result<Option<CommitDiff>> {
        Ok(None)
    }
    fn unstaged_diff(&self, _context_lines: u32) -> anyhow::Result<Option<CommitDiff>> {
        Ok(None)
    }
    fn unstaged_diff_for_fragmap(&self) -> anyhow::Result<Option<CommitDiff>> {
        Ok(None)
    }
    fn drop_commit(&self, _: &Oid, _: &Oid) -> anyhow::Result<RebaseOutcome> {
        if self.drop_ok {
            Ok(RebaseOutcome::Complete)
        } else {
            Err(anyhow::anyhow!("drop failed"))
        }
    }
    fn move_commit(&self, _: &Oid, _: Option<&Oid>, _: &Oid) -> anyhow::Result<RebaseOutcome> {
        if self.move_ok {
            Ok(RebaseOutcome::Complete)
        } else {
            Err(anyhow::anyhow!("move failed"))
        }
    }
    fn rebase_abort(&self, _: &ConflictState) -> anyhow::Result<()> {
        if self.abort_ok {
            Ok(())
        } else {
            Err(anyhow::anyhow!("abort failed"))
        }
    }
    fn read_journal(&self) -> anyhow::Result<git_tailor::repo::JournalStatus> {
        Ok(git_tailor::repo::JournalStatus::None)
    }
    fn clear_journal(&self) -> anyhow::Result<()> {
        Ok(())
    }
    fn prune_stale_journal(&self) -> anyhow::Result<()> {
        Ok(())
    }
    fn clean_journal(&self) -> anyhow::Result<git_tailor::repo::JournalCleanSummary> {
        Ok(git_tailor::repo::JournalCleanSummary {
            refs_removed: 0,
            journal_removed: false,
        })
    }
    fn undo(&self) -> anyhow::Result<git_tailor::repo::UndoOutcome> {
        if self.undo_skips_autostash {
            Ok(git_tailor::repo::UndoOutcome::Done {
                label: "Stage all".to_string(),
            })
        } else {
            Ok(git_tailor::repo::UndoOutcome::Empty)
        }
    }
    fn redo(&self) -> anyhow::Result<git_tailor::repo::UndoOutcome> {
        if self.redo_skips_autostash {
            Ok(git_tailor::repo::UndoOutcome::Done {
                label: "Stage all".to_string(),
            })
        } else {
            Ok(git_tailor::repo::UndoOutcome::Empty)
        }
    }
    fn pending_undo_skips_autostash(&self) -> anyhow::Result<bool> {
        Ok(self.undo_skips_autostash)
    }
    fn pending_redo_skips_autostash(&self) -> anyhow::Result<bool> {
        Ok(self.redo_skips_autostash)
    }
    fn stage_all(&self) -> anyhow::Result<git_tailor::repo::StageOutcome> {
        mock_stage_outcome(self.stage_ok, self.stage_changed)
    }
    fn unstage_all(&self) -> anyhow::Result<git_tailor::repo::StageOutcome> {
        mock_stage_outcome(self.stage_ok, self.stage_changed)
    }
    fn commit_staged(&self, _: &str) -> anyhow::Result<git_tailor::repo::CommitOutcome> {
        if self.stage_ok {
            Ok(if self.stage_changed {
                git_tailor::repo::CommitOutcome::Committed
            } else {
                git_tailor::repo::CommitOutcome::NothingStaged
            })
        } else {
            Err(anyhow::anyhow!("commit failed"))
        }
    }
    fn autostash_save(&mut self) -> anyhow::Result<()> {
        self.autostash_save_calls
            .set(self.autostash_save_calls.get() + 1);
        Ok(())
    }
    fn autostash_restore(&mut self) -> anyhow::Result<git_tailor::repo::AutostashRestore> {
        if self.autostash_restore_ok {
            Ok(git_tailor::repo::AutostashRestore::Done)
        } else {
            Ok(git_tailor::repo::AutostashRestore::Conflict {
                files: vec!["conflict.txt".to_string()],
            })
        }
    }
    fn autostash_conflict_continue(
        &mut self,
    ) -> anyhow::Result<git_tailor::repo::AutostashContinue> {
        Ok(git_tailor::repo::AutostashContinue::Resolved)
    }
    fn autostash_conflict_abort(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
    fn count_split_per_file(&self, _: &Oid) -> anyhow::Result<usize> {
        if self.count_ok {
            Ok(self.count_per_file)
        } else {
            Err(anyhow::anyhow!("count failed"))
        }
    }
    fn commit_diff_for_fragmap(&self, _: &Oid) -> anyhow::Result<CommitDiff> {
        unimplemented!()
    }
    fn find_reference_point(&self, _: &str) -> anyhow::Result<Oid> {
        unimplemented!()
    }
    fn commit_diff(&self, _: &Oid, _context_lines: u32) -> anyhow::Result<CommitDiff> {
        unimplemented!()
    }
    fn split_commit_per_file(&self, _: &Oid, _: &Oid) -> anyhow::Result<()> {
        unimplemented!()
    }
    fn split_commit_per_hunk(&self, _: &Oid, _: &Oid) -> anyhow::Result<()> {
        unimplemented!()
    }
    fn split_commit_per_hunk_group(&self, _: &Oid, _: &Oid, _: &Oid) -> anyhow::Result<()> {
        unimplemented!()
    }
    fn list_commit_files(&self, _: &Oid) -> anyhow::Result<Vec<String>> {
        unimplemented!()
    }
    fn split_commit_out_file(&self, _: &Oid, _: &str, _: &Oid) -> anyhow::Result<()> {
        unimplemented!()
    }
    fn count_split_per_hunk(&self, _: &Oid) -> anyhow::Result<usize> {
        unimplemented!()
    }
    fn count_split_per_hunk_group(&self, _: &Oid, _: &Oid, _: &Oid) -> anyhow::Result<usize> {
        unimplemented!()
    }
    fn reword_commit(&self, _: &Oid, _: &str, _: &Oid) -> anyhow::Result<()> {
        unimplemented!()
    }
    fn get_config_string(&self, _: &str) -> anyhow::Result<Option<String>> {
        unimplemented!()
    }
    fn rebase_continue(&self, _: &ConflictState) -> anyhow::Result<RebaseOutcome> {
        unimplemented!()
    }
    fn workdir(&self) -> Option<std::path::PathBuf> {
        unimplemented!()
    }
    fn read_index_stage(&self, _: &str, _: i32) -> anyhow::Result<Option<Vec<u8>>> {
        unimplemented!()
    }
    fn read_conflicting_files(&self) -> Vec<String> {
        unimplemented!()
    }
    fn squash_commits(&self, _: &Oid, _: &Oid, _: &str, _: &Oid) -> anyhow::Result<RebaseOutcome> {
        unimplemented!()
    }
    fn squash_try_combine(
        &self,
        _: &Oid,
        _: &Oid,
        _: &str,
        _: SquashMode,
        _: &Oid,
    ) -> anyhow::Result<Option<ConflictState>> {
        unimplemented!()
    }
    fn squash_finalize(
        &self,
        _: &SquashContext,
        _: &str,
        _: &Oid,
        _: Option<&git_tailor::repo::AutofixupContext>,
    ) -> anyhow::Result<RebaseOutcome> {
        unimplemented!()
    }
    fn autofixup(
        &self,
        _: &Oid,
        _: &Oid,
        _: &std::collections::HashMap<String, String>,
    ) -> anyhow::Result<RebaseOutcome> {
        if self.autofixup_conflicts {
            return Ok(RebaseOutcome::Conflict(Box::new(ConflictState {
                operation_label: "Squash".to_string(),
                original_branch_oid: Oid::from("a".repeat(40)),
                new_tip_oid: Oid::from("b".repeat(40)),
                conflicting_commit_oid: Oid::from("c".repeat(40)),
                remaining_oids: vec![],
                conflicting_files: vec![],
                still_unresolved: false,
                moved_commit_oid: None,
                squash_context: None,
                is_orphan_root: false,
                autofixup_context: Some(git_tailor::repo::AutofixupContext {
                    reference_oid: Oid::from("d".repeat(40)),
                    message_overrides: std::collections::HashMap::new(),
                }),
            })));
        }
        if self.autofixup_ok {
            Ok(RebaseOutcome::Complete)
        } else {
            Err(anyhow::anyhow!("autofixup failed"))
        }
    }
    fn stage_file(&self, _: &str) -> anyhow::Result<()> {
        unimplemented!()
    }
    fn auto_stage_resolved_conflicts(&self, _: &[String]) -> anyhow::Result<()> {
        unimplemented!()
    }
    fn default_branch(&self) -> anyhow::Result<Option<String>> {
        Ok(None)
    }
    fn root_commit_oid(&self) -> anyhow::Result<Oid> {
        unimplemented!()
    }
    fn commit_walker<'a>(
        &'a self,
        _from_oid: &Oid,
        _to_oid: &Oid,
    ) -> anyhow::Result<Box<dyn Iterator<Item = anyhow::Result<CommitInfo>> + 'a>> {
        unimplemented!()
    }
}

fn make_conflict_state() -> ConflictState {
    ConflictState {
        operation_label: "Drop".to_string(),
        original_branch_oid: Oid::from("b".repeat(40)),
        new_tip_oid: Oid::from("c".repeat(40)),
        conflicting_commit_oid: Oid::from("d".repeat(40)),
        remaining_oids: vec![],
        conflicting_files: vec![],
        still_unresolved: false,
        moved_commit_oid: None,
        squash_context: None,
        is_orphan_root: false,
        autofixup_context: None,
    }
}

#[test]
fn execute_drop_complete_sets_success_message() {
    let mut repo = MockRepo::default();
    let mut app = AppState::default();
    let result = handle_execute_drop(
        &mut repo,
        &mut app,
        Oid::from("a".repeat(40)),
        Oid::from("b".repeat(40)),
    );
    assert!(matches!(result, Ok(LoopAction::ReloadPreserving)));
    assert_eq!(app.status_message.as_deref(), Some("Commit dropped"));
    assert!(!app.status_is_error);
}

#[test]
fn execute_drop_opens_stash_conflict_dialog_on_conflict() {
    // The drop completed, but reapplying the auto-stash conflicted. Instead of a
    // terse error the user is dropped into the resolution dialog (same as a
    // cherry-pick conflict), so their changes are never silently abandoned.
    let mut repo = MockRepo {
        autostash_restore_ok: false,
        ..MockRepo::default()
    };
    let mut app = AppState::default();
    let result = handle_execute_drop(
        &mut repo,
        &mut app,
        Oid::from("a".repeat(40)),
        Oid::from("b".repeat(40)),
    );
    assert!(matches!(result, Ok(LoopAction::Continue)));
    match &app.mode {
        AppMode::StashConflict(state) => {
            assert_eq!(state.operation_label, "Drop");
            assert_eq!(state.conflicting_files, vec!["conflict.txt".to_string()]);
        }
        other => panic!("expected StashConflict mode, got {other:?}"),
    }
}

#[test]
fn execute_drop_error_sets_error_message() {
    let mut repo = MockRepo {
        drop_ok: false,
        ..MockRepo::default()
    };
    let mut app = AppState::default();
    let _ = handle_execute_drop(
        &mut repo,
        &mut app,
        Oid::from("a".repeat(40)),
        Oid::from("b".repeat(40)),
    );
    assert!(app.status_is_error);
    assert!(
        app.status_message
            .as_deref()
            .unwrap_or("")
            .contains("Drop failed")
    );
}

#[test]
fn execute_move_complete_sets_success_message() {
    let mut repo = MockRepo::default();
    let mut app = AppState::default();
    let result = handle_execute_move(&mut repo, &mut app, Oid::from("a".repeat(40)), None);
    assert!(matches!(result, Ok(LoopAction::ReloadPreserving)));
    assert_eq!(app.status_message.as_deref(), Some("Commit moved"));
    assert!(!app.status_is_error);
}

#[test]
fn execute_move_head_error_continues() {
    let mut repo = MockRepo {
        head_ok: false,
        ..MockRepo::default()
    };
    let mut app = AppState::default();
    let result = handle_execute_move(&mut repo, &mut app, Oid::from("a".repeat(40)), None);
    assert!(matches!(result, Ok(LoopAction::Continue)));
    assert!(app.status_is_error);
}

#[test]
fn rebase_abort_success_sets_success_message() {
    let mut repo = MockRepo::default();
    let mut app = AppState::default();
    let state = make_conflict_state();
    let result = handle_rebase_abort(&mut repo, &mut app, state);
    assert!(matches!(result, Ok(LoopAction::Reload)));
    assert!(!app.status_is_error);
    assert!(
        app.status_message
            .as_deref()
            .unwrap_or("")
            .contains("aborted")
    );
}

#[test]
fn rebase_abort_error_sets_error_message() {
    let mut repo = MockRepo {
        abort_ok: false,
        ..MockRepo::default()
    };
    let mut app = AppState::default();
    let state = make_conflict_state();
    let _ = handle_rebase_abort(&mut repo, &mut app, state);
    assert!(app.status_is_error);
    assert!(
        app.status_message
            .as_deref()
            .unwrap_or("")
            .contains("Abort failed")
    );
}

#[test]
fn prepare_split_count_error_sets_error_message() {
    let mut repo = MockRepo {
        count_ok: false,
        ..MockRepo::default()
    };
    let mut app = AppState::default();
    let result = handle_prepare_split(
        &mut repo,
        &mut app,
        SplitStrategy::PerFile,
        Oid::from("a".repeat(40)),
    );
    assert!(matches!(result, Ok(LoopAction::Proceed)));
    assert!(app.status_is_error);
}

#[test]
fn prepare_split_above_threshold_enters_confirm_mode() {
    let mut repo = MockRepo {
        count_per_file: SPLIT_CONFIRM_THRESHOLD + 1,
        ..MockRepo::default()
    };
    let mut app = AppState::default();
    let _ = handle_prepare_split(
        &mut repo,
        &mut app,
        SplitStrategy::PerFile,
        Oid::from("a".repeat(40)),
    );
    assert!(matches!(app.mode, AppMode::SplitConfirm(_)));
}

#[test]
fn stage_all_changed_reloads_and_reports_success() {
    let repo = MockRepo::default();
    let mut app = AppState::default();
    let action = report_stage_outcome(
        &mut app,
        repo.stage_all(),
        "Staged all changes",
        "Nothing to stage",
    );
    assert!(matches!(action, LoopAction::Reload));
    assert_eq!(app.status_message.as_deref(), Some("Staged all changes"));
    assert!(!app.status_is_error);
}

#[test]
fn stage_all_noop_reports_without_reload() {
    let repo = MockRepo {
        stage_changed: false,
        ..MockRepo::default()
    };
    let mut app = AppState::default();
    let action = report_stage_outcome(
        &mut app,
        repo.stage_all(),
        "Staged all changes",
        "Nothing to stage",
    );
    assert!(matches!(action, LoopAction::Proceed));
    assert_eq!(app.status_message.as_deref(), Some("Nothing to stage"));
    assert!(!app.status_is_error);
}

#[test]
fn stage_all_error_sets_error_message() {
    let repo = MockRepo {
        stage_ok: false,
        ..MockRepo::default()
    };
    let mut app = AppState::default();
    let action = report_stage_outcome(
        &mut app,
        repo.unstage_all(),
        "Unstaged all changes",
        "Nothing to unstage",
    );
    assert!(matches!(action, LoopAction::Proceed));
    assert!(app.status_is_error);
}

#[test]
fn worktree_preserving_undo_skips_autostash() {
    // The working-tree-preserving undo paths (stage/unstage all, commit soft
    // reset) must not stash/restore, or they would squirrel away and reapply the
    // very state they are restoring.
    let mut repo = MockRepo {
        undo_skips_autostash: true,
        ..MockRepo::default()
    };
    let mut app = AppState::default();
    let result = handle_undo(&mut repo, &mut app);
    assert!(matches!(result, Ok(LoopAction::Reload)));
    assert_eq!(repo.autostash_save_calls.get(), 0);
    assert_eq!(app.status_message.as_deref(), Some("Undid stage all"));
    assert!(!app.status_is_error);
}

#[test]
fn ref_move_undo_runs_autostash() {
    // A normal (ref-moving) undo still runs the auto-stash dance.
    let mut repo = MockRepo::default();
    let mut app = AppState::default();
    let _ = handle_undo(&mut repo, &mut app);
    assert_eq!(repo.autostash_save_calls.get(), 1);
}

#[test]
fn clean_journal_parses_on_its_own() {
    use clap::Parser;
    assert!(crate::cli::Cli::try_parse_from(["gt", "--clean-journal"]).is_ok());
}

#[test]
fn clean_journal_conflicts_with_browse_args() {
    use clap::Parser;
    for extra in [
        ["--clean-journal", "somebase"],
        ["--clean-journal", "--all"],
        ["--clean-journal", "--static"],
    ] {
        let argv = std::iter::once("gt").chain(extra);
        assert!(
            crate::cli::Cli::try_parse_from(argv).is_err(),
            "--clean-journal must conflict with {extra:?}"
        );
    }
}

#[test]
fn clean_journal_ignores_cosmetic_flags() {
    // Cosmetic flags don't conflict (no TUI is launched), so a globally-set
    // GT_* env var won't wrongly block --clean-journal.
    use clap::Parser;
    assert!(crate::cli::Cli::try_parse_from(["gt", "--clean-journal", "--reverse"]).is_ok());
}

#[test]
fn only_key_events_dismiss_transient_status() {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

    let key_press = Event::Key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));
    assert!(crate::event_dismisses_status(&key_press));

    // A Repeat key event still counts as a keypress.
    let mut repeat = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE);
    repeat.kind = KeyEventKind::Repeat;
    assert!(crate::event_dismisses_status(&Event::Key(repeat)));

    // Non-key events still reach the loop (so it can redraw on resize) but must
    // NOT wipe the status message before it is read.
    assert!(!crate::event_dismisses_status(&Event::Resize(80, 24)));
    assert!(!crate::event_dismisses_status(&Event::FocusGained));
    assert!(!crate::event_dismisses_status(&Event::FocusLost));
}

mod autofixup_selection {
    use super::*;
    use git_tailor::VirtualOid;
    use git_tailor::app::SquashMode as Mode;
    use git_tailor::autofixup::AutofixupPair;

    fn commit(oid: &str) -> CommitInfo {
        CommitInfo {
            oid: VirtualOid::Real(Oid::new(oid.repeat(40))),
            summary: String::new(),
            author: None,
            date: None,
            parent_oids: vec![],
            message: String::new(),
            author_email: None,
            author_date: None,
            committer: None,
            committer_email: None,
            commit_date: None,
        }
    }

    fn synthetic(oid: VirtualOid) -> CommitInfo {
        CommitInfo {
            oid,
            summary: String::new(),
            author: None,
            date: None,
            parent_oids: vec![],
            message: String::new(),
            author_email: None,
            author_date: None,
            committer: None,
            committer_email: None,
            commit_date: None,
        }
    }

    fn pair(source: &str, target: &str) -> AutofixupPair {
        AutofixupPair {
            source_oid: Oid::new(source.repeat(40)),
            target_oid: Oid::new(target.repeat(40)),
            source_summary: String::new(),
            target_summary: String::new(),
            source_message: String::new(),
            target_message: String::new(),
            mode: Mode::Fixup,
        }
    }

    // Layout: A, T(target), F1(fixup->T), C(survivor), F2(fixup->T)
    // Batch removes F1 and F2, folding both into T.
    fn commits() -> Vec<CommitInfo> {
        vec![
            commit("a"),
            commit("t"),
            commit("1"),
            commit("c"),
            commit("2"),
        ]
    }

    fn pairs() -> Vec<AutofixupPair> {
        vec![pair("1", "t"), pair("2", "t")]
    }

    #[test]
    fn selection_on_a_surviving_commit_shifts_down_by_removed_commits_before_it() {
        // C is at index 3; only F1 (index 2) is removed before it -> index 2.
        let idx = crate::autofixup_target_selection_index(&commits(), 3, &pairs());
        assert_eq!(idx, Some(2));
    }

    #[test]
    fn selection_before_any_removal_is_unaffected() {
        // T is at index 1; nothing removed before it.
        let idx = crate::autofixup_target_selection_index(&commits(), 1, &pairs());
        assert_eq!(idx, Some(1));
    }

    #[test]
    fn selection_on_a_folded_away_fixup_lands_on_its_target() {
        // F2 (index 4) was folded into T (index 1); nothing removed before T.
        let idx = crate::autofixup_target_selection_index(&commits(), 4, &pairs());
        assert_eq!(idx, Some(1));
    }

    #[test]
    fn selection_on_the_first_fixup_also_lands_on_its_target() {
        let idx = crate::autofixup_target_selection_index(&commits(), 2, &pairs());
        assert_eq!(idx, Some(1));
    }

    #[test]
    fn synthetic_row_selection_falls_back_to_none() {
        let mut cs = commits();
        cs.push(synthetic(VirtualOid::Unstaged));
        let idx = crate::autofixup_target_selection_index(&cs, 5, &pairs());
        assert_eq!(idx, None, "synthetic rows aren't touched by autofixup");
    }

    #[test]
    fn result_never_exceeds_the_commit_list_bounds() {
        // Even in degenerate/empty inputs the caller still clamps with `.min(len-1)`
        // before assigning to `selection_index` — verify the raw index this
        // function returns is sane (in-bounds) for a normal batch too.
        let idx = crate::autofixup_target_selection_index(&commits(), 4, &pairs()).unwrap();
        assert!(idx < commits().len());
    }

    #[test]
    fn execute_autofixup_reloads_selecting_the_computed_index() {
        let mut repo = MockRepo::default();
        let mut app = AppState {
            commits: commits(),
            selection_index: 4, // F2, folded into T (index 1).
            ..Default::default()
        };

        let result = crate::handle_execute_autofixup(
            &mut repo,
            &mut app,
            Oid::from("a".repeat(40)),
            Oid::from("b".repeat(40)),
            pairs(),
            std::collections::HashMap::new(),
        );

        assert!(matches!(result, Ok(LoopAction::ReloadSelecting(1))));
        assert_eq!(app.status_message.as_deref(), Some("Commits autofixed up"));
    }

    #[test]
    fn execute_autofixup_stashes_the_index_when_it_hits_a_conflict() {
        let mut repo = MockRepo {
            autofixup_conflicts: true,
            ..Default::default()
        };
        let mut app = AppState {
            commits: commits(),
            selection_index: 4, // F2, folded into T (index 1).
            ..Default::default()
        };

        let result = crate::handle_execute_autofixup(
            &mut repo,
            &mut app,
            Oid::from("a".repeat(40)),
            Oid::from("b".repeat(40)),
            pairs(),
            std::collections::HashMap::new(),
        );

        assert!(matches!(result, Ok(LoopAction::Continue)));
        assert_eq!(
            app.pending_autofixup_selection,
            Some(1),
            "the target index computed up front must survive into the conflict dialog"
        );
    }

    #[test]
    fn apply_pending_selection_swaps_reload_preserving_for_reload_selecting() {
        let mut app = AppState {
            pending_autofixup_selection: Some(2),
            ..Default::default()
        };
        let result =
            crate::apply_pending_autofixup_selection(&mut app, true, LoopAction::ReloadPreserving);
        assert!(matches!(result, LoopAction::ReloadSelecting(2)));
        assert_eq!(
            app.pending_autofixup_selection, None,
            "consumed on the completing round"
        );
    }

    #[test]
    fn apply_pending_selection_is_a_no_op_for_non_autofixup_operations() {
        let mut app = AppState {
            pending_autofixup_selection: Some(2),
            ..Default::default()
        };
        let result =
            crate::apply_pending_autofixup_selection(&mut app, false, LoopAction::ReloadPreserving);
        assert!(matches!(result, LoopAction::ReloadPreserving));
        assert_eq!(
            app.pending_autofixup_selection,
            Some(2),
            "not this batch's field to touch"
        );
    }

    #[test]
    fn apply_pending_selection_falls_back_when_nothing_was_stashed() {
        let mut app = AppState::default();
        let result =
            crate::apply_pending_autofixup_selection(&mut app, true, LoopAction::ReloadPreserving);
        assert!(matches!(result, LoopAction::ReloadPreserving));
    }

    #[test]
    fn apply_pending_selection_keeps_the_index_across_another_conflict_round() {
        let mut app = AppState {
            pending_autofixup_selection: Some(2),
            ..Default::default()
        };
        let result = crate::apply_pending_autofixup_selection(&mut app, true, LoopAction::Continue);
        assert!(matches!(result, LoopAction::Continue));
        assert_eq!(
            app.pending_autofixup_selection,
            Some(2),
            "still resolving the batch; next round needs it"
        );
    }

    #[test]
    fn apply_pending_selection_clears_stale_state_on_failure() {
        let mut app = AppState {
            pending_autofixup_selection: Some(2),
            ..Default::default()
        };
        let result = crate::apply_pending_autofixup_selection(&mut app, true, LoopAction::Proceed);
        assert!(matches!(result, LoopAction::Proceed));
        assert_eq!(
            app.pending_autofixup_selection, None,
            "batch abandoned; don't leak into a later, unrelated reload"
        );
    }
}
