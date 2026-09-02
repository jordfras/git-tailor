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

use git_tailor::Oid;
use git_tailor::app::{AppMode, AppState, CommitListState, SplitStrategy};
use git_tailor::repo::RepoWrite;
use git_tailor::{
    CommitDiff, CommitInfo, DeltaStatus, DiffLine, DiffLineKind, FileDiff, Hunk, VirtualOid,
};

use crate::mock_repo::{MockRepo, make_conflict_state};

use super::conflict::ToolRun;
use super::split::SPLIT_CONFIRM_THRESHOLD;
use super::*;

#[test]
fn execute_drop_complete_sets_success_message() {
    let mut repo = MockRepo::default();
    let mut app = AppState::default();
    let result = handle_execute_drop(
        &mut repo,
        &mut app,
        Oid::from("a".repeat(40)),
        Oid::from("b".repeat(40)),
    );
    assert!(matches!(result, Ok(LoopAction::ReloadPreserving)));
    assert_eq!(app.status.message.as_deref(), Some("Commit dropped"));
    assert!(!app.status.is_error);
}

#[test]
fn execute_drop_opens_stash_conflict_dialog_on_conflict() {
    // The drop completed, but reapplying the auto-stash conflicted. Instead of a
    // terse error the user is dropped into the resolution dialog (same as a
    // cherry-pick conflict), so their changes are never silently abandoned.
    let mut repo = MockRepo {
        autostash_restore_ok: false,
        ..MockRepo::default()
    };
    let mut app = AppState::default();
    let result = handle_execute_drop(
        &mut repo,
        &mut app,
        Oid::from("a".repeat(40)),
        Oid::from("b".repeat(40)),
    );
    assert!(matches!(result, Ok(LoopAction::Continue)));
    match &app.mode {
        AppMode::StashConflict(state) => {
            assert_eq!(state.operation_label, "Drop");
            assert_eq!(state.conflicting_files, vec!["conflict.txt".to_string()]);
        }
        other => panic!("expected StashConflict mode, got {other:?}"),
    }
}

#[test]
fn execute_drop_error_sets_error_message() {
    let mut repo = MockRepo {
        drop_ok: false,
        ..MockRepo::default()
    };
    let mut app = AppState::default();
    let _ = handle_execute_drop(
        &mut repo,
        &mut app,
        Oid::from("a".repeat(40)),
        Oid::from("b".repeat(40)),
    );
    assert!(app.status.is_error);
    assert!(
        app.status
            .message
            .as_deref()
            .unwrap_or("")
            .contains("Drop failed")
    );
}

#[test]
fn execute_move_complete_sets_success_message() {
    let mut repo = MockRepo::default();
    let mut app = AppState::default();
    let result = handle_execute_move(&mut repo, &mut app, Oid::from("a".repeat(40)), None);
    assert!(matches!(result, Ok(LoopAction::ReloadPreserving)));
    assert_eq!(app.status.message.as_deref(), Some("Commit moved"));
    assert!(!app.status.is_error);
}

#[test]
fn execute_move_head_error_continues() {
    let mut repo = MockRepo {
        head_ok: false,
        ..MockRepo::default()
    };
    let mut app = AppState::default();
    let result = handle_execute_move(&mut repo, &mut app, Oid::from("a".repeat(40)), None);
    assert!(matches!(result, Ok(LoopAction::Continue)));
    assert!(app.status.is_error);
}

#[test]
fn rebase_abort_success_sets_success_message() {
    let mut repo = MockRepo::default();
    let mut app = AppState::default();
    let state = make_conflict_state();
    let result = handle_rebase_abort(
        &mut repo,
        &mut app,
        &mut PendingAutofixupSelection::default(),
        state,
    );
    assert!(matches!(result, Ok(LoopAction::Reload)));
    assert!(!app.status.is_error);
    assert!(
        app.status
            .message
            .as_deref()
            .unwrap_or("")
            .contains("aborted")
    );
}

#[test]
fn rebase_abort_error_sets_error_message() {
    let mut repo = MockRepo {
        abort_ok: false,
        ..MockRepo::default()
    };
    let mut app = AppState::default();
    let state = make_conflict_state();
    let _ = handle_rebase_abort(
        &mut repo,
        &mut app,
        &mut PendingAutofixupSelection::default(),
        state,
    );
    assert!(app.status.is_error);
    assert!(
        app.status
            .message
            .as_deref()
            .unwrap_or("")
            .contains("Abort failed")
    );
}

#[test]
fn prepare_split_count_error_sets_error_message() {
    let mut repo = MockRepo {
        count_ok: false,
        ..MockRepo::default()
    };
    let mut app = AppState::default();
    let result = handle_prepare_split(
        &mut repo,
        &mut app,
        SplitStrategy::PerFile,
        Oid::from("a".repeat(40)),
    );
    assert!(matches!(result, Ok(LoopAction::Proceed)));
    assert!(app.status.is_error);
}

