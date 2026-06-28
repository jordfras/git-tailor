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
    fn staged_diff(&self) -> anyhow::Result<Option<CommitDiff>> {
        Ok(None)
    }
    fn unstaged_diff(&self) -> anyhow::Result<Option<CommitDiff>> {
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
    fn commit_diff(&self, _: &Oid) -> anyhow::Result<CommitDiff> {
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
    ) -> anyhow::Result<RebaseOutcome> {
        unimplemented!()
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
    let repo = MockRepo {
        count_ok: false,
        ..MockRepo::default()
    };
    let mut app = AppState::default();
    let result = handle_prepare_split(
        &repo,
        &mut app,
        SplitStrategy::PerFile,
        Oid::from("a".repeat(40)),
    );
    assert!(matches!(result, Ok(LoopAction::Proceed)));
    assert!(app.status_is_error);
}

#[test]
fn prepare_split_above_threshold_enters_confirm_mode() {
    let repo = MockRepo {
        count_per_file: SPLIT_CONFIRM_THRESHOLD + 1,
        ..MockRepo::default()
    };
    let mut app = AppState::default();
    let _ = handle_prepare_split(
        &repo,
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
