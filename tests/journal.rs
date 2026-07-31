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

//! Integration tests for the crash-safety operation journal.

#[allow(dead_code)]
mod common;

use common::prelude::*;
use git_tailor::repo::{InProgress, JournalStatus};

/// After an operation conflicts, the journal records the in-progress state and
/// pins the original tip — and a freshly opened repo handle (simulating a
/// restart) recovers it.
#[test]
fn conflict_records_journal_and_recovers_after_reopen() {
    let test = common::TestRepo::new();
    let state = test.make_drop_conflict();

    // The pin ref must exist so gc cannot prune the in-flight commits.
    assert!(
        test.repo.find_reference("refs/git-tailor/orig").is_ok(),
        "refs/git-tailor/orig should pin the original tip"
    );

    // Simulate a restart: a brand-new handle reads the on-disk journal.
    let git_repo = test.git_repo();
    match git_repo.read_journal().unwrap() {
        JournalStatus::Recovered(recovered) => {
            let InProgress::Conflict(recovered) = *recovered else {
                panic!("expected a recovered conflict, not an Edit");
            };
            assert_eq!(recovered.operation_label, "Drop");
            assert_eq!(recovered.original_branch_oid, state.original_branch_oid);
            assert_eq!(recovered.remaining_oids(), state.remaining_oids());
            assert_eq!(
                recovered.conflicting_commit_oid,
                state.conflicting_commit_oid
            );
        }
        other => panic!("expected Recovered, got {other:?}"),
    }
}

/// Resuming a recovered operation to completion clears the journal entirely
/// Resuming a recovered operation to completion clears the in-progress record
/// and the in-progress pin ref. (The journal file itself now persists to hold
/// the undo stack — crash detection keys off the in-progress marker, not the
/// file's existence, so `read_journal` still reports `None`.)
#[test]
fn resume_to_completion_clears_journal() {
    let test = common::TestRepo::new();
    test.commit_file("a.txt", "line1\n", "base");
    let to_drop = test.commit_file("a.txt", "line1\nline2\n", "add line2");
    let head = test.commit_file("a.txt", "line1\nline2\nline3\n", "add line3");

    let git_repo = test.git_repo();
    let state = expect_rebase_conflict!(
        git_repo
            .drop_commit(&Oid::from(to_drop), &Oid::from(head))
            .unwrap()
    );
    assert!(matches!(
        git_repo.read_journal().unwrap(),
        JournalStatus::Recovered(_)
    ));

    // Simulate the user resolving the conflict, then resume.
    test.write_file("a.txt", "line1\nline3\n");
    let mut index = test.repo.index().unwrap();
    index
        .conflict_remove(std::path::Path::new("a.txt"))
        .unwrap();
    index.add_path(std::path::Path::new("a.txt")).unwrap();
    index.write().unwrap();

    assert_rebase_complete!(git_repo.rebase_continue(&state).unwrap());

    assert!(matches!(
        git_repo.read_journal().unwrap(),
        JournalStatus::None
    ));
    assert!(
        test.repo.find_reference("refs/git-tailor/orig").is_err(),
        "in-progress pin ref should be gone after completion"
    );
}

/// Aborting a recovered operation restores the branch and clears the journal.
#[test]
fn abort_clears_journal_and_restores_branch() {
    let test = common::TestRepo::new();
    let state = test.make_drop_conflict();
    let git_repo = test.git_repo();

    git_repo.rebase_abort(&state).unwrap();

    let head = test.repo.head().unwrap().target().unwrap();
    assert_eq!(Oid::from(head), state.original_branch_oid);
    assert!(matches!(
        git_repo.read_journal().unwrap(),
        JournalStatus::None
    ));
    assert!(
        test.repo.find_reference("refs/git-tailor/orig").is_err(),
        "pin ref should be gone after abort"
    );
}

