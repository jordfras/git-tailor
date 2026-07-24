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

// Side-effect handlers for undo/redo — operation-agnostic, journal-driven:
// they step the branch back and forth through the recorded history of every
// other operation.

use anyhow::Result;
use git_tailor::app::AppState;
use git_tailor::repo::{AutostashRestore, GitRepo, UndoOutcome};

use crate::dispatch::{LoopAction, settle_autostash};

pub(crate) fn handle_undo(git_repo: &mut impl GitRepo, app: &mut AppState) -> Result<LoopAction> {
    // A working-tree-preserving undo (stage/unstage all, or a commit's soft
    // reset) restores the very state that auto-stash would squirrel away and
    // reapply — running the stash dance would negate it — so bypass it for those.
    let skip_autostash = git_repo.pending_undo_skips_autostash().unwrap_or(false);
    if !skip_autostash && let Err(e) = git_repo.autostash_save() {
        app.set_error_message(format!("Auto-stash failed: {e}"));
        return Ok(LoopAction::Proceed);
    }
    let outcome = git_repo.undo();
    let restored = if skip_autostash {
        Ok(AutostashRestore::Done)
    } else {
        git_repo.autostash_restore()
    };
    match outcome {
        Ok(UndoOutcome::Done { label }) => Ok(settle_autostash(
            app,
            restored,
            "Undo",
            &format!("Undid {}", label.to_lowercase()),
            LoopAction::Reload,
        )),
        Ok(UndoOutcome::Empty) => {
            app.set_error_message("Nothing to undo");
            Ok(LoopAction::Proceed)
        }
        Ok(UndoOutcome::Stale) => {
            app.set_error_message("Undo history no longer matches the branch — discarded");
            Ok(LoopAction::Reload)
        }
        Err(e) => {
            app.set_error_message(format!("Undo failed: {e}"));
            Ok(LoopAction::Proceed)
        }
    }
}

pub(crate) fn handle_redo(git_repo: &mut impl GitRepo, app: &mut AppState) -> Result<LoopAction> {
    // See handle_undo: skip the auto-stash dance for a working-tree-preserving redo.
    let skip_autostash = git_repo.pending_redo_skips_autostash().unwrap_or(false);
    if !skip_autostash && let Err(e) = git_repo.autostash_save() {
        app.set_error_message(format!("Auto-stash failed: {e}"));
        return Ok(LoopAction::Proceed);
    }
    let outcome = git_repo.redo();
    let restored = if skip_autostash {
        Ok(AutostashRestore::Done)
    } else {
        git_repo.autostash_restore()
    };
    match outcome {
        Ok(UndoOutcome::Done { label }) => Ok(settle_autostash(
            app,
            restored,
            "Redo",
            &format!("Redid {}", label.to_lowercase()),
            LoopAction::Reload,
        )),
        Ok(UndoOutcome::Empty) => {
            app.set_error_message("Nothing to redo");
            Ok(LoopAction::Proceed)
        }
        Ok(UndoOutcome::Stale) => {
            app.set_error_message("Redo history no longer matches the branch — discarded");
            Ok(LoopAction::Reload)
        }
        Err(e) => {
            app.set_error_message(format!("Redo failed: {e}"));
            Ok(LoopAction::Proceed)
        }
    }
}
