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

// Side-effect handlers for the single-commit history rewrites: drop, move,
// reword, and squash/fixup. (Split and Edit are richer multi-step flows and
// live in their own modules.)

use anyhow::Result;
use git_tailor::Oid;
use git_tailor::app::{AppState, SquashMode, SquashSource};
use git_tailor::repo::{GitRepo, WorktreeSourceSnapshot};

use crate::dispatch::{LoopAction, edit_message_suspended, handle_rebase_outcome};
use crate::{autostash_save_or_bail, get_head_oid_or_continue};

pub(crate) fn handle_execute_drop(
    git_repo: &mut impl GitRepo,
    app: &mut AppState,
    commit_oid: Oid,
    head_oid: Oid,
) -> Result<LoopAction> {
    autostash_save_or_bail!(git_repo, app);
    let outcome = git_repo.drop_commit(&commit_oid, &head_oid);
    Ok(handle_rebase_outcome(
        git_repo,
        app,
        outcome,
        "Drop",
        "Commit dropped",
    ))
}

pub(crate) fn handle_execute_move(
    git_repo: &mut impl GitRepo,
    app: &mut AppState,
    source_oid: Oid,
    insert_after_oid: Option<Oid>,
) -> Result<LoopAction> {
    let head_oid = get_head_oid_or_continue!(git_repo, app);
    autostash_save_or_bail!(git_repo, app);
    let outcome = git_repo.move_commit(&source_oid, insert_after_oid.as_ref(), &head_oid);
    Ok(handle_rebase_outcome(
        git_repo,
        app,
        outcome,
        "Move",
        "Commit moved",
    ))
}