#[test]
fn prepare_split_above_threshold_enters_confirm_mode() {
    let mut repo = MockRepo {
        count_per_file: SPLIT_CONFIRM_THRESHOLD + 1,
        ..MockRepo::default()
    };
    let mut app = AppState::default();
    let _ = handle_prepare_split(
        &mut repo,
        &mut app,
        SplitStrategy::PerFile,
        Oid::from("a".repeat(40)),
    );
    assert!(matches!(app.mode, AppMode::SplitConfirm(_)));
}

/// A single-line hunk, for `handle_prepare_split_out_hunks` fixtures.
fn one_line_hunk(old_start: u32) -> Hunk {
    Hunk {
        old_start,
        old_lines: 1,
        new_start: old_start,
        new_lines: 1,
        lines: vec![
            DiffLine {
                kind: DiffLineKind::Deletion,
                content: "old\n".to_string(),
            },
            DiffLine {
                kind: DiffLineKind::Addition,
                content: "new\n".to_string(),
            },
        ],
    }
}

/// Two files, three hunks total: a.txt has two, b.txt has one — matching the
/// commit_diff fixture `handle_prepare_split_out_hunks` flattens into
/// `HunkPickerEntry` rows.
fn three_hunk_commit_diff() -> CommitDiff {
    CommitDiff {
        commit: CommitInfo {
            oid: VirtualOid::Real(Oid::from("a".repeat(40))),
            summary: String::new(),
            author: None,
            date: None,
            parent_oids: vec![],
            message: String::new(),
            author_email: None,
            author_date: None,
            committer: None,
            committer_email: None,
            commit_date: None,
        },
        files: vec![
            FileDiff {
                old_path: Some("a.txt".to_string()),
                new_path: Some("a.txt".to_string()),
                status: DeltaStatus::Modified,
                is_binary: false,
                hunks: vec![one_line_hunk(1), one_line_hunk(10)],
            },
            FileDiff {
                old_path: Some("b.txt".to_string()),
                new_path: Some("b.txt".to_string()),
                status: DeltaStatus::Modified,
                is_binary: false,
                hunks: vec![one_line_hunk(1)],
            },
        ],
    }
}

/// The commit's diff is flattened into one `HunkPickerEntry` per hunk, in
/// file/hunk order, with `delta_idx`/`hunk_idx` matching that position — the
/// exact pair the backend expects back when the split is confirmed.
#[test]
fn prepare_split_out_hunks_flattens_diff_into_picker_entries() {
    let repo = MockRepo {
        commit_diff: Some(three_hunk_commit_diff()),
        ..MockRepo::default()
    };
    let mut app = AppState::default();

    let result = handle_prepare_split_out_hunks(&repo, &mut app, Oid::from("a".repeat(40)), 3);

    assert!(matches!(result, Ok(LoopAction::Proceed)));
    match &app.mode {
        AppMode::SplitHunksSelect {
            hunks,
            context_lines,
            ..
        } => {
            let ids: Vec<(usize, usize, &str)> = hunks
                .iter()
                .map(|h| (h.delta_idx, h.hunk_idx, h.file_path.as_str()))
                .collect();
            assert_eq!(ids, vec![(0, 0, "a.txt"), (0, 1, "a.txt"), (1, 0, "b.txt")]);
            assert_eq!(*context_lines, 3);
        }
        other => panic!("expected SplitHunksSelect mode, got {other:?}"),
    }
}

/// A commit with fewer than 2 hunks total refuses to open the picker — an
/// empty or single-hunk "rest" split is meaningless.
#[test]
fn prepare_split_out_hunks_refuses_fewer_than_two_hunks() {
    let mut diff = three_hunk_commit_diff();
    diff.files.truncate(1);
    diff.files[0].hunks.truncate(1);
    let repo = MockRepo {
        commit_diff: Some(diff),
        ..MockRepo::default()
    };
    let mut app = AppState::default();

    let result = handle_prepare_split_out_hunks(&repo, &mut app, Oid::from("a".repeat(40)), 3);

    assert!(matches!(result, Ok(LoopAction::Proceed)));
    assert!(app.status.is_error);
    assert_eq!(app.mode, AppMode::CommitList);
}

/// A `commit_diff` failure surfaces as an error message rather than entering
/// the picker with stale or empty data.
#[test]
fn prepare_split_out_hunks_error_sets_error_message() {
    let repo = MockRepo::default(); // commit_diff left unconfigured -> Err
    let mut app = AppState::default();

    let result = handle_prepare_split_out_hunks(&repo, &mut app, Oid::from("a".repeat(40)), 3);

    assert!(matches!(result, Ok(LoopAction::Proceed)));
    assert!(app.status.is_error);
    assert_eq!(app.mode, AppMode::CommitList);
}

