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

//! A `GitRepo` stub shared by the terminal-free unit tests of the binary
//! crate's own modules — dispatch handlers and startup recovery. Everything it
//! answers is configurable per test; anything a test does not reach is left
//! `unimplemented!()` so a new caller shows up as a panic rather than as a
//! silently plausible default.

use git_tailor::Oid;
use git_tailor::app::SquashMode;
use git_tailor::repo::{ConflictState, RebaseOutcome, RepoRead, RepoWrite, SquashContext};
use git_tailor::{CommitDiff, CommitInfo};

/// Minimal `GitRepo` stub for testing terminal-free binary-crate helpers.
pub(crate) struct MockRepo {
    pub(crate) head_ok: bool,
    pub(crate) drop_ok: bool,
    pub(crate) move_ok: bool,
    pub(crate) autofixup_ok: bool,
    pub(crate) autofixup_conflicts: bool,
    pub(crate) abort_ok: bool,
    pub(crate) autostash_restore_ok: bool,
    pub(crate) count_per_file: usize,
    pub(crate) count_ok: bool,
    pub(crate) stage_ok: bool,
    pub(crate) stage_changed: bool,
    pub(crate) undo_skips_autostash: bool,
    pub(crate) redo_skips_autostash: bool,
    /// Counts `autostash_save` invocations so tests can assert the working-tree-
    /// preserving undo/redo paths skip the stash dance.
    pub(crate) autostash_save_calls: std::cell::Cell<usize>,
    /// Configurable `commit_diff` result, for `handle_prepare_split_out_hunks` tests.
    pub(crate) commit_diff: Option<CommitDiff>,
    /// Files reported by `read_conflicting_files`, for the conflict-tool tests.
    pub(crate) conflicting_files: Vec<String>,
    /// What `lift_worktree_row` answers: `Ok(None)` for an empty row, an
    /// error, or a lifted row to build the fold on.
    pub(crate) lift: LiftOutcome,
    /// Whether `restore_lifted_row` succeeds. A failed unwind leaves the
    /// temporary commit on the branch, which the caller has to report.
    pub(crate) restore_lifted_ok: bool,
    /// Counts `restore_lifted_row` invocations.
    pub(crate) restore_lifted_calls: std::cell::Cell<usize>,
    /// What `rescue_lifted_row` answers: the ref it kept the working tree under,
    /// or `None` for a record with nothing worth keeping.
    pub(crate) rescued_ref: Option<String>,
    /// What `read_journal` answers, for the startup-recovery tests.
    pub(crate) journal: Option<git_tailor::repo::InProgress>,
    /// Counts `clear_journal` invocations, so a test can tell a discarded
    /// journal from a recovered one.
    pub(crate) clear_journal_calls: std::cell::Cell<usize>,
    /// Whether `abort_edit` succeeds.
    pub(crate) abort_edit_ok: bool,
    /// What `squash_try_combine` answers: no conflict, a conflict, or an error.
    pub(crate) squash_probe: SquashProbe,
    /// The message `squash_try_combine` was last given, so a test can check what
    /// a squash would seed its editor with.
    pub(crate) squash_probe_message: std::cell::RefCell<Option<String>>,
}

/// What [`MockRepo::lift_worktree_row`] answers.
#[derive(Default)]
pub(crate) enum LiftOutcome {
    /// The row has nothing to fold in.
    #[default]
    Empty,
    /// The row could not be lifted.
    Error,
    /// The row was lifted into a temporary commit.
    Lifted,
}

/// What [`MockRepo::squash_try_combine`] answers.
#[derive(Default)]
pub(crate) enum SquashProbe {
    #[default]
    Clean,
    Conflict,
    Error,
}

/// A conflict state for tests that need one.
pub(crate) fn make_conflict_state() -> ConflictState {
    ConflictState {
        operation_label: "Drop".to_string(),
        original_branch_oid: Oid::from("b".repeat(40)),
        new_tip_oid: Oid::from("c".repeat(40)),
        conflicting_commit_oid: Oid::from("d".repeat(40)),
        conflicting_files: vec![],
        ..Default::default()
    }
}

