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

pub mod keymap;
pub mod state;

pub use keymap::{KeyCommand, read_event};
pub use state::AppState;

use crate::{Oid, repo::ConflictState};

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
    /// Begin the drop flow: get head_oid from repo, then show confirmation.
    PrepareDropConfirm {
        commit_oid: Oid,
        commit_summary: String,
    },
    /// Execute a confirmed drop.
    ExecuteDrop { commit_oid: Oid, head_oid: Oid },
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
    /// Execute a confirmed move: reorder the source commit to after insert_after_oid.
    ExecuteMove {
        source_oid: Oid,
        insert_after_oid: Option<Oid>,
    },
}

/// Split strategy options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitStrategy {
    PerFile,
    PerHunk,
    PerHunkGroup,
}

impl SplitStrategy {
    pub const ALL: [SplitStrategy; 3] = [
        SplitStrategy::PerFile,
        SplitStrategy::PerHunk,
        SplitStrategy::PerHunkGroup,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SplitStrategy::PerFile => "Per file",
            SplitStrategy::PerHunk => "Per hunk",
            SplitStrategy::PerHunkGroup => "Per hunk group",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            SplitStrategy::PerFile => "Create one commit per changed file",
            SplitStrategy::PerHunk => "Create one commit per diff hunk",
            SplitStrategy::PerHunkGroup => "Create one commit per hunk group",
        }
    }
}

/// Whether a squash operation keeps the target message (fixup) or combines both (squash).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SquashMode {
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
    /// Commit list view with fragmap.
    #[default]
    CommitList,
    /// Detailed view of a single commit.
    CommitDetail,
    /// Split strategy selection dialog; carries the highlighted option index.
    SplitSelect { strategy_index: usize },
    /// Confirmation dialog for large splits (> SPLIT_CONFIRM_THRESHOLD commits).
    SplitConfirm(PendingSplit),
    /// Confirmation dialog before dropping a commit.
    DropConfirm(PendingDrop),
    /// Waiting for the user to resolve merge conflicts that arose during a
    /// rebase operation. Enter continues, Esc aborts the entire operation.
    RebaseConflict(Box<ConflictState>),
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
            AppMode::CommitList | AppMode::CommitDetail => None,
            AppMode::SquashSelect { .. } | AppMode::MoveSelect { .. } => None,
            AppMode::SplitSelect { .. }
            | AppMode::SplitConfirm(_)
            | AppMode::DropConfirm(_)
            | AppMode::RebaseConflict(_) => Some(AppMode::CommitList),
            AppMode::Help(prev) => Some(prev.as_ref().clone()),
        }
    }
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