/// Three files: a.txt and b.txt modified, c.txt deleted (no `new_path`) — the
/// deleted-file case that exercises `FileDiff`'s "fall back to old_path"
/// identity resolution used both here and in `split_files_select`.
fn three_file_commit_diff() -> CommitDiff {
    CommitDiff {
        commit: CommitInfo {
            oid: VirtualOid::Real(Oid::from("a".repeat(40))),
            summary: String::new(),
            author: None,
            date: None,
            parent_oids: vec![],
            message: String::new(),
            author_email: None,
            author_date: None,
            committer: None,
            committer_email: None,
            commit_date: None,
        },
        files: vec![
            FileDiff {
                old_path: Some("a.txt".to_string()),
                new_path: Some("a.txt".to_string()),
                status: DeltaStatus::Modified,
                is_binary: false,
                hunks: vec![one_line_hunk(1)],
            },
            FileDiff {
                old_path: Some("b.txt".to_string()),
                new_path: Some("b.txt".to_string()),
                status: DeltaStatus::Modified,
                is_binary: false,
                hunks: vec![one_line_hunk(1)],
            },
            FileDiff {
                old_path: Some("c.txt".to_string()),
                new_path: None,
                status: DeltaStatus::Deleted,
                is_binary: false,
                hunks: vec![one_line_hunk(1)],
            },
        ],
    }
}

/// The commit's diff is loaded as-is into the picker's file list, in diff
/// order — including a deleted file, whose identity must resolve to its
/// `old_path` since it has no `new_path`.
#[test]
fn prepare_split_out_files_loads_diff_into_picker_files() {
    let repo = MockRepo {
        commit_diff: Some(three_file_commit_diff()),
        ..MockRepo::default()
    };
    let mut app = AppState::default();

    let result = handle_prepare_split_out_files(&repo, &mut app, Oid::from("a".repeat(40)));

    assert!(matches!(result, Ok(LoopAction::Proceed)));
    match &app.mode {
        AppMode::SplitFilesSelect { files, .. } => {
            let paths: Vec<Option<&str>> = files
                .iter()
                .map(|f| f.new_path.as_deref().or(f.old_path.as_deref()))
                .collect();
            assert_eq!(paths, vec![Some("a.txt"), Some("b.txt"), Some("c.txt")]);
        }
        other => panic!("expected SplitFilesSelect mode, got {other:?}"),
    }
}

/// A commit with fewer than 2 changed files refuses to open the picker — an
/// empty or single-file "rest" split is meaningless.
#[test]
fn prepare_split_out_files_refuses_fewer_than_two_files() {
    let mut diff = three_file_commit_diff();
    diff.files.truncate(1);
    let repo = MockRepo {
        commit_diff: Some(diff),
        ..MockRepo::default()
    };
    let mut app = AppState::default();

    let result = handle_prepare_split_out_files(&repo, &mut app, Oid::from("a".repeat(40)));

    assert!(matches!(result, Ok(LoopAction::Proceed)));
    assert!(app.status.is_error);
    assert_eq!(app.mode, AppMode::CommitList);
}

/// A `commit_diff` failure surfaces as an error message rather than entering
/// the picker with stale or empty data.
#[test]
fn prepare_split_out_files_error_sets_error_message() {
    let repo = MockRepo::default(); // commit_diff left unconfigured -> Err
    let mut app = AppState::default();

    let result = handle_prepare_split_out_files(&repo, &mut app, Oid::from("a".repeat(40)));

    assert!(matches!(result, Ok(LoopAction::Proceed)));
    assert!(app.status.is_error);
    assert_eq!(app.mode, AppMode::CommitList);
}

#[test]
fn stage_all_changed_reloads_and_reports_success() {
    let repo = MockRepo::default();
    let mut app = AppState::default();
    let action = report_stage_outcome(
        &mut app,
        repo.stage_all(),
        "Staged all changes",
        "Nothing to stage",
    );
    assert!(matches!(action, LoopAction::Reload));
    assert_eq!(app.status.message.as_deref(), Some("Staged all changes"));
    assert!(!app.status.is_error);
}

#[test]
fn stage_all_noop_reports_without_reload() {
    let repo = MockRepo {
        stage_changed: false,
        ..MockRepo::default()
    };
    let mut app = AppState::default();
    let action = report_stage_outcome(
        &mut app,
        repo.stage_all(),
        "Staged all changes",
        "Nothing to stage",
    );
    assert!(matches!(action, LoopAction::Proceed));
    assert_eq!(app.status.message.as_deref(), Some("Nothing to stage"));
    assert!(!app.status.is_error);
}

