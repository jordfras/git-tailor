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

// TUI application state management

pub mod detail;
pub mod keymap;
pub mod operation;
pub mod scroll;
pub mod search;
pub mod state;

pub use detail::{DetailContextLines, DetailState};
pub use keymap::{KeyCommand, read_event};
pub use operation::Operation;
pub use scroll::ScrollState;
pub use search::SearchState;
pub use state::AppState;

use crate::{
    FileDiff, Oid,
    autofixup::AutofixupPair,
    repo::{ConflictState, StashConflictState},
};

/// Result of a view module's `handle_key` function.
///
/// Pure state mutations (scroll, selection, mode transitions) are applied
/// directly to `AppState` inside the handler. Side effects that require git
/// operations or terminal access are returned here for `main.rs` to execute.
#[derive(Debug)]
pub enum AppAction {
    /// Fully handled, no side effects needed.
    Handled,
    /// The application should quit.
    Quit,
    /// Reload commits from the repository.
    ReloadCommits,
    /// Begin the split flow: get head_oid, count results, confirm if large.
    PrepareSplit {
        strategy: SplitStrategy,
        commit_oid: Oid,
    },
    /// Execute a split that has already been confirmed.
    ExecuteSplit {
        strategy: SplitStrategy,
        commit_oid: Oid,
        head_oid: Oid,
    },
    /// Open the file picker for the "split out file(s)" strategy: load the
    /// commit's diff, then show the second dialog.
    PrepareSplitOutFiles { commit_oid: Oid },
    /// Execute a "split out file(s)" rewrite: peel `file_paths` into a
    /// follow-up commit together.
    ExecuteSplitOutFiles {
        commit_oid: Oid,
        file_paths: Vec<String>,
    },
    /// Open (or refresh, after a `+`/`-` context change) the hunk picker for
    /// the "split out hunk(s)" strategy: load the commit's diff at
    /// `context_lines` and show the dialog.
    PrepareSplitOutHunks { commit_oid: Oid, context_lines: u32 },
    /// Execute a "split out hunks" rewrite: peel the selected hunks into
    /// their own commit. `context_lines` is the diff context `hunks`' indices
    /// were derived at, so the backend can rebuild the same diff.
    ExecuteSplitOutHunks {
        commit_oid: Oid,
        hunks: Vec<(usize, usize)>,
        context_lines: u32,
    },
    /// Begin the drop flow: get head_oid from repo, then show confirmation.
    PrepareDropConfirm {
        commit_oid: Oid,
        commit_summary: String,
    },
    /// Execute a confirmed drop.
    ExecuteDrop { commit_oid: Oid, head_oid: Oid },
    /// Edit a commit in a shell: check it out, suspend the TUI, spawn `$SHELL`,
    /// then splice the resulting chain back in. `commit_summary` is shown in the
    /// pre-suspend instruction banner.
    ExecuteEdit {
        commit_oid: Oid,
        commit_summary: String,
    },
    /// Continue a rebase after the user resolved merge conflicts.
    RebaseContinue(ConflictState),
    /// Abort a rebase that hit conflicts.
    RebaseAbort(ConflictState),
    /// Launch the merge tool for conflicting files.
    RunMergetool {
        files: Vec<String>,
        conflict_state: ConflictState,
    },
    /// Open conflicting files in the configured editor.
    RunEditor {
        files: Vec<String>,
        conflict_state: ConflictState,
    },
    /// Finish a conflicting auto-stash reapply (drop the stash if resolved).
    AutostashContinue,
    /// Abort a conflicting auto-stash reapply, rewinding the whole operation.
    AutostashAbort,
    /// Launch the merge tool for files conflicting in an auto-stash reapply.
    RunMergetoolForStash { files: Vec<String> },
    /// Open auto-stash conflicting files in the configured editor.
    RunEditorForStash { files: Vec<String> },
    /// Start the reword flow: get head_oid, launch editor, rewrite commit.
    PrepareReword {
        commit_oid: Oid,
        current_message: String,
    },
    /// Start the squash/fixup flow: user picked source and target.
    PrepareSquash {
        source_oid: Oid,
        target_oid: Oid,
        source_message: String,
        target_message: String,
        squash_mode: SquashMode,
    },
    /// Stage all working-tree changes (`git add -A`).
    StageAll,
    /// Unstage all staged changes (reset the index to HEAD).
    UnstageAll,
    /// Start the commit-staged flow: launch the editor, then commit the index.
    PrepareCommitStaged,
    /// Undo the most recent history-rewriting operation.
    Undo,
    /// Redo the most recently undone operation.
    Redo,
    /// Execute a confirmed move: reorder the source commit to after insert_after_oid.
    ExecuteMove {
        source_oid: Oid,
        insert_after_oid: Option<Oid>,
    },
    /// Begin the autofixup flow: get head_oid, compute the pair plan from the
    /// already-loaded commits, then show the confirmation dialog.
    PrepareAutofixupConfirm,
    /// Open `$EDITOR` on `template` to edit the final message for the target
    /// group identified by `target_summary` (its original, stable identity).
    PrepareAutofixupEditMessage {
        target_summary: String,
        template: String,
    },
    /// Execute a confirmed autofixup batch. `pairs` is the plan shown in the
    /// confirmation dialog, reused after completion to work out where the
    /// cursor should land (see `main.rs::autofixup_target_selection_index`).
    /// `message_overrides` carries any per-target messages the user edited
    /// before confirming, keyed by the target's original summary text.
    ExecuteAutofixup {
        head_oid: Oid,
        reference_oid: Oid,
        pairs: Vec<AutofixupPair>,
        message_overrides: std::collections::HashMap<String, String>,
    },
}

