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

// Operations offered by the operation picker (`AppMode::OperationSelect`).

use crate::VirtualOid;
use crate::app::KeyCommand;

/// A commit/working-tree operation offered by the operation picker
/// (`AppMode::OperationSelect`).
///
/// Each variant maps to the [`KeyCommand`] that already triggers it from the
/// commit list, so the picker dispatches through the existing handler rather
/// than duplicating the entry-point logic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Split,
    Squash,
    Fixup,
    Reword,
    Move,
    Drop,
    Edit,
    Stage,
    Unstage,
    Commit,
    Undo,
    Redo,
    Autofixup,
}

impl Operation {
    /// Operations valid for the selected row, in display order. Real commits
    /// get the history-rewriting operations; the synthetic working-tree rows get
    /// their staging operations; undo/redo are always available.
    ///
    /// `is_oldest` marks the oldest commit on the branch, which omits Move —
    /// there is nothing earlier for it to move ahead of.
    pub fn available_for(oid: &VirtualOid, is_oldest: bool) -> Vec<Operation> {
        match oid {
            VirtualOid::Real(_) => {
                let mut ops = vec![
                    Operation::Split,
                    Operation::Squash,
                    Operation::Fixup,
                    Operation::Reword,
                    Operation::Move,
                    Operation::Drop,
                    Operation::Edit,
                    Operation::Autofixup,
                    Operation::Undo,
                    Operation::Redo,
                ];
                if is_oldest {
                    ops.retain(|op| *op != Operation::Move);
                }
                ops
            }
            VirtualOid::Unstaged => vec![
                Operation::Squash,
                Operation::Fixup,
                Operation::Stage,
                Operation::Autofixup,
                Operation::Undo,
                Operation::Redo,
            ],
            VirtualOid::Staged => vec![
                Operation::Squash,
                Operation::Fixup,
                Operation::Commit,
                Operation::Unstage,
                Operation::Autofixup,
                Operation::Undo,
                Operation::Redo,
            ],
        }
    }

    /// The keyboard shortcut for this operation in the commit list, for the
    /// picker to hint so users can learn the direct keys.
    pub fn shortcut(self) -> &'static str {
        match self {
            Operation::Split => "p",
            Operation::Squash => "s",
            Operation::Fixup => "f",
            Operation::Reword => "r",
            Operation::Move => "m",
            Operation::Drop => "d",
            Operation::Edit => "E",
            Operation::Stage => "a",
            Operation::Unstage => "A",
            Operation::Commit => "c",
            Operation::Undo => "u",
            Operation::Redo => "Ctrl-r",
            Operation::Autofixup => "F",
        }
    }

    /// The key command this operation dispatches to in the commit list.
    pub fn key_command(self) -> KeyCommand {
        match self {
            Operation::Split => KeyCommand::Split,
            Operation::Squash => KeyCommand::Squash,
            Operation::Fixup => KeyCommand::Fixup,
            Operation::Reword => KeyCommand::Reword,
            Operation::Move => KeyCommand::Move,
            Operation::Drop => KeyCommand::Drop,
            Operation::Edit => KeyCommand::Edit,
            Operation::Stage => KeyCommand::StageAll,
            Operation::Unstage => KeyCommand::UnstageAll,
            Operation::Commit => KeyCommand::CommitStaged,
            Operation::Undo => KeyCommand::Undo,
            Operation::Redo => KeyCommand::Redo,
            Operation::Autofixup => KeyCommand::Autofixup,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Operation::Split => "Split",
            Operation::Squash => "Squash",
            Operation::Fixup => "Fixup",
            Operation::Reword => "Reword",
            Operation::Move => "Move",
            Operation::Drop => "Drop",
            Operation::Edit => "Edit",
            Operation::Stage => "Stage all",
            Operation::Unstage => "Unstage all",
            Operation::Commit => "Commit staged",
            Operation::Undo => "Undo",
            Operation::Redo => "Redo",
            Operation::Autofixup => "Autofixup",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Operation::Split => "Break into commits",
            Operation::Squash => "Fold into an earlier commit",
            Operation::Fixup => "Fold in, keep its message",
            Operation::Reword => "Edit the message",
            Operation::Move => "Reorder this commit",
            Operation::Drop => "Delete this commit",
            Operation::Edit => "Edit in a shell",
            Operation::Stage => "Stage all tracked changes",
            Operation::Unstage => "Unstage all changes",
            Operation::Commit => "Commit staged changes",
            Operation::Undo => "Undo last operation",
            Operation::Redo => "Redo last operation",
            Operation::Autofixup => "Squash all fixup!/squash!",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Oid;

    #[test]
    fn available_operations_for_a_real_commit() {
        let oid = VirtualOid::Real(Oid::new("a".repeat(40)));
        assert_eq!(
            Operation::available_for(&oid, false),
            vec![
                Operation::Split,
                Operation::Squash,
                Operation::Fixup,
                Operation::Reword,
                Operation::Move,
                Operation::Drop,
                Operation::Edit,
                Operation::Autofixup,
                Operation::Undo,
                Operation::Redo,
            ]
        );
    }

    #[test]
    fn oldest_commit_omits_move() {
        let oid = VirtualOid::Real(Oid::new("a".repeat(40)));
        let ops = Operation::available_for(&oid, true);
        assert!(!ops.contains(&Operation::Move));
        // The other real-commit operations are still offered.
        assert!(ops.contains(&Operation::Split));
        assert!(ops.contains(&Operation::Drop));
    }

    #[test]
    fn available_operations_for_the_unstaged_row() {
        assert_eq!(
            Operation::available_for(&VirtualOid::Unstaged, false),
            vec![
                Operation::Squash,
                Operation::Fixup,
                Operation::Stage,
                Operation::Autofixup,
                Operation::Undo,
                Operation::Redo,
            ]
        );
    }

    #[test]
    fn available_operations_for_the_staged_row() {
        assert_eq!(
            Operation::available_for(&VirtualOid::Staged, false),
            vec![
                Operation::Squash,
                Operation::Fixup,
                Operation::Commit,
                Operation::Unstage,
                Operation::Autofixup,
                Operation::Undo,
                Operation::Redo,
            ]
        );
    }

    #[test]
    fn every_operation_maps_to_its_key_command() {
        // Guards the picker's dispatch: each menu item routes to the matching
        // commit-list command.
        assert_eq!(Operation::Split.key_command(), KeyCommand::Split);
        assert_eq!(Operation::Stage.key_command(), KeyCommand::StageAll);
        assert_eq!(Operation::Unstage.key_command(), KeyCommand::UnstageAll);
        assert_eq!(Operation::Commit.key_command(), KeyCommand::CommitStaged);
        assert_eq!(Operation::Redo.key_command(), KeyCommand::Redo);
    }
}
