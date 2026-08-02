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

// Move commit target selection — key handling only; rendering is done via
// the commit list (separator row injection + footer).

use crate::app::{AppAction, AppMode, AppState, KeyCommand};

/// Handle an action while in MoveSelect mode.
///
/// The user navigates an insertion cursor between commits. The cursor
/// (`insert_before`) represents the position where the source commit will
/// be placed. Arrow keys move the insertion point; Enter confirms; Esc
/// cancels.
pub fn handle_key(action: KeyCommand, app: &mut AppState) -> AppAction {
    let (source_index, insert_before) = match app.mode {
        AppMode::MoveSelect {
            source_index,
            insert_before,
        } => (source_index, insert_before),
        _ => return AppAction::Handled,
    };

    let max_insert = app.list.commits.len();
    let page_size = crate::app::scroll::page_size(app.list.visible_height);

    match action {
        KeyCommand::MoveUp => {
            let next = advance_insert(
                insert_before,
                source_index,
                max_insert,
                1,
                app.list.reverse,
                true,
            );
            app.mode = AppMode::MoveSelect {
                source_index,
                insert_before: next,
            };
            app.list.selection_index =
                viewport_selection_for_separator(next, app.list.reverse, &app.list.commits);
            AppAction::Handled
        }
        KeyCommand::MoveDown => {
            let next = advance_insert(
                insert_before,
                source_index,
                max_insert,
                1,
                app.list.reverse,
                false,
            );
            app.mode = AppMode::MoveSelect {
                source_index,
                insert_before: next,
            };
            app.list.selection_index =
                viewport_selection_for_separator(next, app.list.reverse, &app.list.commits);
            AppAction::Handled
        }
        KeyCommand::PageUp => {
            let next = advance_insert(
                insert_before,
                source_index,
                max_insert,
                page_size,
                app.list.reverse,
                true,
            );
            app.mode = AppMode::MoveSelect {
                source_index,
                insert_before: next,
            };
            app.list.selection_index =
                viewport_selection_for_separator(next, app.list.reverse, &app.list.commits);
            AppAction::Handled
        }
        KeyCommand::PageDown => {
            let next = advance_insert(
                insert_before,
                source_index,
                max_insert,
                page_size,
                app.list.reverse,
                false,
            );
            app.mode = AppMode::MoveSelect {
                source_index,
                insert_before: next,
            };
            app.list.selection_index =
                viewport_selection_for_separator(next, app.list.reverse, &app.list.commits);
            AppAction::Handled
        }
        KeyCommand::Confirm => {
            let is_noop = |pos: usize| pos == source_index || pos == source_index + 1;
            if is_noop(insert_before) {
                app.set_error_message("Commit is already at this position");
                return AppAction::Handled;
            }

            let source = &app.list.commits[source_index];
            if source.oid.is_synthetic() {
                app.set_error_message("Cannot move staged/unstaged changes");
                return AppAction::Handled;
            }

            // insert_before is the commit-list index where the separator sits.
            // The source should be placed *after* the commit at insert_before - 1,
            let source_oid = source.oid.expect_real_oid();
            let insert_after_oid = if insert_before == 0 {
                // In --all mode the reference commit (root) is itself a visible
                // entry. Moving before position 0 means "make this the new root";
                // signal that with None sentinel to move_commit.
                if app.include_reference_oid {
                    None
                } else {
                    Some(app.reference_oid.clone())
                }
            } else {
                let idx = (insert_before - 1).min(app.list.commits.len().saturating_sub(1));
                app.list.commits[idx].oid.as_oid().cloned()
            };

            // Navigation left `selection_index` as a scroll anchor that can point
            // past the last commit; restore it to a valid index before leaving
            // MoveSelect so the CommitList render (e.g. behind the conflict dialog
            // if the move conflicts) doesn't index out of bounds.
            app.list.selection_index = source_index;
            app.mode = AppMode::CommitList;

            AppAction::ExecuteMove {
                source_oid,
                insert_after_oid,
            }
        }
        KeyCommand::ShowHelp => {
            app.toggle_help();
            AppAction::Handled
        }
        KeyCommand::Quit => {
            app.cancel_move_select();
            AppAction::Handled
        }
        _ => AppAction::Handled,
    }
}

/// Advance the insertion cursor by `step` positions, skipping the two no-op
/// slots (source_index and source_index + 1). `up` is the logical direction
/// before `reverse` is applied.
fn advance_insert(
    pos: usize,
    source_index: usize,
    max: usize,
    step: usize,
    reverse: bool,
    up: bool,
) -> usize {
    let is_noop = |p: usize| p == source_index || p == source_index + 1;
    let go_lower = up ^ reverse;

    let mut next = if go_lower {
        pos.saturating_sub(step)
    } else {
        pos.saturating_add(step).min(max)
    };

    // Skip noop positions (at most two consecutive: source and source+1)
    for _ in 0..2 {
        if is_noop(next) {
            next = if go_lower {
                next.saturating_sub(1)
            } else {
                next.saturating_add(1).min(max)
            };
        }
    }

    // If still on a noop, no valid move exists — stay put
    if is_noop(next) { pos } else { next }
}

/// Compute the `selection_index` value that keeps the move separator visible.
///
/// The scroll formula puts `selection_index` at the *bottom* of the viewport.
/// But `build_rows` reduces `visible_commits` by one row when `separator_visible`
/// is true, which would exclude the commit the separator must be drawn *before*.
/// Setting `selection_index` one logical step below the separator (in visual
/// terms) places the separator at the second-to-last row instead, so the
/// trigger commit is always included in `visible_commits`.
///
/// The returned value may equal `commits.len()` when `insert_before` is the
/// last commit index. That is intentional and safe: the footer renderer
/// guards against it with a mode check, and the scroll math still works.
fn viewport_selection_for_separator(
    insert_before: usize,
    reverse: bool,
    _commits: &[crate::CommitInfo],
) -> usize {
    if reverse {
        // In reverse mode, visual position = n - logical_index.
        // One step "below" visually means a lower logical index.
        insert_before.saturating_sub(1)
    } else {
        // One step below visually means a higher logical index.
        // May equal commits.len() at the boundary — safe in MoveSelect mode.
        insert_before + 1
    }
}