/// Split strategy options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitStrategy {
    PerFile,
    OutFiles,
    PerHunk,
    PerHunkGroup,
    OutHunks,
}

impl SplitStrategy {
    pub const ALL: [SplitStrategy; 5] = [
        SplitStrategy::PerFile,
        SplitStrategy::OutFiles,
        SplitStrategy::PerHunk,
        SplitStrategy::PerHunkGroup,
        SplitStrategy::OutHunks,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SplitStrategy::PerFile => "Per file",
            SplitStrategy::OutFiles => "Split out file(s)",
            SplitStrategy::PerHunk => "Per hunk",
            SplitStrategy::PerHunkGroup => "Per hunk group",
            SplitStrategy::OutHunks => "Split out hunk(s)",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            SplitStrategy::PerFile => "Create one commit per changed file",
            SplitStrategy::OutFiles => "Peel selected files into a commit",
            SplitStrategy::PerHunk => "Create one commit per diff hunk",
            SplitStrategy::PerHunkGroup => "Create one commit per hunk group",
            SplitStrategy::OutHunks => "Peel selected hunks into a commit",
        }
    }
}

/// Whether a squash operation keeps the target message (fixup) or combines both (squash).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum SquashMode {
    #[default]
    Squash,
    Fixup,
}

impl SquashMode {
    pub fn label(self) -> &'static str {
        match self {
            SquashMode::Squash => "Squash",
            SquashMode::Fixup => "Fixup",
        }
    }

    pub fn keeps_target_message(self) -> bool {
        match self {
            SquashMode::Squash => false,
            SquashMode::Fixup => true,
        }
    }
}

