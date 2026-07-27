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

use git_tailor::Oid;
use git_tailor::app::{AppMode, AppState, SplitStrategy, SquashMode};
use git_tailor::repo::{ConflictState, GitRepo, RebaseOutcome, RepoRead, SquashContext};
use git_tailor::{
    CommitDiff, CommitInfo, DeltaStatus, DiffLine, DiffLineKind, FileDiff, Hunk, VirtualOid,
};

use super::conflict::ToolRun;
use super::split::SPLIT_CONFIRM_THRESHOLD;
use super::*;

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
    /// Configurable `commit_diff` result, for `handle_prepare_split_out_hunks` tests.
    commit_diff: Option<CommitDiff>,
    /// Files reported by `read_conflicting_files`, for the conflict-tool tests.
    conflicting_files: Vec<String>,
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
            commit_diff: None,
            conflicting_files: Vec::new(),
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

impl RepoRead for MockRepo {
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
    fn commit_diff_for_fragmap(&self, _: &Oid) -> anyhow::Result<CommitDiff> {
        unimplemented!()
    }
    fn find_reference_point(&self, _: &str) -> anyhow::Result<Oid> {
        unimplemented!()
    }
    fn commit_diff(&self, _: &Oid, _context_lines: u32) -> anyhow::Result<CommitDiff> {
        self.commit_diff
            .clone()
            .ok_or_else(|| anyhow::anyhow!("commit_diff not configured"))
    }
    fn get_config_string(&self, _: &str) -> anyhow::Result<Option<String>> {
        unimplemented!()
    }
    fn workdir(&self) -> Option<std::path::PathBuf> {
        unimplemented!()
    }
    fn is_worktree_dirty(&self) -> anyhow::Result<bool> {
        Ok(false)
    }
    fn read_index_stage(&self, _: &str, _: i32) -> anyhow::Result<Option<Vec<u8>>> {
        unimplemented!()
    }
    fn read_conflicting_files(&self) -> Vec<String> {
        self.conflicting_files.clone()
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

impl GitRepo for MockRepo {
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
    fn begin_edit(&self, _: &Oid, _: &Oid) -> anyhow::Result<()> {
        unimplemented!()
    }
    fn finish_edit(&self, _: &Oid) -> anyhow::Result<git_tailor::repo::EditOutcome> {
        unimplemented!()
    }
    fn abort_edit(&self) -> anyhow::Result<()> {
        unimplemented!()
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
    fn split_commit_per_file(&self, _: &Oid, _: &Oid) -> anyhow::Result<()> {
        unimplemented!()
    }
    fn split_commit_per_hunk(&self, _: &Oid, _: &Oid) -> anyhow::Result<()> {
        unimplemented!()
    }
    fn split_commit_per_hunk_group(&self, _: &Oid, _: &Oid, _: &Oid) -> anyhow::Result<()> {
        unimplemented!()
    }
    fn split_commit_out_files(&self, _: &Oid, _: &[String], _: &Oid) -> anyhow::Result<()> {
        unimplemented!()
    }
    fn split_commit_out_hunks(
        &self,
        _: &Oid,
        _: &[(usize, usize)],
        _: &Oid,
        _: u32,
    ) -> anyhow::Result<()> {
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
    fn rebase_continue(&self, _: &ConflictState) -> anyhow::Result<RebaseOutcome> {
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
                autofixup_context: Some(git_tailor::repo::AutofixupContext {
                    reference_oid: Oid::from("d".repeat(40)),
                    message_overrides: std::collections::HashMap::new(),
                }),
                ..Default::default()
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
}

fn make_conflict_state() -> ConflictState {
    ConflictState {
        operation_label: "Drop".to_string(),
        original_branch_oid: Oid::from("b".repeat(40)),
        new_tip_oid: Oid::from("c".repeat(40)),
        conflicting_commit_oid: Oid::from("d".repeat(40)),
        remaining_oids: vec![],
        conflicting_files: vec![],
        ..Default::default()
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

/// A single-line hunk, for `handle_prepare_split_out_hunks` fixtures.
fn one_line_hunk(old_start: u32) -> Hunk {
    Hunk {
        old_start,
        old_lines: 1,
        new_start: old_start,
        new_lines: 1,
        lines: vec![
            DiffLine {
                kind: DiffLineKind::Deletion,
                content: "old\n".to_string(),
            },
            DiffLine {
                kind: DiffLineKind::Addition,
                content: "new\n".to_string(),
            },
        ],
    }
}

/// Two files, three hunks total: a.txt has two, b.txt has one — matching the
/// commit_diff fixture `handle_prepare_split_out_hunks` flattens into
/// `HunkPickerEntry` rows.
fn three_hunk_commit_diff() -> CommitDiff {
    CommitDiff {
        commit: CommitInfo {
            oid: VirtualOid::Real(Oid::from("a".repeat(40))),
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
        },
        files: vec![
            FileDiff {
                old_path: Some("a.txt".to_string()),
                new_path: Some("a.txt".to_string()),
                status: DeltaStatus::Modified,
                hunks: vec![one_line_hunk(1), one_line_hunk(10)],
            },
            FileDiff {
                old_path: Some("b.txt".to_string()),
                new_path: Some("b.txt".to_string()),
                status: DeltaStatus::Modified,
                hunks: vec![one_line_hunk(1)],
            },
        ],
    }
}

/// The commit's diff is flattened into one `HunkPickerEntry` per hunk, in
/// file/hunk order, with `delta_idx`/`hunk_idx` matching that position — the
/// exact pair the backend expects back when the split is confirmed.
#[test]
fn prepare_split_out_hunks_flattens_diff_into_picker_entries() {
    let repo = MockRepo {
        commit_diff: Some(three_hunk_commit_diff()),
        ..MockRepo::default()
    };
    let mut app = AppState::default();

    let result = handle_prepare_split_out_hunks(&repo, &mut app, Oid::from("a".repeat(40)), 3);

    assert!(matches!(result, Ok(LoopAction::Proceed)));
    match &app.mode {
        AppMode::SplitHunksSelect {
            hunks,
            context_lines,
            ..
        } => {
            let ids: Vec<(usize, usize, &str)> = hunks
                .iter()
                .map(|h| (h.delta_idx, h.hunk_idx, h.file_path.as_str()))
                .collect();
            assert_eq!(ids, vec![(0, 0, "a.txt"), (0, 1, "a.txt"), (1, 0, "b.txt")]);
            assert_eq!(*context_lines, 3);
        }
        other => panic!("expected SplitHunksSelect mode, got {other:?}"),
    }
}

/// A commit with fewer than 2 hunks total refuses to open the picker — an
/// empty or single-hunk "rest" split is meaningless.
#[test]
fn prepare_split_out_hunks_refuses_fewer_than_two_hunks() {
    let mut diff = three_hunk_commit_diff();
    diff.files.truncate(1);
    diff.files[0].hunks.truncate(1);
    let repo = MockRepo {
        commit_diff: Some(diff),
        ..MockRepo::default()
    };
    let mut app = AppState::default();

    let result = handle_prepare_split_out_hunks(&repo, &mut app, Oid::from("a".repeat(40)), 3);

    assert!(matches!(result, Ok(LoopAction::Proceed)));
    assert!(app.status_is_error);
    assert_eq!(app.mode, AppMode::CommitList);
}

/// A `commit_diff` failure surfaces as an error message rather than entering
/// the picker with stale or empty data.
#[test]
fn prepare_split_out_hunks_error_sets_error_message() {
    let repo = MockRepo::default(); // commit_diff left unconfigured -> Err
    let mut app = AppState::default();

    let result = handle_prepare_split_out_hunks(&repo, &mut app, Oid::from("a".repeat(40)), 3);

    assert!(matches!(result, Ok(LoopAction::Proceed)));
    assert!(app.status_is_error);
    assert_eq!(app.mode, AppMode::CommitList);
}

/// Three files: a.txt and b.txt modified, c.txt deleted (no `new_path`) — the
/// deleted-file case that exercises `FileDiff`'s "fall back to old_path"
/// identity resolution used both here and in `split_files_select`.
fn three_file_commit_diff() -> CommitDiff {
    CommitDiff {
        commit: CommitInfo {
            oid: VirtualOid::Real(Oid::from("a".repeat(40))),
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
        },
        files: vec![
            FileDiff {
                old_path: Some("a.txt".to_string()),
                new_path: Some("a.txt".to_string()),
                status: DeltaStatus::Modified,
                hunks: vec![one_line_hunk(1)],
            },
            FileDiff {
                old_path: Some("b.txt".to_string()),
                new_path: Some("b.txt".to_string()),
                status: DeltaStatus::Modified,
                hunks: vec![one_line_hunk(1)],
            },
            FileDiff {
                old_path: Some("c.txt".to_string()),
                new_path: None,
                status: DeltaStatus::Deleted,
                hunks: vec![one_line_hunk(1)],
            },
        ],
    }
}

/// The commit's diff is loaded as-is into the picker's file list, in diff
/// order — including a deleted file, whose identity must resolve to its
/// `old_path` since it has no `new_path`.
#[test]
fn prepare_split_out_files_loads_diff_into_picker_files() {
    let repo = MockRepo {
        commit_diff: Some(three_file_commit_diff()),
        ..MockRepo::default()
    };
    let mut app = AppState::default();

    let result = handle_prepare_split_out_files(&repo, &mut app, Oid::from("a".repeat(40)));

    assert!(matches!(result, Ok(LoopAction::Proceed)));
    match &app.mode {
        AppMode::SplitFilesSelect { files, .. } => {
            let paths: Vec<Option<&str>> = files
                .iter()
                .map(|f| f.new_path.as_deref().or(f.old_path.as_deref()))
                .collect();
            assert_eq!(paths, vec![Some("a.txt"), Some("b.txt"), Some("c.txt")]);
        }
        other => panic!("expected SplitFilesSelect mode, got {other:?}"),
    }
}

/// A commit with fewer than 2 changed files refuses to open the picker — an
/// empty or single-file "rest" split is meaningless.
#[test]
fn prepare_split_out_files_refuses_fewer_than_two_files() {
    let mut diff = three_file_commit_diff();
    diff.files.truncate(1);
    let repo = MockRepo {
        commit_diff: Some(diff),
        ..MockRepo::default()
    };
    let mut app = AppState::default();

    let result = handle_prepare_split_out_files(&repo, &mut app, Oid::from("a".repeat(40)));

    assert!(matches!(result, Ok(LoopAction::Proceed)));
    assert!(app.status_is_error);
    assert_eq!(app.mode, AppMode::CommitList);
}

/// A `commit_diff` failure surfaces as an error message rather than entering
/// the picker with stale or empty data.
#[test]
fn prepare_split_out_files_error_sets_error_message() {
    let repo = MockRepo::default(); // commit_diff left unconfigured -> Err
    let mut app = AppState::default();

    let result = handle_prepare_split_out_files(&repo, &mut app, Oid::from("a".repeat(40)));

    assert!(matches!(result, Ok(LoopAction::Proceed)));
    assert!(app.status_is_error);
    assert_eq!(app.mode, AppMode::CommitList);
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

#[test]
fn conflict_tool_finished_refreshes_rebase_dialog() {
    // After a tool resolved (some) files, the rebase-conflict dialog is rebuilt
    // with the still-conflicting files and a success banner naming the tool.
    let repo = MockRepo {
        conflicting_files: vec!["a.txt".to_string()],
        ..MockRepo::default()
    };
    let mut app = AppState::default();
    let action = handle_run_conflict_tool(
        &repo,
        &mut app,
        make_conflict_state(),
        "Merge tool",
        Ok(ToolRun::Finished),
    );
    assert!(matches!(action, LoopAction::Proceed));
    match &app.mode {
        AppMode::RebaseConflict(state) => {
            assert_eq!(state.conflicting_files, vec!["a.txt".to_string()]);
            assert!(!state.still_unresolved);
        }
        other => panic!("expected RebaseConflict mode, got {other:?}"),
    }
    assert!(
        app.status_message
            .as_deref()
            .unwrap_or("")
            .contains("Merge tool finished")
    );
    assert!(!app.status_is_error);
}

#[test]
fn conflict_tool_no_merge_tool_sets_error() {
    let repo = MockRepo::default();
    let mut app = AppState::default();
    let action = handle_run_conflict_tool(
        &repo,
        &mut app,
        make_conflict_state(),
        "Merge tool",
        Ok(ToolRun::NoMergeTool),
    );
    assert!(matches!(action, LoopAction::Proceed));
    assert!(app.status_is_error);
    assert!(
        app.status_message
            .as_deref()
            .unwrap_or("")
            .contains("No merge tool configured")
    );
}

#[test]
fn conflict_tool_failure_reports_the_tool_name() {
    let repo = MockRepo::default();
    let mut app = AppState::default();
    let action = handle_run_conflict_tool(
        &repo,
        &mut app,
        make_conflict_state(),
        "Editor",
        Err(anyhow::anyhow!("boom")),
    );
    assert!(matches!(action, LoopAction::Proceed));
    assert!(app.status_is_error);
    let msg = app.status_message.as_deref().unwrap_or("");
    assert!(msg.contains("Editor failed"), "unexpected message: {msg}");
    assert!(msg.contains("boom"), "unexpected message: {msg}");
}

#[test]
fn stash_tool_finished_refreshes_stash_dialog_keeping_the_label() {
    let repo = MockRepo {
        conflicting_files: vec!["b.txt".to_string()],
        ..MockRepo::default()
    };
    // handle_run_stash_tool reads the operation label off the current mode.
    let mut app = AppState {
        mode: AppMode::StashConflict(Box::new(StashConflictState {
            operation_label: "Drop".to_string(),
            conflicting_files: vec![],
            still_unresolved: true,
        })),
        ..Default::default()
    };
    let action = handle_run_stash_tool(&repo, &mut app, "Editor", Ok(ToolRun::Finished));
    assert!(matches!(action, LoopAction::Proceed));
    match &app.mode {
        AppMode::StashConflict(state) => {
            assert_eq!(state.operation_label, "Drop");
            assert_eq!(state.conflicting_files, vec!["b.txt".to_string()]);
            assert!(!state.still_unresolved);
        }
        other => panic!("expected StashConflict mode, got {other:?}"),
    }
    assert!(
        app.status_message
            .as_deref()
            .unwrap_or("")
            .contains("Editor finished")
    );
}

mod autofixup_selection {
    use super::*;
    use crate::dispatch::autofixup::{
        apply_pending_autofixup_selection, autofixup_target_selection_index,
        handle_execute_autofixup,
    };
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
        let idx = autofixup_target_selection_index(&commits(), 3, &pairs());
        assert_eq!(idx, Some(2));
    }

    #[test]
    fn selection_before_any_removal_is_unaffected() {
        // T is at index 1; nothing removed before it.
        let idx = autofixup_target_selection_index(&commits(), 1, &pairs());
        assert_eq!(idx, Some(1));
    }

    #[test]
    fn selection_on_a_folded_away_fixup_lands_on_its_target() {
        // F2 (index 4) was folded into T (index 1); nothing removed before T.
        let idx = autofixup_target_selection_index(&commits(), 4, &pairs());
        assert_eq!(idx, Some(1));
    }

    #[test]
    fn selection_on_the_first_fixup_also_lands_on_its_target() {
        let idx = autofixup_target_selection_index(&commits(), 2, &pairs());
        assert_eq!(idx, Some(1));
    }

    #[test]
    fn synthetic_row_selection_falls_back_to_none() {
        let mut cs = commits();
        cs.push(synthetic(VirtualOid::Unstaged));
        let idx = autofixup_target_selection_index(&cs, 5, &pairs());
        assert_eq!(idx, None, "synthetic rows aren't touched by autofixup");
    }

    #[test]
    fn result_never_exceeds_the_commit_list_bounds() {
        // Even in degenerate/empty inputs the caller still clamps with `.min(len-1)`
        // before assigning to `selection_index` — verify the raw index this
        // function returns is sane (in-bounds) for a normal batch too.
        let idx = autofixup_target_selection_index(&commits(), 4, &pairs()).unwrap();
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

        let result = handle_execute_autofixup(
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

        let result = handle_execute_autofixup(
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
            apply_pending_autofixup_selection(&mut app, true, LoopAction::ReloadPreserving);
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
            apply_pending_autofixup_selection(&mut app, false, LoopAction::ReloadPreserving);
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
            apply_pending_autofixup_selection(&mut app, true, LoopAction::ReloadPreserving);
        assert!(matches!(result, LoopAction::ReloadPreserving));
    }

    #[test]
    fn apply_pending_selection_keeps_the_index_across_another_conflict_round() {
        let mut app = AppState {
            pending_autofixup_selection: Some(2),
            ..Default::default()
        };
        let result = apply_pending_autofixup_selection(&mut app, true, LoopAction::Continue);
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
        let result = apply_pending_autofixup_selection(&mut app, true, LoopAction::Proceed);
        assert!(matches!(result, LoopAction::Proceed));
        assert_eq!(
            app.pending_autofixup_selection, None,
            "batch abandoned; don't leak into a later, unrelated reload"
        );
    }
}
