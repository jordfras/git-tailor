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
    count_per_file: usize,
    count_ok: bool,
}

impl Default for MockRepo {
    fn default() -> Self {
        Self {
            head_ok: true,
            drop_ok: true,
            move_ok: true,
            abort_ok: true,
            count_per_file: 0,
            count_ok: true,
        }
    }
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
    fn staged_diff(&self) -> Option<CommitDiff> {
        None
    }
    fn unstaged_diff(&self) -> Option<CommitDiff> {
        None
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
    fn count_split_per_hunk(&self, _: &Oid) -> anyhow::Result<usize> {
        unimplemented!()
    }
    fn count_split_per_hunk_group(&self, _: &Oid, _: &Oid, _: &Oid) -> anyhow::Result<usize> {
        unimplemented!()
    }
    fn reword_commit(&self, _: &Oid, _: &str, _: &Oid) -> anyhow::Result<()> {
        unimplemented!()
    }
    fn get_config_string(&self, _: &str) -> Option<String> {
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
    fn default_branch(&self) -> Option<String> {
        None
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
    let repo = MockRepo::default();
    let mut app = AppState::default();
    let result = handle_execute_drop(
        &repo,
        &mut app,
        Oid::from("a".repeat(40)),
        Oid::from("b".repeat(40)),
    );
    assert!(matches!(result, Ok(LoopAction::ReloadPreserving)));
    assert_eq!(app.status_message.as_deref(), Some("Commit dropped"));
    assert!(!app.status_is_error);
}

#[test]
fn execute_drop_error_sets_error_message() {
    let repo = MockRepo {
        drop_ok: false,
        ..MockRepo::default()
    };
    let mut app = AppState::default();
    let _ = handle_execute_drop(
        &repo,
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
    let repo = MockRepo::default();
    let mut app = AppState::default();
    let result = handle_execute_move(&repo, &mut app, Oid::from("a".repeat(40)), None);
    assert!(matches!(result, Ok(LoopAction::ReloadPreserving)));
    assert_eq!(app.status_message.as_deref(), Some("Commit moved"));
    assert!(!app.status_is_error);
}

#[test]
fn execute_move_head_error_continues() {
    let repo = MockRepo {
        head_ok: false,
        ..MockRepo::default()
    };
    let mut app = AppState::default();
    let result = handle_execute_move(&repo, &mut app, Oid::from("a".repeat(40)), None);
    assert!(matches!(result, Ok(LoopAction::Continue)));
    assert!(app.status_is_error);
}

#[test]
fn rebase_abort_success_sets_success_message() {
    let repo = MockRepo::default();
    let mut app = AppState::default();
    let state = make_conflict_state();
    let result = handle_rebase_abort(&repo, &mut app, state);
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
    let repo = MockRepo {
        abort_ok: false,
        ..MockRepo::default()
    };
    let mut app = AppState::default();
    let state = make_conflict_state();
    let _ = handle_rebase_abort(&repo, &mut app, state);
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
