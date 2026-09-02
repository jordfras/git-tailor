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

// Side-effect handlers for the split operations.

use anyhow::Result;
use git_tailor::Oid;
use git_tailor::app::{AppState, HunkPickerEntry, SplitStrategy};
use git_tailor::repo::{DEFAULT_CONTEXT_LINES, GitRepo};

use crate::dispatch::{LoopAction, settle_autostash};
use crate::{autostash_save_or_bail, get_head_oid_or_continue};

/// Number of output commits above which a split requires explicit confirmation.
pub(crate) const SPLIT_CONFIRM_THRESHOLD: usize = 5;

pub(crate) fn handle_prepare_split(
    git_repo: &mut impl GitRepo,
    app: &mut AppState,
    strategy: SplitStrategy,
    commit_oid: Oid,
) -> Result<LoopAction> {
    let head_oid = get_head_oid_or_continue!(git_repo, app);
    let count_result = match strategy {
        SplitStrategy::PerFile => git_repo.count_split_per_file(&commit_oid),
        SplitStrategy::PerHunk => git_repo.count_split_per_hunk(&commit_oid),
        SplitStrategy::PerHunkGroup => {
            git_repo.count_split_per_hunk_group(&commit_oid, &head_oid, &app.reference_oid)
        }
        // "Split out file(s)" and "split out hunk(s)" each open their own
        // picker dialog instead, dispatched before reaching here.
        SplitStrategy::OutFiles => unreachable!("OutFiles uses PrepareSplitOutFiles"),
        SplitStrategy::OutHunks => unreachable!("OutHunks uses PrepareSplitOutHunks"),
    };
    match count_result {
        Err(e) => app.set_error_message(format!("{e:#}")),
        Ok(count) if count > SPLIT_CONFIRM_THRESHOLD => {
            app.enter_split_confirm(strategy, commit_oid, head_oid, count);
        }
        Ok(_) => {
            return Ok(execute_split(
                git_repo,
                app,
                strategy,
                &commit_oid,
                &head_oid,
            ));
        }
    }
    Ok(LoopAction::Proceed)
}

/// Load the commit's diff so the picker can show each changed file's full
/// diff in the preview pane, not just its path.
pub(crate) fn handle_prepare_split_out_files(
    git_repo: &impl GitRepo,
    app: &mut AppState,
    commit_oid: Oid,
) -> Result<LoopAction> {
    match git_repo.commit_diff(&commit_oid, DEFAULT_CONTEXT_LINES) {
        Err(e) => app.set_error_message(format!("{e:#}")),
        Ok(diff) if diff.files.len() < 2 => {
            app.set_error_message("Commit touches fewer than 2 files — nothing to split out");
        }
        Ok(diff) => app.enter_split_files_select(commit_oid, diff.files),
    }
    Ok(LoopAction::Proceed)
}

/// Load the commit's diff at `context_lines` and flatten it into one
/// [`HunkPickerEntry`] per hunk, in file/hunk order — `delta_idx`/`hunk_idx`
/// are that position, matching what the backend expects when the diff is
/// rebuilt at the same context level. Also used to refresh the picker when
/// the user adjusts context with `+`/`-` while it's open.
pub(crate) fn handle_prepare_split_out_hunks(
    git_repo: &impl GitRepo,
    app: &mut AppState,
    commit_oid: Oid,
    context_lines: u32,
) -> Result<LoopAction> {
    match git_repo.commit_diff(&commit_oid, context_lines) {
        Err(e) => app.set_error_message(format!("{e:#}")),
        Ok(diff) => {
            let hunks: Vec<HunkPickerEntry> = diff
                .files
                .iter()
                .enumerate()
                .flat_map(|(delta_idx, file)| {
                    let file_path = file
                        .new_path
                        .clone()
                        .or_else(|| file.old_path.clone())
                        .unwrap_or_default();
                    file.hunks
                        .iter()
                        .enumerate()
                        .map(move |(hunk_idx, hunk)| HunkPickerEntry {
                            delta_idx,
                            hunk_idx,
                            file_path: file_path.clone(),
                            hunk: hunk.clone(),
                        })
                })
                .collect();
            if hunks.len() < 2 {
                app.set_error_message("Commit has fewer than 2 hunks — nothing to split out");
            } else {
                app.enter_split_hunks_select(commit_oid, hunks, context_lines);
            }
        }
    }
    Ok(LoopAction::Proceed)
}

pub(crate) fn handle_execute_split_out_files(
    git_repo: &mut impl GitRepo,
    app: &mut AppState,
    commit_oid: Oid,
    file_paths: Vec<String>,
) -> Result<LoopAction> {
    let head_oid = get_head_oid_or_continue!(git_repo, app);
    autostash_save_or_bail!(git_repo, app);
    let result = git_repo.split_commit_out_files(&commit_oid, &file_paths, &head_oid);
    Ok(settle_split_autostash(git_repo, app, result))
}

pub(crate) fn handle_execute_split_out_hunks(
    git_repo: &mut impl GitRepo,
    app: &mut AppState,
    commit_oid: Oid,
    hunks: Vec<(usize, usize)>,
    context_lines: u32,
) -> Result<LoopAction> {
    let head_oid = get_head_oid_or_continue!(git_repo, app);
    autostash_save_or_bail!(git_repo, app);
    let result = git_repo.split_commit_out_hunks(&commit_oid, &hunks, &head_oid, context_lines);
    Ok(settle_split_autostash(git_repo, app, result))
}

/// Execute a split operation; returns true when a reload is needed.
pub(crate) fn execute_split(
    git_repo: &mut impl GitRepo,
    app: &mut AppState,
    strategy: SplitStrategy,
    commit_oid: &Oid,
    head_oid: &Oid,
) -> LoopAction {
    // Stash dirty state first so a split whose files overlap uncommitted changes
    // is not refused: a split reproduces the same final tree, so reapplying the
    // stash afterwards is always conflict-free.
    if let Err(e) = git_repo.autostash_save() {
        app.set_error_message(format!("Auto-stash failed: {e:#}"));
        return LoopAction::Proceed;
    }
    let result = match strategy {
        SplitStrategy::PerFile => git_repo.split_commit_per_file(commit_oid, head_oid),
        SplitStrategy::PerHunk => git_repo.split_commit_per_hunk(commit_oid, head_oid),
        SplitStrategy::PerHunkGroup => {
            git_repo.split_commit_per_hunk_group(commit_oid, head_oid, &app.reference_oid)
        }
        // "Split out file(s)" and "split out hunk(s)" never reach
        // PrepareSplit/ExecuteSplit at all — each is executed via its own
        // handle_execute_split_out_* once confirmed in its picker dialog.
        SplitStrategy::OutFiles => unreachable!("OutFiles uses ExecuteSplitOutFiles"),
        SplitStrategy::OutHunks => unreachable!("OutHunks uses ExecuteSplitOutHunks"),
    };
    settle_split_autostash(git_repo, app, result)
}

/// Restore the auto-stash after a split. On success, reapply the stash and show
/// the outcome (opening the stash-conflict dialog if it cannot reapply); on
/// failure, put the stashed changes back and surface the error.
fn settle_split_autostash(
    git_repo: &mut impl GitRepo,
    app: &mut AppState,
    result: Result<()>,
) -> LoopAction {
    match result {
        Ok(()) => settle_autostash(
            app,
            git_repo.autostash_restore(),
            "Split",
            "Commit split",
            LoopAction::Reload,
        ),
        Err(e) => {
            let _ = git_repo.autostash_restore();
            app.set_error_message(format!("Split failed: {e:#}"));
            LoopAction::Proceed
        }
    }
}