/// Application display mode.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum AppMode {
    /// Loading screen shown while commits are streamed and the fragmap is built.
    Loading {
        title: &'static str,
        message: &'static str,
        /// `Some((done, total))` when progress is known, `None` for indeterminate phases.
        progress: Option<(usize, usize)>,
        /// When `true`, pressing `s` skips the current phase. When `false`,
        /// Ctrl-C is the quit key and is shown as the hint instead.
        skippable: bool,
    },
    /// Commit list view with fragmap.
    #[default]
    CommitList,
    /// Detailed view of a single commit.
    CommitDetail,
    /// Operation picker dialog; lists the operations valid for the selected row
    /// and carries the highlighted operation. The menu is dynamic (filtered by
    /// row), so the highlighted choice is stored as the `Operation` itself
    /// rather than an index into a list that is recomputed each frame.
    OperationSelect { operation: Operation },
    /// Split strategy selection dialog; carries the highlighted option index.
    SplitSelect { strategy_index: usize },
    /// File picker for the "split out file(s)" strategy: a wide two-pane
    /// dialog listing the commit's changed files on the left (with a diff
    /// preview of the highlighted one on the right), letting the user
    /// toggle-select one or more before splitting them out together.
    SplitFilesSelect {
        commit_oid: Oid,
        files: Vec<FileDiff>,
        file_index: usize,
        selected: std::collections::HashSet<usize>,
        /// Horizontal scroll offset for the diff preview pane, reset whenever
        /// the highlighted file changes.
        preview_h_scroll: usize,
        /// Vertical scroll offset for the diff preview pane (`Ctrl-↑`/`Ctrl-↓`
        /// — plain arrows already move the file list cursor). Reset alongside
        /// `preview_h_scroll` when the highlighted file changes.
        preview_v_scroll: usize,
    },
    /// Hunk picker for the "split out hunk(s)" strategy: a wide two-pane
    /// dialog listing the commit's hunks on the left (with a diff preview of
    /// the highlighted one on the right), letting the user toggle-select one
    /// or more before splitting them out together.
    SplitHunksSelect {
        commit_oid: Oid,
        hunks: Vec<HunkPickerEntry>,
        hunk_index: usize,
        selected: std::collections::HashSet<usize>,
        /// Diff context level `hunks` was loaded at, adjustable with `+`/`-`
        /// like the commit detail view. Re-fetching at a new level rebuilds
        /// `hunks` from scratch and resets the selection (indices may no
        /// longer mean the same thing once hunks merge or split apart).
        context_lines: u32,
        /// Horizontal scroll offset for the diff preview pane, reset whenever
        /// the highlighted hunk changes (each hunk's content is unrelated to
        /// the last, so starting left-aligned avoids a confusingly clipped
        /// view carried over from wherever the previous hunk was scrolled to).
        preview_h_scroll: usize,
        /// Vertical scroll offset for the diff preview pane (`Ctrl-↑`/`Ctrl-↓`
        /// — plain arrows already move the hunk list cursor). Reset alongside
        /// `preview_h_scroll` when the highlighted hunk changes.
        preview_v_scroll: usize,
    },
    /// Confirmation dialog for large splits (> SPLIT_CONFIRM_THRESHOLD commits).
    SplitConfirm(PendingSplit),
    /// Confirmation dialog before dropping a commit.
    DropConfirm(PendingDrop),
    /// Confirmation dialog before running a bulk autofixup batch.
    AutofixupConfirm(PendingAutofixup),
    /// Waiting for the user to resolve merge conflicts that arose during a
    /// rebase operation. Enter continues, Esc aborts the entire operation.
    RebaseConflict(Box<ConflictState>),
    /// Waiting for the user to resolve conflicts left by reapplying the
    /// auto-stash after an operation completed. Enter finishes (drops the
    /// stash), Esc aborts the entire operation back to the pre-operation state.
    StashConflict(Box<StashConflictState>),
    /// Startup prompt offering to recover an operation that a previous run was
    /// killed in the middle of (detected from the persisted journal). Enter
    /// resumes (enters `RebaseConflict`), Esc aborts back to the original tip.
    RecoverConfirm(Box<ConflictState>),
    /// Squash/fixup target selection: user picks which commit to squash the source into.
    SquashSelect {
        source_index: usize,
        squash_mode: SquashMode,
    },
    /// Move commit selection: user picks where to insert the source commit.
    /// `insert_before` is the index in the commit list where the separator row
    /// appears; the commit will be moved to that position.
    MoveSelect {
        source_index: usize,
        insert_before: usize,
    },
    /// Help dialog overlay; carries the mode to return to when closed.
    Help(Box<AppMode>),
}

impl AppMode {
    /// For overlay modes, return the base view that should be rendered
    /// underneath. Returns `None` for base views (CommitList, CommitDetail).
    pub fn background(&self) -> Option<AppMode> {
        match self {
            AppMode::Loading { .. } | AppMode::CommitList | AppMode::CommitDetail => None,
            AppMode::SquashSelect { .. } | AppMode::MoveSelect { .. } => None,
            AppMode::OperationSelect { .. }
            | AppMode::SplitSelect { .. }
            | AppMode::SplitFilesSelect { .. }
            | AppMode::SplitHunksSelect { .. }
            | AppMode::SplitConfirm(_)
            | AppMode::DropConfirm(_)
            | AppMode::AutofixupConfirm(_)
            | AppMode::RebaseConflict(_)
            | AppMode::StashConflict(_)
            | AppMode::RecoverConfirm(_) => Some(AppMode::CommitList),
            AppMode::Help(prev) => Some(prev.as_ref().clone()),
        }
    }
}

/// One row of the "split out hunk(s)" picker: a hunk's identity for the
/// backend (`delta_idx`/`hunk_idx`, against the diff loaded at
/// `repo::DEFAULT_CONTEXT_LINES`), its file path for the list label, and its
/// own content for the preview pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HunkPickerEntry {
    pub delta_idx: usize,
    pub hunk_idx: usize,
    pub file_path: String,
    pub hunk: crate::Hunk,
}

/// Data retained while the user is shown the large-split confirmation dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingSplit {
    pub strategy: SplitStrategy,
    pub commit_oid: Oid,
    pub head_oid: Oid,
    pub count: usize,
}

/// Data retained while the user is shown the drop confirmation dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingDrop {
    pub commit_oid: Oid,
    pub commit_summary: String,
    pub head_oid: Oid,
}

/// Data retained while the user is shown the autofixup confirmation dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingAutofixup {
    pub pairs: Vec<AutofixupPair>,
    pub head_oid: Oid,
    pub reference_oid: Oid,
    /// Index into `group_by_target(&pairs)` of the highlighted target group.
    pub selected_group: usize,
    /// User-edited final messages, keyed by the target's original summary
    /// text (see `AutofixupContext::message_overrides`).
    pub message_overrides: std::collections::HashMap<String, String>,
}