#[test]
fn stage_all_error_sets_error_message() {
    let repo = MockRepo {
        stage_ok: false,
        ..MockRepo::default()
    };
    let mut app = AppState::default();
    let action = report_stage_outcome(
        &mut app,
        repo.unstage_all(),
        "Unstaged all changes",
        "Nothing to unstage",
    );
    assert!(matches!(action, LoopAction::Proceed));
    assert!(app.status.is_error);
}

#[test]
fn stage_all_error_message_keeps_the_underlying_cause() {
    // `format!("{e}")` on an `anyhow::Error` prints only the outermost context
    // and drops the source chain, which hid libgit2's "invalid path: 'nul'"
    // behind a bare "failed to stage working-tree changes".
    let repo = MockRepo {
        stage_ok: false,
        ..MockRepo::default()
    };
    let mut app = AppState::default();
    report_stage_outcome(
        &mut app,
        repo.stage_all(),
        "Staged all changes",
        "Nothing to stage",
    );
    let message = app.status.message.as_deref().unwrap();
    assert!(
        message.contains("failed to stage working-tree changes"),
        "outer context missing from {message:?}"
    );
    assert!(
        message.contains("invalid path: 'nul'"),
        "underlying cause missing from {message:?}"
    );
}

#[test]
fn worktree_preserving_undo_skips_autostash() {
    // The working-tree-preserving undo paths (stage/unstage all, commit soft
    // reset) must not stash/restore, or they would squirrel away and reapply the
    // very state they are restoring.
    let mut repo = MockRepo {
        undo_skips_autostash: true,
        ..MockRepo::default()
    };
    let mut app = AppState::default();
    let result = handle_undo(&mut repo, &mut app);
    assert!(matches!(result, Ok(LoopAction::Reload)));
    assert_eq!(repo.autostash_save_calls.get(), 0);
    assert_eq!(app.status.message.as_deref(), Some("Undid stage all"));
    assert!(!app.status.is_error);
}

#[test]
fn ref_move_undo_runs_autostash() {
    // A normal (ref-moving) undo still runs the auto-stash dance.
    let mut repo = MockRepo::default();
    let mut app = AppState::default();
    let _ = handle_undo(&mut repo, &mut app);
    assert_eq!(repo.autostash_save_calls.get(), 1);
}

#[test]
fn clean_journal_parses_on_its_own() {
    use clap::Parser;
    assert!(crate::cli::Cli::try_parse_from(["gt", "--clean-journal"]).is_ok());
}

#[test]
fn clean_journal_conflicts_with_browse_args() {
    use clap::Parser;
    for extra in [
        ["--clean-journal", "somebase"],
        ["--clean-journal", "--all"],
        ["--clean-journal", "--static"],
    ] {
        let argv = std::iter::once("gt").chain(extra);
        assert!(
            crate::cli::Cli::try_parse_from(argv).is_err(),
            "--clean-journal must conflict with {extra:?}"
        );
    }
}

#[test]
fn clean_journal_ignores_cosmetic_flags() {
    // Cosmetic flags don't conflict (no TUI is launched), so a globally-set
    // GT_* env var won't wrongly block --clean-journal.
    use clap::Parser;
    assert!(crate::cli::Cli::try_parse_from(["gt", "--clean-journal", "--reverse"]).is_ok());
}

#[test]
fn only_key_events_dismiss_transient_status() {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

    let key_press = Event::Key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));
    assert!(crate::event_dismisses_status(&key_press));

    // A Repeat key event still counts as a keypress.
    let mut repeat = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE);
    repeat.kind = KeyEventKind::Repeat;
    assert!(crate::event_dismisses_status(&Event::Key(repeat)));

    // Non-key events still reach the loop (so it can redraw on resize) but must
    // NOT wipe the status message before it is read.
    assert!(!crate::event_dismisses_status(&Event::Resize(80, 24)));
    assert!(!crate::event_dismisses_status(&Event::FocusGained));
    assert!(!crate::event_dismisses_status(&Event::FocusLost));
}

#[test]
fn conflict_tool_finished_refreshes_rebase_dialog() {
    // After a tool resolved (some) files, the rebase-conflict dialog is rebuilt
    // with the still-conflicting files and a success banner naming the tool.
    let repo = MockRepo {
        conflicting_files: vec!["a.txt".to_string()],
        ..MockRepo::default()
    };
    let mut app = AppState::default();
    let action = handle_run_conflict_tool(
        &repo,
        &mut app,
        make_conflict_state(),
        "Merge tool",
        Ok(ToolRun::Finished),
    );
    assert!(matches!(action, LoopAction::Proceed));
    match &app.mode {
        AppMode::RebaseConflict(state) => {
            assert_eq!(state.conflicting_files, vec!["a.txt".to_string()]);
            assert!(!state.still_unresolved);
        }
        other => panic!("expected RebaseConflict mode, got {other:?}"),
    }
    assert!(
        app.status
            .message
            .as_deref()
            .unwrap_or("")
            .contains("Merge tool finished")
    );
    assert!(!app.status.is_error);
}

