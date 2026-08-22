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

use crate::{
    CommitInfo, Oid, VirtualOid,
    app::SquashMode,
    fragmap::FragMap,
    repo::{ConflictState, StashConflictState},
    views::{palette::Colors, theme::Theme},
};

use super::{AppMode, Operation, PendingAutofixup, PendingDrop, PendingSplit, SplitStrategy};
use crate::app::ScrollState;
use crate::app::commit_list::CommitListState;
use crate::app::detail::DetailState;
use crate::app::search::SearchState;
use crate::autofixup::AutofixupPair;

/// Transient status message shown in the footer, cleared on the next keypress.
#[derive(Debug, Default)]
pub struct StatusState {
    pub message: Option<String>,
    /// Whether the message represents an error (red) or a success (green).
    pub is_error: bool,
}

impl StatusState {
    /// Set a success status message (shown with green background).
    pub fn set_success(&mut self, msg: impl Into<String>) {
        self.message = Some(msg.into());
        self.is_error = false;
    }

    /// Set an error status message (shown with red background).
    pub fn set_error(&mut self, msg: impl Into<String>) {
        self.message = Some(msg.into());
        self.is_error = true;
    }

    /// Clear the transient status message.
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

/// Application state for the TUI.
///
/// Manages the overall state of the interactive terminal interface,
/// including quit flag and commit list state.
#[derive(Default)]
pub struct AppState {
    pub should_quit: bool,
    /// The commit list, its selection, and its scroll position.
    pub list: CommitListState,
    /// Show all hunk-group columns without deduplication (--full flag).
    pub full_fragmap: bool,
    /// Active fragmap rendering theme.
    pub theme: Theme,
    /// Active color palette (--palette). Terminal (default) keeps the terminal's
    /// own colors; a fixed scheme resolves them to specific RGB.
    pub colors: Colors,
    /// The reference OID (merge-base) used when the session started.
    /// Stored here so 'u' update can rescan from HEAD down to the same base.
    pub reference_oid: Oid,
    /// Optional fragmap visualization data.
    /// None if fragmap computation failed or was not performed.
    pub fragmap: Option<FragMap>,
    /// Horizontal scroll offset for the fragmap grid.
    pub fragmap_scroll_offset: usize,
    /// Current display mode.
    pub mode: AppMode,
    /// Scroll position, diff context and file offsets of the detail view.
    pub detail: DetailState,
    /// Transient status message shown in the footer (cleared on next keypress).
    pub status: StatusState,
    /// User-controlled offset for the vertical separator bar (positive = right, negative = left).
    pub separator_offset: i16,
    /// Scroll state for the current dialog (e.g. help). Offset is reset when a
    /// dialog opens; bounds are updated during render.
    pub dialog: ScrollState,
    /// When true, the reference_oid commit is included in the commit list.
    /// Set when the user passes `--all` to browse the complete repository history.
    pub include_reference_oid: bool,
    /// Regex search state for the detail view.
    pub search: SearchState,
    /// Set when the background check detects a newer crates.io release. Persistent
    /// (NOT cleared by `status.clear`); shown in the footer hint slot.
    pub update_notice: Option<String>,
}

impl AppState {
    /// Create a new AppState with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new AppState with the given commits, selecting the last one (HEAD).
    pub fn with_commits(commits: Vec<CommitInfo>) -> Self {
        let selection_index = commits.len().saturating_sub(1);
        Self {
            list: CommitListState {
                commits,
                selection_index,
                ..Default::default()
            },
            ..Self::default()
        }
    }

    /// Scroll fragmap grid left.
    pub fn scroll_fragmap_left(&mut self) {
        if self.fragmap_scroll_offset > 0 {
            self.fragmap_scroll_offset -= 1;
        }
    }

    /// Scroll fragmap grid right.
    pub fn scroll_fragmap_right(&mut self) {
        self.fragmap_scroll_offset += 1;
    }

    /// Scroll fragmap grid to the leftmost column.
    pub fn scroll_fragmap_to_left(&mut self) {
        self.fragmap_scroll_offset = 0;
    }

    /// Scroll fragmap grid to the rightmost column (render will clamp).
    pub fn scroll_fragmap_to_right(&mut self) {
        self.fragmap_scroll_offset = usize::MAX;
    }

    /// Enter the large-split confirmation dialog.
    pub fn enter_split_confirm(
        &mut self,
        strategy: SplitStrategy,
        commit_oid: Oid,
        head_oid: Oid,
        count: usize,
    ) {
        self.enter_dialog(AppMode::SplitConfirm(PendingSplit {
            strategy,
            commit_oid,
            head_oid,
            count,
        }));
    }

    /// Cancel the large-split confirmation and return to CommitList.
    pub fn cancel_split_confirm(&mut self) {
        self.exit_dialog();
    }

    /// Enter the drop confirmation dialog.
    pub fn enter_drop_confirm(&mut self, commit_oid: Oid, commit_summary: String, head_oid: Oid) {
        self.enter_dialog(AppMode::DropConfirm(PendingDrop {
            commit_oid,
            commit_summary,
            head_oid,
        }));
    }