pub(crate) fn handle_prepare_reword(
    git_repo: &impl GitRepo,
    app: &mut AppState,
    commit_oid: Oid,
    current_message: String,
    terminal_guard: &mut crate::terminal_guard::TerminalGuard,
    kb_enhanced: bool,
) -> Result<LoopAction> {
    let head_oid = get_head_oid_or_continue!(git_repo, app);
    let editor_result =
        edit_message_suspended(git_repo, terminal_guard, kb_enhanced, &current_message);
    match editor_result {
        Err(e) => app.set_error_message(format!("Editor error: {e:#}")),
        Ok(new_message) if new_message.trim().is_empty() => {
            app.set_success_message("Reword cancelled: message is empty");
        }
        Ok(new_message) if new_message == current_message => {
            app.set_success_message("No changes made");
        }
        Ok(new_message) => match git_repo.reword_commit(&commit_oid, &new_message, &head_oid) {
            Ok(()) => return Ok(LoopAction::ReloadPreserving),
            Err(e) => app.set_error_message(format!("Reword failed: {e:#}")),
        },
    }
    Ok(LoopAction::Proceed)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_prepare_squash(
    git_repo: &mut impl GitRepo,
    app: &mut AppState,
    source: SquashSource,
    target_oid: Oid,
    target_message: String,
    squash_mode: SquashMode,
    terminal_guard: &mut crate::terminal_guard::TerminalGuard,
    kb_enhanced: bool,
) -> Result<LoopAction> {
    let label = squash_mode.label();
    let Some(prepared) = prepare_source(git_repo, app, &source, label)? else {
        return Ok(LoopAction::Continue);
    };
    let (source_oid, head_oid) = (prepared.source_oid().clone(), prepared.head_oid().clone());

    // A working-tree row has no message of its own, so a squash starts from the
    // target's alone rather than the two joined.
    let combined = match source.message() {
        Some(source_message) => format!("{target_message}\n\n{source_message}"),
        None => target_message.clone(),
    };
    let message_for_context = if squash_mode.keeps_target_message() {
        target_message.clone()
    } else {
        combined.clone()
    };
    match git_repo.squash_try_combine(
        &source_oid,
        &target_oid,
        &message_for_context,
        squash_mode,
        &head_oid,
    ) {
        Ok(Some(conflict_state)) => {
            // Squash-tree conflict — defer restoring the working tree until the
            // user resolves and the squash finalizes (or aborts).
            app.enter_rebase_conflict(conflict_state);
            return Ok(LoopAction::Continue);
        }
        Err(e) => {
            return Ok(prepared.unwind(
                git_repo,
                app,
                format!("{label} failed: {e:#}"),
                LoopAction::Continue,
            ));
        }
        Ok(None) => {}
    }
    let final_message = if squash_mode.keeps_target_message() {
        Some(target_message)
    } else {
        let editor_result =
            edit_message_suspended(git_repo, terminal_guard, kb_enhanced, &combined);
        match editor_result {
            Err(e) => {
                return Ok(prepared.unwind(
                    git_repo,
                    app,
                    format!("Editor error: {e:#}"),
                    LoopAction::Continue,
                ));
            }
            Ok(msg) if msg.trim().is_empty() => {
                return Ok(prepared.unwind(
                    git_repo,
                    app,
                    format!("{label} aborted: empty commit message"),
                    LoopAction::Continue,
                ));
            }
            Ok(msg) => Some(msg),
        }
    };
    if let Some(msg) = final_message {
        let success_msg = match squash_mode {
            SquashMode::Fixup => format!("{} fixed up", source.label()),
            SquashMode::Squash => format!("{} squashed", source.label()),
        };
        let outcome = git_repo.squash_commits(&source_oid, &target_oid, &msg, &head_oid);
        if let Err(e) = outcome {
            return Ok(prepared.unwind(
                git_repo,
                app,
                format!("{label} failed: {e:#}"),
                LoopAction::Proceed,
            ));
        }
        return Ok(handle_rebase_outcome(
            git_repo,
            app,
            outcome,
            label,
            &success_msg,
        ));
    }
    Ok(LoopAction::Proceed)
}

/// A squash source got into a shape the squash engine accepts, together with
/// what has to be put back if the squash does not go through.
///
/// The two ways of clearing the working tree out of a squash's path pair with
/// two different ways of restoring it, so they are one choice made once rather
/// than two made in separate places.
enum Prepared {
    /// A commit source, with the working tree stashed out of the way.
    Commit { source_oid: Oid, head_oid: Oid },
    /// A working-tree row lifted into a temporary commit, which is both the
    /// source and the tip the squash runs from.
    Lifted(WorktreeSourceSnapshot),
}

impl Prepared {
    /// The commit whose changes are being folded in.
    fn source_oid(&self) -> &Oid {
        match self {
            Prepared::Commit { source_oid, .. } => source_oid,
            Prepared::Lifted(snapshot) => &snapshot.temp_oid,
        }
    }

    /// The tip the squash rewrites from.
    fn head_oid(&self) -> &Oid {
        match self {
            Prepared::Commit { head_oid, .. } => head_oid,
            Prepared::Lifted(snapshot) => &snapshot.temp_oid,
        }
    }

    /// Report a squash that gave up, putting back whatever was set up for it.
    ///
    /// An unwind that itself fails leaves the temporary commit on the branch,
    /// holding the row's changes. Say so and reload, rather than reporting only
    /// the original failure and leaving a commit the user cannot see.
    fn unwind(
        &self,
        git_repo: &mut impl GitRepo,
        app: &mut AppState,
        message: String,
        done: LoopAction,
    ) -> LoopAction {
        match self {
            Prepared::Commit { .. } => {
                let _ = git_repo.autostash_restore();
                app.set_error_message(message);
                done
            }
            Prepared::Lifted(snapshot) => match git_repo.abort_worktree_source(snapshot) {
                Ok(()) => {
                    app.set_error_message(message);
                    done
                }
                Err(e) => {
                    app.set_error_message(format!(
                        "{message}; your changes are in a temporary commit on the \
                         branch — undoing it failed: {e:#}"
                    ));
                    LoopAction::Reload
                }
            },
        }
    }
}

/// Get the source into a shape the squash engine accepts, and the working tree
/// into a shape it can check out over.
///
/// A commit source needs the ordinary auto-stash. A working-tree row is lifted
/// into a temporary commit instead, which both makes it a source *and* leaves
/// only the other row's changes behind — recorded exactly, so no stash is
/// needed. Returns `None` when the caller should give up (the error message is
/// already set).
fn prepare_source(
    git_repo: &mut impl GitRepo,
    app: &mut AppState,
    source: &SquashSource,
    label: &str,
) -> Result<Option<Prepared>> {
    match source {
        SquashSource::Commit { oid, .. } => {
            let head_oid = match git_repo.head_oid() {
                Ok(oid) => oid,
                Err(e) => {
                    app.set_error_message(format!("Failed to get HEAD: {e:#}"));
                    return Ok(None);
                }
            };
            if let Err(e) = git_repo.autostash_save() {
                app.set_error_message(format!("Auto-stash failed: {e:#}"));
                return Ok(None);
            }
            Ok(Some(Prepared::Commit {
                source_oid: oid.clone(),
                head_oid,
            }))
        }
        SquashSource::Worktree(row) => match git_repo.begin_worktree_source(*row) {
            Ok(Some(snapshot)) => Ok(Some(Prepared::Lifted(snapshot))),
            Ok(None) => {
                app.set_error_message(format!("Nothing to {}", label.to_lowercase()));
                Ok(None)
            }
            Err(e) => {
                app.set_error_message(format!("{label} failed: {e:#}"));
                Ok(None)
            }
        },
    }
}