#[test]
fn conflict_tool_no_merge_tool_sets_error() {
    let repo = MockRepo::default();
    let mut app = AppState::default();
    let action = handle_run_conflict_tool(
        &repo,
        &mut app,
        make_conflict_state(),
        "Merge tool",
        Ok(ToolRun::NoMergeTool),
    );
    assert!(matches!(action, LoopAction::Proceed));
    assert!(app.status.is_error);
    assert!(
        app.status
            .message
            .as_deref()
            .unwrap_or("")
            .contains("No merge tool configured")
    );
}

#[test]
fn conflict_tool_failure_reports_the_tool_name() {
    let repo = MockRepo::default();
    let mut app = AppState::default();
    let action = handle_run_conflict_tool(
        &repo,
        &mut app,
        make_conflict_state(),
        "Editor",
        Err(anyhow::anyhow!("boom")),
    );
    assert!(matches!(action, LoopAction::Proceed));
    assert!(app.status.is_error);
    let msg = app.status.message.as_deref().unwrap_or("");
    assert!(msg.contains("Editor failed"), "unexpected message: {msg}");
    assert!(msg.contains("boom"), "unexpected message: {msg}");
}

#[test]
fn stash_tool_finished_refreshes_stash_dialog_keeping_the_label() {
    let repo = MockRepo {
        conflicting_files: vec!["b.txt".to_string()],
        ..MockRepo::default()
    };
    // handle_run_stash_tool reads the operation label off the current mode.
    let mut app = AppState {
        mode: AppMode::StashConflict(Box::new(StashConflictState {
            operation_label: "Drop".to_string(),
            conflicting_files: vec![],
            still_unresolved: true,
        })),
        ..Default::default()
    };
    let action = handle_run_stash_tool(&repo, &mut app, "Editor", Ok(ToolRun::Finished));
    assert!(matches!(action, LoopAction::Proceed));
    match &app.mode {
        AppMode::StashConflict(state) => {
            assert_eq!(state.operation_label, "Drop");
            assert_eq!(state.conflicting_files, vec!["b.txt".to_string()]);
            assert!(!state.still_unresolved);
        }
        other => panic!("expected StashConflict mode, got {other:?}"),
    }
    assert!(
        app.status
            .message
            .as_deref()
            .unwrap_or("")
            .contains("Editor finished")
    );
}

mod autofixup_selection {
    use super::*;
    use crate::dispatch::autofixup::{
        apply_pending_autofixup_selection, autofixup_target_selection_index,
        handle_execute_autofixup,
    };
    use git_tailor::VirtualOid;
    use git_tailor::app::SquashMode as Mode;
    use git_tailor::autofixup::AutofixupPair;

    fn commit(oid: &str) -> CommitInfo {
        CommitInfo {
            oid: VirtualOid::Real(Oid::new(oid.repeat(40))),
            summary: String::new(),
            author: None,
            date: None,
            parent_oids: vec![],
            message: String::new(),
            author_email: None,
            author_date: None,
            committer: None,
            committer_email: None,
            commit_date: None,
        }
    }

    fn synthetic(oid: VirtualOid) -> CommitInfo {
        CommitInfo {
            oid,
            summary: String::new(),
            author: None,
            date: None,
            parent_oids: vec![],
            message: String::new(),
            author_email: None,
            author_date: None,
            committer: None,
            committer_email: None,
            commit_date: None,
        }
    }

    fn pair(source: &str, target: &str) -> AutofixupPair {
        AutofixupPair {
            source_oid: Oid::new(source.repeat(40)),
            target_oid: Oid::new(target.repeat(40)),
            source_summary: String::new(),
            target_summary: String::new(),
            source_message: String::new(),
            target_message: String::new(),
            mode: Mode::Fixup,
        }
    }

    // Layout: A, T(target), F1(fixup->T), C(survivor), F2(fixup->T)
    // Batch removes F1 and F2, folding both into T.
    fn commits() -> Vec<CommitInfo> {
        vec![
            commit("a"),
            commit("t"),
            commit("1"),
            commit("c"),
            commit("2"),
        ]
    }

    fn pairs() -> Vec<AutofixupPair> {
        vec![pair("1", "t"), pair("2", "t")]
    }

    #[test]
    fn selection_on_a_surviving_commit_shifts_down_by_removed_commits_before_it() {
        // C is at index 3; only F1 (index 2) is removed before it -> index 2.
        let idx = autofixup_target_selection_index(&commits(), 3, &pairs());
        assert_eq!(idx, Some(2));
    }

    #[test]
    fn selection_before_any_removal_is_unaffected() {
        // T is at index 1; nothing removed before it.
        let idx = autofixup_target_selection_index(&commits(), 1, &pairs());
        assert_eq!(idx, Some(1));
    }

