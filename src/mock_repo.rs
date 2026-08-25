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
    fn begin_worktree_source(
        &self,
        _: git_tailor::repo::WorktreeSource,
    ) -> anyhow::Result<Option<git_tailor::repo::WorktreeSourceSnapshot>> {
        unimplemented!()
    }
    fn abort_worktree_source(
        &self,
        _: &git_tailor::repo::WorktreeSourceSnapshot,
    ) -> anyhow::Result<()> {
        unimplemented!()
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