/// The temporary commit [`LiftOutcome::Lifted`] pretends to have made.
pub(crate) fn mock_temp_oid() -> Oid {
    Oid::from("c".repeat(40))
}

/// The lifted row [`LiftOutcome::Lifted`] hands back.
pub(crate) fn mock_lifted_row() -> git_tailor::repo::LiftedRow {
    git_tailor::repo::LiftedRow {
        source: git_tailor::repo::WorktreeSource::Staged,
        tip_before: Oid::from("a".repeat(40)),
        index_tree_before: Oid::from("d".repeat(40)),
        worktree_tree: Oid::from("e".repeat(40)),
        source_tree: Oid::from("f".repeat(40)),
        temp_oid: mock_temp_oid(),
    }
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
            lift: LiftOutcome::default(),
            restore_lifted_ok: true,
            restore_lifted_calls: std::cell::Cell::new(0),
            rescued_ref: None,
            journal: None,
            clear_journal_calls: std::cell::Cell::new(0),
            abort_edit_ok: true,
            squash_probe: SquashProbe::default(),
            squash_probe_message: std::cell::RefCell::new(None),
        }
    }
}

pub(crate) fn mock_stage_outcome(
    ok: bool,
    changed: bool,
) -> anyhow::Result<git_tailor::repo::StageOutcome> {
    if !ok {
        // Mirrors the real `stage_op` shape: a libgit2 cause carrying the useful
        // detail, wrapped in an `anyhow` context that alone says nothing useful.
        return Err(
            anyhow::anyhow!("invalid path: 'nul'").context("failed to stage working-tree changes")
        );
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

impl RepoWrite for MockRepo {
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
        if self.abort_edit_ok {
            Ok(())
        } else {
            Err(anyhow::anyhow!("abort edit failed"))
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
        Ok(match &self.journal {
            Some(record) => git_tailor::repo::JournalStatus::Recovered(Box::new(record.clone())),
            None => git_tailor::repo::JournalStatus::None,
        })
    }
    fn clear_journal(&self) -> anyhow::Result<()> {
        self.clear_journal_calls
            .set(self.clear_journal_calls.get() + 1);
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
    fn lift_worktree_row(
        &self,
        _: git_tailor::repo::WorktreeSource,
    ) -> anyhow::Result<Option<git_tailor::repo::LiftedRow>> {
        match self.lift {
            LiftOutcome::Empty => Ok(None),
            // Mirrors the real shape: a libgit2 cause under an anyhow context.
            LiftOutcome::Error => Err(anyhow::anyhow!("the index has conflicts")
                .context("failed to lift the working-tree changes")),
            LiftOutcome::Lifted => Ok(Some(mock_lifted_row())),
        }
    }
    fn restore_lifted_row(&self, _: &git_tailor::repo::LiftedRow) -> anyhow::Result<()> {
        self.restore_lifted_calls
            .set(self.restore_lifted_calls.get() + 1);
        if self.restore_lifted_ok {
            Ok(())
        } else {
            Err(anyhow::anyhow!("ref is locked").context("failed to move the branch back"))
        }
    }
    fn rescue_lifted_row(&self, _: &git_tailor::repo::LiftedRow) -> anyhow::Result<Option<String>> {
        Ok(self.rescued_ref.clone())
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
        message: &str,
        _: SquashMode,
        _: &Oid,
    ) -> anyhow::Result<Option<ConflictState>> {
        *self.squash_probe_message.borrow_mut() = Some(message.to_string());
        match self.squash_probe {
            SquashProbe::Clean => Ok(None),
            SquashProbe::Conflict => Ok(Some(make_conflict_state())),
            SquashProbe::Error => {
                Err(anyhow::anyhow!("tree is unmergeable")
                    .context("failed to combine the two commits"))
            }
        }
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