    #[test]
    fn selection_on_a_folded_away_fixup_lands_on_its_target() {
        // F2 (index 4) was folded into T (index 1); nothing removed before T.
        let idx = autofixup_target_selection_index(&commits(), 4, &pairs());
        assert_eq!(idx, Some(1));
    }

    #[test]
    fn selection_on_the_first_fixup_also_lands_on_its_target() {
        let idx = autofixup_target_selection_index(&commits(), 2, &pairs());
        assert_eq!(idx, Some(1));
    }

    #[test]
    fn synthetic_row_selection_falls_back_to_none() {
        let mut cs = commits();
        cs.push(synthetic(VirtualOid::Unstaged));
        let idx = autofixup_target_selection_index(&cs, 5, &pairs());
        assert_eq!(idx, None, "synthetic rows aren't touched by autofixup");
    }

    #[test]
    fn result_never_exceeds_the_commit_list_bounds() {
        // Even in degenerate/empty inputs the caller still clamps with `.min(len-1)`
        // before assigning to `selection_index` — verify the raw index this
        // function returns is sane (in-bounds) for a normal batch too.
        let idx = autofixup_target_selection_index(&commits(), 4, &pairs()).unwrap();
        assert!(idx < commits().len());
    }

    #[test]
    fn execute_autofixup_reloads_selecting_the_computed_index() {
        let mut repo = MockRepo::default();
        let mut app = AppState {
            list: CommitListState {
                commits: commits(),
                selection_index: 4, // F2, folded into T (index 1).
                ..Default::default()
            },
            ..Default::default()
        };
        let mut pending = PendingAutofixupSelection::default();

        let result = handle_execute_autofixup(
            &mut repo,
            &mut app,
            &mut pending,
            Oid::from("a".repeat(40)),
            Oid::from("b".repeat(40)),
            pairs(),
            std::collections::HashMap::new(),
        );

        assert!(matches!(result, Ok(LoopAction::ReloadSelecting(1))));
        assert_eq!(app.status.message.as_deref(), Some("Commits autofixed up"));
    }

    #[test]
    fn execute_autofixup_stashes_the_index_when_it_hits_a_conflict() {
        let mut repo = MockRepo {
            autofixup_conflicts: true,
            ..Default::default()
        };
        let mut app = AppState {
            list: CommitListState {
                commits: commits(),
                selection_index: 4, // F2, folded into T (index 1).
                ..Default::default()
            },
            ..Default::default()
        };
        let mut pending = PendingAutofixupSelection::default();

        let result = handle_execute_autofixup(
            &mut repo,
            &mut app,
            &mut pending,
            Oid::from("a".repeat(40)),
            Oid::from("b".repeat(40)),
            pairs(),
            std::collections::HashMap::new(),
        );

        assert!(matches!(result, Ok(LoopAction::Continue)));
        assert_eq!(
            pending.0,
            Some(1),
            "the target index computed up front must survive into the conflict dialog"
        );
    }

    #[test]
    fn apply_pending_selection_swaps_reload_preserving_for_reload_selecting() {
        let mut pending = PendingAutofixupSelection::default();
        pending.set(Some(2));
        let result =
            apply_pending_autofixup_selection(&mut pending, true, LoopAction::ReloadPreserving);
        assert!(matches!(result, LoopAction::ReloadSelecting(2)));
        assert_eq!(pending.0, None, "consumed on the completing round");
    }

    #[test]
    fn apply_pending_selection_is_a_no_op_for_non_autofixup_operations() {
        let mut pending = PendingAutofixupSelection::default();
        pending.set(Some(2));
        let result =
            apply_pending_autofixup_selection(&mut pending, false, LoopAction::ReloadPreserving);
        assert!(matches!(result, LoopAction::ReloadPreserving));
        assert_eq!(pending.0, Some(2), "not this batch's field to touch");
    }

    #[test]
    fn apply_pending_selection_falls_back_when_nothing_was_stashed() {
        let mut pending = PendingAutofixupSelection::default();
        let result =
            apply_pending_autofixup_selection(&mut pending, true, LoopAction::ReloadPreserving);
        assert!(matches!(result, LoopAction::ReloadPreserving));
    }

    #[test]
    fn apply_pending_selection_keeps_the_index_across_another_conflict_round() {
        let mut pending = PendingAutofixupSelection::default();
        pending.set(Some(2));
        let result = apply_pending_autofixup_selection(&mut pending, true, LoopAction::Continue);
        assert!(matches!(result, LoopAction::Continue));
        assert_eq!(
            pending.0,
            Some(2),
            "still resolving the batch; next round needs it"
        );
    }