    /// Cancel the drop confirmation and return to CommitList.
    pub fn cancel_drop_confirm(&mut self) {
        self.exit_dialog();
    }

    /// Enter the autofixup confirmation dialog.
    pub fn enter_autofixup_confirm(
        &mut self,
        pairs: Vec<AutofixupPair>,
        head_oid: Oid,
        reference_oid: Oid,
    ) {
        self.enter_dialog(AppMode::AutofixupConfirm(PendingAutofixup {
            pairs,
            head_oid,
            reference_oid,
            selected_group: 0,
            message_overrides: std::collections::HashMap::new(),
        }));
    }

    /// Cancel the autofixup confirmation and return to CommitList.
    pub fn cancel_autofixup_confirm(&mut self) {
        self.exit_dialog();
    }

    /// Enter the rebase-conflict resolution dialog.
    pub fn enter_rebase_conflict(&mut self, state: ConflictState) {
        self.enter_dialog(AppMode::RebaseConflict(Box::new(state)));
    }

    /// Enter the auto-stash conflict resolution dialog.
    pub fn enter_stash_conflict(&mut self, state: StashConflictState) {
        self.enter_dialog(AppMode::StashConflict(Box::new(state)));
    }

    /// Enter the startup crash-recovery prompt for an interrupted operation.
    pub fn enter_recover_confirm(&mut self, state: ConflictState) {
        self.enter_dialog(AppMode::RecoverConfirm(Box::new(state)));
    }

    /// Returns the selected commit if it is a real (non-synthetic) commit.
    /// Sets an error message and returns `None` for staged/unstaged rows.
    pub fn selected_real_commit(&mut self, action: &str) -> Option<&CommitInfo> {
        if self.list.selected().is_some_and(|c| c.oid.is_synthetic()) {
            self.status
                .set_error(format!("Cannot {action} staged/unstaged changes"));
            return None;
        }
        self.list.selected()
    }

    /// Whether the selected row is the synthetic `want` row (`Staged` /
    /// `Unstaged`). Otherwise sets a guiding hint naming the row to select for
    /// `action` (e.g. "stage all changes") and returns `false`.
    pub fn selected_synthetic_row_is(&mut self, want: VirtualOid, action: &str) -> bool {
        if self.list.selected_virtual_oid() == Some(&want) {
            return true;
        }
        let row = match want {
            VirtualOid::Staged => "Staged",
            VirtualOid::Unstaged => "Unstaged",
            VirtualOid::Real(_) => "",
        };
        self.status
            .set_error(format!("Select the \"{row}\" row to {action}"));
        false
    }

    /// Enter the operation picker for the selected row, highlighting the first
    /// available operation. Every real/synthetic row offers at least undo/redo,
    /// so this only no-ops when there is no selection at all.
    pub fn enter_operation_select(&mut self) {
        let is_oldest = self.list.selected_is_oldest_commit();
        let first = self
            .list
            .selected_virtual_oid()
            .map(|oid| Operation::available_for(oid, is_oldest))
            .and_then(|ops| ops.into_iter().next());
        if let Some(operation) = first {
            self.enter_dialog(AppMode::OperationSelect { operation });
        }
    }

    /// Enter split strategy selection mode.
    /// Only allowed for real commits (not staged/unstaged synthetic rows).
    pub fn enter_split_select(&mut self) {
        if self.selected_real_commit("split").is_none() {
            return;
        }
        self.enter_dialog(AppMode::SplitSelect { strategy_index: 0 });
    }

    /// Enter the "split out file(s)" picker with the commit's changed files.
    pub fn enter_split_files_select(&mut self, commit_oid: Oid, files: Vec<crate::FileDiff>) {
        self.enter_dialog(AppMode::SplitFilesSelect {
            commit_oid,
            files,
            file_index: 0,
            selected: std::collections::HashSet::new(),
            preview_h_scroll: 0,
            preview_v_scroll: 0,
        });
    }

    /// Cancel the "split out file(s)" picker and return to CommitList.
    pub fn cancel_split_files_select(&mut self) {
        self.exit_dialog();
    }

    /// Enter the "split out hunk(s)" picker with the commit's hunks.
    pub fn enter_split_hunks_select(
        &mut self,
        commit_oid: Oid,
        hunks: Vec<crate::app::HunkPickerEntry>,
        context_lines: u32,
    ) {
        self.enter_dialog(AppMode::SplitHunksSelect {
            commit_oid,
            hunks,
            hunk_index: 0,
            selected: std::collections::HashSet::new(),
            context_lines,
            preview_h_scroll: 0,
            preview_v_scroll: 0,
        });
    }

    /// Cancel the "split out hunk(s)" picker and return to CommitList.
    pub fn cancel_split_hunks_select(&mut self) {
        self.exit_dialog();
    }

    /// Enter squash target selection mode.
    /// Only allowed for real commits (not staged/unstaged synthetic rows).
    pub fn enter_squash_select(&mut self) {
        self.enter_squash_or_fixup_select(SquashMode::Squash);
    }