/// A journal written by a newer git-tailor is rejected (not parsed) and the
/// file is left untouched for that newer version.
#[test]
fn newer_version_is_rejected() {
    let test = common::TestRepo::new();
    write_raw_journal(&test, r#"{"version": 9999, "in_progress": null}"#);

    let git_repo = test.git_repo();
    match git_repo.read_journal().unwrap() {
        JournalStatus::NewerVersion(v) => assert_eq!(v, 9999),
        other => panic!("expected NewerVersion, got {other:?}"),
    }
    assert!(
        journal_file_exists(&test),
        "a newer-version journal must be preserved, not deleted"
    );
}

/// A malformed journal is reported as corrupt rather than crashing startup.
#[test]
fn malformed_journal_is_corrupt() {
    let test = common::TestRepo::new();
    write_raw_journal(&test, "{ this is not valid json");

    let git_repo = test.git_repo();
    assert!(matches!(
        git_repo.read_journal().unwrap(),
        JournalStatus::Corrupt(_)
    ));
}

/// No journal file means nothing to recover.
#[test]
fn absent_journal_is_none() {
    let test = common::TestRepo::new();
    test.commit_file("a.txt", "x\n", "c");

    let git_repo = test.git_repo();
    assert!(matches!(
        git_repo.read_journal().unwrap(),
        JournalStatus::None
    ));
}

/// A `ConflictState` carrying a `SquashContext` survives a JSON round-trip,
/// covering the nested-struct/enum serialization the journal relies on.
#[test]
fn conflict_state_with_squash_context_round_trips() {
    use git_tailor::app::SquashMode;
    use git_tailor::repo::{ConflictState, SquashContext};

    let state = ConflictState {
        operation_label: "Squash".into(),
        original_branch_oid: Oid::from("aaaaaaaa"),
        new_tip_oid: Oid::from("bbbbbbbb"),
        conflicting_commit_oid: Oid::from("cccccccc"),
        conflicting_files: vec!["a.txt".into()],
        resume: Resume::Squash(SquashContext {
            base_oid: Some(Oid::from("eeeeeeee")),
            source_oid: Oid::from("ffffffff"),
            target_oid: Oid::from("11111111"),
            combined_message: "combined".into(),
            descendant_oids: vec![Oid::from("22222222")],
            squash_mode: SquashMode::Fixup,
        }),
        ..Default::default()
    };

    let json = serde_json::to_string(&state).unwrap();
    let back: ConflictState = serde_json::from_str(&json).unwrap();
    assert_eq!(state, back);
}

/// A v1 journal with nothing paused upgrades losslessly: undo/redo/autostash
/// carry over and the file is rewritten to the current version.
#[test]
fn v1_journal_without_interrupted_op_migrates_and_keeps_undo() {
    let test = common::TestRepo::new();
    let v1 = r#"{
        "version": 1,
        "in_progress": null,
        "undo": [
            { "kind": "RefMove", "label": "Drop", "tip_before": "1111", "tip_after": "2222" }
        ],
        "redo": [],
        "autostash": null
    }"#;
    write_raw_journal(&test, v1);

    // Nothing paused, so nothing recovers — but the migration must run.
    let git_repo = test.git_repo();
    assert!(matches!(
        git_repo.read_journal().unwrap(),
        JournalStatus::None
    ));

    // The file was rewritten to the current version with the undo stack intact.
    let raw = std::fs::read_to_string(journal_path(&test)).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(doc["version"], 2);
    assert_eq!(
        doc["undo"].as_array().unwrap().len(),
        1,
        "the undo stack must survive migration"
    );
}

/// A v1 journal that held an operation interrupted mid-flight cannot be resumed
/// by this version. It is reported as `UpgradeInterrupted` and — crucially — the
/// file is left byte-for-byte untouched so an older git-tailor can still finish
/// the operation (see CHANGELOG).
#[test]
fn v1_journal_with_interrupted_op_is_flagged_and_file_left_untouched() {
    let test = common::TestRepo::new();
    let v1 = r#"{
        "version": 1,
        "in_progress": {
            "operation_label": "Squash",
            "original_branch_oid": "aaaa",
            "squash_context": { "source_oid": "bbbb" }
        },
        "undo": [
            { "kind": "RefMove", "label": "Drop", "tip_before": "1111", "tip_after": "2222" }
        ],
        "redo": [],
        "autostash": null
    }"#;
    write_raw_journal(&test, v1);
    let before = std::fs::read_to_string(journal_path(&test)).unwrap();

    let git_repo = test.git_repo();
    match git_repo.read_journal().unwrap() {
        JournalStatus::UpgradeInterrupted { op } => assert_eq!(op, "Squash"),
        other => panic!("expected UpgradeInterrupted, got {other:?}"),
    }

    // The v1 file must be preserved verbatim — not migrated, not rewritten — so
    // the previous git-tailor can still recover the paused operation.
    let after = std::fs::read_to_string(journal_path(&test)).unwrap();
    assert_eq!(
        before, after,
        "an interrupted v1 journal must be left untouched, not overwritten"
    );
}

// Helpers --------------------------------------------------------------------

fn journal_path(test: &common::TestRepo) -> std::path::PathBuf {
    test.repo.path().join("git-tailor").join("journal.json")
}

fn journal_file_exists(test: &common::TestRepo) -> bool {
    journal_path(test).exists()
}

fn write_raw_journal(test: &common::TestRepo, contents: &str) {
    let path = journal_path(test);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, contents).unwrap();
}