    #[test]
    fn apply_pending_selection_clears_stale_state_on_failure() {
        let mut pending = PendingAutofixupSelection::default();
        pending.set(Some(2));
        let result = apply_pending_autofixup_selection(&mut pending, true, LoopAction::Proceed);
        assert!(matches!(result, LoopAction::Proceed));
        assert_eq!(
            pending.0, None,
            "batch abandoned; don't leak into a later, unrelated reload"
        );
    }
}

// --- Squash from a working-tree row -----------------------------------------
//
// The handler itself needs a live terminal for the editor step, so these cover
// the parts of it that decide anything: how a source is prepared, how it is put
// back when the squash gives up, and what the two messages say.

use crate::mock_repo::{LiftOutcome, SquashProbe, mock_temp_oid};
use git_tailor::app::{SquashMode, SquashSource};
use git_tailor::repo::WorktreeSource;
use rewrite::{prepare_source, squash_editor_seed, squash_success_message};

fn commit_source() -> SquashSource {
    SquashSource::Commit {
        oid: Oid::from("b".repeat(40)),
        message: "the source commit".to_string(),
    }
}

fn worktree_source() -> SquashSource {
    SquashSource::Worktree(WorktreeSource::Staged)
}

/// A commit source goes through the ordinary auto-stash.
#[test]
fn a_commit_source_is_prepared_behind_the_autostash() {
    let mut repo = MockRepo::default();
    let mut app = AppState::default();

    let prepared = prepare_source(&mut repo, &mut app, &commit_source(), "Squash")
        .unwrap()
        .expect("a commit source prepares");

    assert_eq!(repo.autostash_save_calls.get(), 1);
    assert_eq!(prepared.source_oid(), &Oid::from("b".repeat(40)));
    assert_eq!(
        prepared.head_oid(),
        &Oid::from("a".repeat(40)),
        "a commit source folds from the real branch tip"
    );
    prepared.discard();
}

/// A working-tree row takes no auto-stash: the lift records the pre-operation
/// state exactly, which is the whole reason the fold works without one.
#[test]
fn a_worktree_row_is_prepared_without_the_autostash() {
    let mut repo = MockRepo {
        lift: LiftOutcome::Lifted,
        ..Default::default()
    };
    let mut app = AppState::default();

    let prepared = prepare_source(&mut repo, &mut app, &worktree_source(), "Squash")
        .unwrap()
        .expect("a lifted row prepares");

    assert_eq!(
        repo.autostash_save_calls.get(),
        0,
        "the lift replaces the stash, it does not join it"
    );
    assert_eq!(prepared.source_oid(), &mock_temp_oid());
    assert_eq!(
        prepared.head_oid(),
        &mock_temp_oid(),
        "the temporary commit is both the source and the tip to fold from"
    );
    prepared.discard();
}

/// An empty row is reported in the operation's own words, not as a failure.
#[test]
fn an_empty_worktree_row_says_there_is_nothing_to_fold() {
    for (label, expected) in [
        ("Squash", "Nothing to squash"),
        ("Fixup", "Nothing to fixup"),
    ] {
        let mut repo = MockRepo::default();
        let mut app = AppState::default();

        let prepared = prepare_source(&mut repo, &mut app, &worktree_source(), label).unwrap();

        assert!(prepared.is_none());
        assert_eq!(app.status.message.as_deref(), Some(expected));
        assert!(app.status.is_error);
    }
}

/// A lift that fails keeps the underlying cause, which is the half that says
/// what actually went wrong.
#[test]
fn a_failed_lift_reports_the_underlying_cause() {
    let mut repo = MockRepo {
        lift: LiftOutcome::Error,
        ..Default::default()
    };
    let mut app = AppState::default();

    assert!(
        prepare_source(&mut repo, &mut app, &worktree_source(), "Squash")
            .unwrap()
            .is_none()
    );
    let message = app.status.message.unwrap();
    assert!(message.starts_with("Squash failed:"), "got {message:?}");
    assert!(
        message.contains("the index has conflicts"),
        "got {message:?}"
    );
}

/// The tripwire itself: a way out of a squash that names no ending is caught
/// where it is written, rather than by a user finding a temporary commit on
/// their branch.
#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "dropped without an ending")]
fn a_prepared_source_that_names_no_ending_trips() {
    let mut repo = MockRepo {
        lift: LiftOutcome::Lifted,
        ..Default::default()
    };
    let mut app = AppState::default();

    let _prepared = prepare_source(&mut repo, &mut app, &worktree_source(), "Squash")
        .unwrap()
        .unwrap();
}

