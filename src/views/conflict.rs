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

// Rebase conflict resolution dialog, shared by drop/squash/etc.

use super::dialog::{
    ConflictView, Dialog, DialogKind, TextRole, handle_dialog_scroll, inner_width,
    push_resume_failure, render_conflict_dialog,
};
use crate::app::{AppAction, AppMode, AppState, KeyCommand};
use crate::repo::Resume;
use ratatui::{
    Frame,
    style::{Color, Style},
    text::{Line, Span},
};

/// Handle an action while in RebaseConflict mode.
pub fn handle_conflict_key(action: KeyCommand, app: &mut AppState) -> AppAction {
    match action {
        KeyCommand::Confirm => {
            if let AppMode::RebaseConflict(state) =
                std::mem::replace(&mut app.mode, AppMode::CommitList)
            {
                AppAction::RebaseContinue(*state)
            } else {
                AppAction::Handled
            }
        }
        KeyCommand::Mergetool => {
            if app.resume_failure.is_some() {
                // Nothing to open: a resume only runs once the markers are
                // resolved, which is why the instruction row hides this.
                return AppAction::Handled;
            }
            if let AppMode::RebaseConflict(ref state) = app.mode {
                AppAction::RunMergetool {
                    files: state.conflicting_files.clone(),
                    conflict_state: state.as_ref().clone(),
                }
            } else {
                AppAction::Handled
            }
        }
        KeyCommand::OpenEditor => {
            if app.resume_failure.is_some() {
                return AppAction::Handled;
            }
            if let AppMode::RebaseConflict(ref state) = app.mode {
                AppAction::RunEditor {
                    files: state.conflicting_files.clone(),
                    conflict_state: state.as_ref().clone(),
                }
            } else {
                AppAction::Handled
            }
        }
        KeyCommand::ShowHelp => {
            app.toggle_help();
            AppAction::Handled
        }
        KeyCommand::Quit => {
            if let AppMode::RebaseConflict(state) =
                std::mem::replace(&mut app.mode, AppMode::CommitList)
            {
                AppAction::RebaseAbort(*state)
            } else {
                AppAction::Handled
            }
        }
        _ => {
            handle_dialog_scroll(action, &mut app.dialog);
            AppAction::Handled
        }
    }
}

/// Render the conflict resolution dialog as a centered overlay.
///
/// Used by any operation (drop, squash, etc.) that may hit a merge conflict
/// during cherry-pick. The dialog title and body text adapt to the
/// `operation_label` stored in `ConflictState`.
pub fn render_conflict(app: &mut AppState, frame: &mut Frame) {
    let state = match &app.mode {
        AppMode::RebaseConflict(s) => s,
        _ => return,
    };

    let short_oid = state.conflicting_commit_oid.short();

    let label = &state.operation_label;
    let label_lower = label.to_lowercase();

    // Look up the commit summary from the loaded commit list so the user can
    // see which commit is conflicting without having to remember the OID.
    let commit_summary = app
        .list
        .commits
        .iter()
        .find(|c| c.oid.as_oid() == Some(&state.conflicting_commit_oid))
        .map(|c| c.summary.as_str())
        .unwrap_or("");

    const PREFERRED_WIDTH: u16 = 62;
    let iw = inner_width(PREFERRED_WIDTH, frame.area().width);

    let remaining = state.remaining_oids().len();

    // A carry conflict is not a commit conflicting: the rewrite is already
    // written, and what is left is the other row with nowhere to land. Naming a
    // commit here would point at the wrong thing entirely.
    if let Resume::CarryRow(lifted) = &state.resume {
        let other = lifted.source.other().label().to_lowercase();
        let note = format!(
            "The {label_lower} itself is done. Your {other} could not be put back on top of what you resolved it to. Resolve the markers below, then continue to keep them — or abort to undo the whole {label_lower} and get your changes back unchanged."
        );
        let dialog = Dialog::new(DialogKind::Danger, app.colors).heading(
            format!("Working-tree conflict after {label_lower}"),
            TextRole::Danger,
        );
        // The operation's own prose describes a conflict waiting to be
        // resolved. After a failed resume the markers are already gone and the
        // note above supersedes it — and dropping it keeps the instructions
        // that say how to get out on screen rather than below the fold.
        let left = format!(
            "The {label_lower} itself is done and your {other} are still set aside. Clear what is named above, then press Enter to try again — or press Esc to unwind the whole {label_lower} and get your changes back unchanged."
        );
        let dialog = push_resume_failure(dialog, app, iw, &left);
        let dialog = if app.resume_failure.is_some() {
            dialog
        } else {
            dialog.wrapped(&note, iw)
        };
        let files = state.conflicting_files.clone();
        let still_unresolved = state.still_unresolved;
        let title = format!("{label} Conflict");
        render_conflict_dialog(
            app,
            frame,
            dialog,
            ConflictView {
                files: &files,
                still_unresolved,
                resume_failed: app.resume_failure.is_some(),
            },
            PREFERRED_WIDTH,
            &title,
        );
        return;
    }

    let dialog = Dialog::new(DialogKind::Danger, app.colors).heading(
        format!("Merge conflict during {label_lower}"),
        TextRole::Danger,
    );
    let mut dialog = push_resume_failure(
        dialog,
        app,
        iw,
        "Still in progress: your branch is on the commit it paused at. Clear what is named above, then press Enter to try again — or press Esc to abort and put the branch back where it started.",
    );

    dialog = dialog.push_line(Line::from(vec![
        Span::raw(" Conflict in "),
        Span::styled(
            short_oid.to_string(),
            Style::default().fg(app.colors.resolve(Color::Cyan)),
        ),
    ]));

    if !commit_summary.is_empty() {
        dialog = dialog.wrapped(commit_summary, iw.saturating_sub(1));
    }

    // For move operations, clarify whether the moved commit itself conflicted
    // or a commit being rebased on top of it.
    let moved_commit_oid = match &state.resume {
        Resume::Chain {
            moved_commit_oid, ..
        } => moved_commit_oid.as_ref(),
        Resume::Squash(_) | Resume::CarryRow(_) => None,
    };
    if app.resume_failure.is_some() {
        // Stale after a failed resume, as above.
    } else if let Some(moved_oid) = moved_commit_oid {
        let note = if state.conflicting_commit_oid == *moved_oid {
            " The moved commit itself caused the conflict."
        } else {
            " A commit being rebased on top of the moved commit conflicted."
        };
        dialog = dialog.wrapped_styled(note, iw, TextRole::Highlight);
    } else if state.operation_label == "Squash" {
        let note = if state.is_squash_tree_conflict() {
            " The squash itself caused the conflict."
        } else {
            " A commit being rebased after the squash conflicted."
        };
        dialog = dialog.wrapped_styled(note, iw, TextRole::Highlight);
    }

    if remaining > 0 {
        let note = format!(" ({remaining} commit(s) still to rebase after this)");
        dialog = dialog.wrapped_indent(&note, iw);
    }

    // Copy the bits the shared renderer needs so the immutable borrow of
    // `app.mode` (via `state`) ends before it takes `app` mutably.
    let title = format!("{label} Conflict");
    let files = state.conflicting_files.clone();
    let still_unresolved = state.still_unresolved;
    render_conflict_dialog(
        app,
        frame,
        dialog,
        ConflictView {
            files: &files,
            still_unresolved,
            resume_failed: app.resume_failure.is_some(),
        },
        PREFERRED_WIDTH,
        &title,
    );
}