    /// Enter fixup target selection mode (same UI as squash, keeps target msg).
    pub fn enter_fixup_select(&mut self) {
        self.enter_squash_or_fixup_select(SquashMode::Fixup);
    }

    fn enter_squash_or_fixup_select(&mut self, squash_mode: SquashMode) {
        let label = squash_mode.label().to_lowercase();
        let Some(selected) = self.list.selected_virtual_oid() else {
            return;
        };
        // A working-tree row is a valid source: it gets folded into a target
        // without ever becoming a commit of its own. That also means it needs
        // only one commit to fold into, where a commit source needs another
        // besides itself.
        let from_worktree = selected.is_synthetic();
        let needed = if from_worktree { 1 } else { 2 };
        if self.list.real_commit_count() < needed {
            self.set_error_message(format!(
                "Nothing to {label} — no earlier commit on the branch"
            ));
            return;
        }
        self.mode = AppMode::SquashSelect {
            source_index: self.list.selection_index,
            squash_mode,
        };
    }

    /// Cancel squash selection and return to CommitList.
    pub fn cancel_squash_select(&mut self) {
        self.exit_dialog();
    }

    /// Enter move commit selection mode.
    /// The insertion cursor starts one position before the source (i.e. one
    /// slot earlier in the commit list, which visually means "above" in
    /// chronological order).
    pub fn enter_move_select(&mut self) {
        if self.selected_real_commit("move").is_none() {
            return;
        }

        // Moving requires at least 2 real (non-synthetic) commits.
        if self.list.real_commit_count() < 2 {
            self.set_error_message("Nothing to move — only one commit on the branch");
            return;
        }

        let source = self.list.selection_index;
        let max = self.list.commits.len();
        // Pick the first valid (non-no-op) position. No-ops are source and
        // source + 1, so try source - 1 first, then scan forward.
        let insert_before = if source > 0 {
            source - 1
        } else {
            // source == 0 → first valid position is 2 (skip 0 and 1)
            2.min(max)
        };
        self.mode = AppMode::MoveSelect {
            source_index: source,
            insert_before,
        };
    }

    /// Cancel move selection and return to CommitList.
    pub fn cancel_move_select(&mut self) {
        self.exit_dialog();
    }

    /// Set a success status message (shown with green background).
    pub fn set_success_message(&mut self, msg: impl Into<String>) {
        self.status.set_success(msg);
    }

    /// Set an error status message (shown with red background).
    pub fn set_error_message(&mut self, msg: impl Into<String>) {
        self.status.set_error(msg);
    }

    /// Clear the transient status message.
    pub fn clear_status_message(&mut self) {
        self.status.clear();
    }

    /// Toggle between CommitList and CommitDetail modes.
    pub fn toggle_detail_view(&mut self) {
        let new_mode = match &self.mode {
            AppMode::CommitList => AppMode::CommitDetail,
            AppMode::CommitDetail => {
                self.search.clear();
                AppMode::CommitList
            }
            AppMode::Help(_)
            | AppMode::OperationSelect { .. }
            | AppMode::SplitSelect { .. }
            | AppMode::SplitFilesSelect { .. }
            | AppMode::SplitHunksSelect { .. }
            | AppMode::SplitConfirm(_)
            | AppMode::DropConfirm(_)
            | AppMode::AutofixupConfirm(_)
            | AppMode::RebaseConflict(_)
            | AppMode::StashConflict(_)
            | AppMode::RecoverConfirm(_)
            | AppMode::SquashSelect { .. }
            | AppMode::MoveSelect { .. }
            | AppMode::Loading { .. } => return,
        };
        self.mode = new_mode;
        self.detail.v.offset = 0;
    }

    /// Show help dialog, saving current mode to return to later.
    pub fn show_help(&mut self) {
        if !matches!(self.mode, AppMode::Help(_)) {
            let current = std::mem::replace(&mut self.mode, AppMode::CommitList);
            self.mode = AppMode::Help(Box::new(current));
            self.dialog.offset = 0;
        }
    }

    /// Close help dialog and return to previous mode.
    pub fn close_help(&mut self) {
        if matches!(self.mode, AppMode::Help(_)) {
            let prev = std::mem::replace(&mut self.mode, AppMode::CommitList);
            if let AppMode::Help(prev_mode) = prev {
                self.mode = *prev_mode;
            }
        }
    }

    /// Toggle help dialog on/off.
    pub fn toggle_help(&mut self) {
        if matches!(self.mode, AppMode::Help(_)) {
            self.close_help();
        } else {
            self.show_help();
        }
    }

    fn enter_dialog(&mut self, mode: AppMode) {
        self.mode = mode;
        self.dialog.offset = 0;
    }

    fn exit_dialog(&mut self) {
        // MoveSelect navigation can leave the selection as a scroll anchor
        // pointing past the last commit; clamp it so CommitList consumers
        // (footer, fragmap highlight) never index out of bounds.
        self.list.clamp_selection();
        self.mode = AppMode::CommitList;
    }
}