/// Giving up on a lifted row unwinds the temporary commit rather than the
/// stash that was never taken.
#[test]
fn abandoning_a_lifted_row_unwinds_the_temporary_commit() {
    let mut repo = MockRepo {
        lift: LiftOutcome::Lifted,
        ..Default::default()
    };
    let mut app = AppState::default();
    let prepared = prepare_source(&mut repo, &mut app, &worktree_source(), "Squash")
        .unwrap()
        .unwrap();

    let action = prepared.unwind(
        &mut repo,
        &mut app,
        "Squash failed: nope".to_string(),
        LoopAction::Continue,
    );

    assert!(matches!(action, LoopAction::Continue));
    assert_eq!(repo.restore_lifted_calls.get(), 1);
    assert_eq!(app.status.message.as_deref(), Some("Squash failed: nope"));
}

/// An unwind that itself fails leaves the row's changes in a commit on the
/// branch. Say so, and reload — otherwise the commit is there and invisible.
#[test]
fn a_failed_unwind_names_the_temporary_commit_and_reloads() {
    let mut repo = MockRepo {
        lift: LiftOutcome::Lifted,
        restore_lifted_ok: false,
        ..Default::default()
    };
    let mut app = AppState::default();
    let prepared = prepare_source(&mut repo, &mut app, &worktree_source(), "Squash")
        .unwrap()
        .unwrap();

    let action = prepared.unwind(
        &mut repo,
        &mut app,
        "Squash failed: nope".to_string(),
        LoopAction::Continue,
    );

    assert!(
        matches!(action, LoopAction::Reload),
        "the branch has a commit on it the list does not show"
    );
    let message = app.status.message.unwrap();
    assert!(
        message.contains("your changes are in a temporary commit on the branch"),
        "got {message:?}"
    );
    assert!(message.contains("ref is locked"), "got {message:?}");
}

/// Abandoning a commit source puts the auto-stash back instead.
#[test]
fn abandoning_a_commit_source_restores_the_autostash() {
    let mut repo = MockRepo::default();
    let mut app = AppState::default();
    let prepared = prepare_source(&mut repo, &mut app, &commit_source(), "Squash")
        .unwrap()
        .unwrap();

    let action = prepared.unwind(
        &mut repo,
        &mut app,
        "Squash failed: nope".to_string(),
        LoopAction::Proceed,
    );

    assert!(matches!(action, LoopAction::Proceed));
    assert_eq!(
        repo.restore_lifted_calls.get(),
        0,
        "there is no temporary commit to unwind"
    );
}

/// A squash from a commit joins the two messages; a row has none of its own, so
/// it starts from the target's alone rather than from a blank line under it.
#[test]
fn a_worktree_row_seeds_the_editor_with_the_target_message_alone() {
    assert_eq!(
        squash_editor_seed(&commit_source(), "the target commit"),
        "the target commit\n\nthe source commit"
    );
    assert_eq!(
        squash_editor_seed(&worktree_source(), "the target commit"),
        "the target commit"
    );
}

/// The report names what was folded in, which for a row is the row.
#[test]
fn a_completed_fold_names_what_it_folded_in() {
    assert_eq!(
        squash_success_message(&commit_source(), SquashMode::Squash),
        "Commit squashed"
    );
    assert_eq!(
        squash_success_message(&worktree_source(), SquashMode::Fixup),
        "Staged changes fixed up"
    );
    assert_eq!(
        squash_success_message(
            &SquashSource::Worktree(WorktreeSource::Unstaged),
            SquashMode::Squash
        ),
        "Unstaged changes squashed"
    );
}

/// The probe runs against the temporary commit and is given the message the
/// fold would keep.
#[test]
fn a_conflict_probe_from_a_row_is_reported_as_a_conflict() {
    let mut repo = MockRepo {
        lift: LiftOutcome::Lifted,
        squash_probe: SquashProbe::Conflict,
        ..Default::default()
    };
    let mut app = AppState::default();
    let prepared = prepare_source(&mut repo, &mut app, &worktree_source(), "Fixup")
        .unwrap()
        .unwrap();

    let probe = repo
        .squash_try_combine(
            prepared.source_oid(),
            &Oid::from("b".repeat(40)),
            "the target commit",
            SquashMode::Fixup,
            prepared.head_oid(),
        )
        .unwrap();

    assert!(probe.is_some());
    assert_eq!(
        repo.squash_probe_message.borrow().as_deref(),
        Some("the target commit")
    );
    prepared.discard();
}

/// A probe that fails carries its cause up, the same as a failed lift.
#[test]
fn a_failed_conflict_probe_reports_the_underlying_cause() {
    let repo = MockRepo {
        squash_probe: SquashProbe::Error,
        ..Default::default()
    };

    let error = repo
        .squash_try_combine(
            &Oid::from("b".repeat(40)),
            &Oid::from("c".repeat(40)),
            "the target commit",
            SquashMode::Fixup,
            &Oid::from("a".repeat(40)),
        )
        .unwrap_err();

    assert_eq!(
        format!("{error:#}"),
        "failed to combine the two commits: tree is unmergeable"
    );
}
