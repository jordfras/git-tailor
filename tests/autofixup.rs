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

//! Integration tests for bulk autofixup (`GitRepo::autofixup`).

#[allow(dead_code)]
mod common;

use common::prelude::*;
use git_tailor::repo::UndoOutcome;

#[test]
fn multiple_fixups_for_the_same_target_stack_correctly() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "base\n", "base");
    test.commit_file("a.txt", "base\ntarget\n", "Add target line");
    test.commit_file("a.txt", "base\ntarget\nfix1\n", "fixup! Add target line");
    test.commit_file(
        "a.txt",
        "base\ntarget\nfix1\nfix2\n",
        "fixup! Add target line",
    );

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    let outcome = git_repo
        .autofixup(&head_oid, &Oid::from(base), &Default::default())
        .unwrap();
    assert_rebase_complete!(outcome);

    assert_history!(&test, base, &["Add target line"]);
    assert_file_contents_at_head!(&test.repo, "a.txt", "base\ntarget\nfix1\nfix2\n");
}

#[test]
fn a_fixup_with_no_matching_target_is_left_in_place() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "base\n", "base");
    test.commit_file("a.txt", "base\ntarget\n", "Add target line");
    test.commit_file("b.txt", "orphan\n", "fixup! Nonexistent commit");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    let outcome = git_repo
        .autofixup(&head_oid, &Oid::from(base), &Default::default())
        .unwrap();
    assert_rebase_complete!(outcome);

    // No matching target: nothing to squash, both commits survive untouched.
    assert_history!(
        &test,
        base,
        &["Add target line", "fixup! Nonexistent commit"]
    );
}

#[test]
fn mixed_fixup_and_squash_prefixes() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "base\n", "base");
    test.commit_file("a.txt", "base\nparser\n", "Add parser");
    test.commit_file("b.txt", "lexer\n", "Add lexer");
    test.commit_file("a.txt", "base\nparser\nfix\n", "fixup! Add parser");
    test.commit_file("b.txt", "lexer\nextra\n", "squash! Add lexer");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    let outcome = git_repo
        .autofixup(&head_oid, &Oid::from(base), &Default::default())
        .unwrap();
    assert_rebase_complete!(outcome);

    assert_history!(&test, base, &["Add parser", "Add lexer"]);
    assert_file_contents_at_head!(&test.repo, "a.txt", "base\nparser\nfix\n");
    assert_file_contents_at_head!(&test.repo, "b.txt", "lexer\nextra\n");

    let commits = test.commits_from_head(base);
    // Fixup keeps the target's message unchanged.
    let parser_commit = test.repo.find_commit(commits[0]).unwrap();
    assert_eq!(parser_commit.message().unwrap(), "Add parser");
    // Squash combines target + source with the default (non-interactive) text.
    let lexer_commit = test.repo.find_commit(commits[1]).unwrap();
    assert_eq!(
        lexer_commit.message().unwrap(),
        "Add lexer\n\nsquash! Add lexer"
    );
}

#[test]
fn a_single_undo_entry_reverts_the_whole_batch() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "base\n", "base");
    test.commit_file("a.txt", "base\ntarget\n", "Add target line");
    test.commit_file("a.txt", "base\ntarget\nfix1\n", "fixup! Add target line");
    test.commit_file(
        "a.txt",
        "base\ntarget\nfix1\nfix2\n",
        "fixup! Add target line",
    );

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    let outcome = git_repo
        .autofixup(&head_oid, &Oid::from(base), &Default::default())
        .unwrap();
    assert_rebase_complete!(outcome);
    assert_history!(&test, base, &["Add target line"]);

    match git_repo.undo().unwrap() {
        UndoOutcome::Done { label } => assert_eq!(label, "Autofixup"),
        other => panic!("expected Done, got {other:?}"),
    }

    // A single undo restores every commit the batch squashed away.
    assert_history!(
        &test,
        base,
        &[
            "Add target line",
            "fixup! Add target line",
            "fixup! Add target line",
        ]
    );
}

#[test]
fn conflict_partway_through_a_batch_resumes_the_remaining_pairs_and_still_undoes_as_one() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "base\n", "base");
    // First pair: a clean append, squashes without conflict.
    test.commit_file("a.txt", "base\nT1\n", "Add T1");
    test.commit_file("a.txt", "base\nT1\nF1\n", "fixup! Add T1");
    // Second pair: three overwrites of the same line force a real conflict
    // when the fixup is cherry-picked onto its target (mirrors
    // squash_returns_conflict_when_source_and_target_conflict).
    test.commit_file("c.txt", "target version\n", "Add T2");
    test.commit_file("c.txt", "mid version\n", "Unrelated edit to c");
    test.commit_file("c.txt", "source version\n", "fixup! Add T2");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    let outcome = git_repo
        .autofixup(&head_oid, &Oid::from(base), &Default::default())
        .unwrap();
    let state = expect_rebase_conflict!(outcome);
    assert_eq!(
        state.original_branch_oid, head_oid,
        "conflict should carry the batch's true starting tip, not the mid-batch one"
    );
    assert!(state.autofixup_context.is_some());

    // The first pair (F1 -> T1) already squashed cleanly before the conflict;
    // T2's own descendants (the unrelated edit and the second fixup) are
    // still pending behind the paused conflict, not yet replayed.
    assert_history!(&test, base, &["Add T1", "Add T2"]);

    // Resolve the conflict and resume. This is a squash-time tree conflict
    // (source vs target), so — mirroring main.rs's dispatch — resolution
    // goes through `squash_finalize`, not the generic `rebase_continue`; an
    // autofixup batch always uses the non-interactive combined message
    // (no editor per pair), matching `pair_message`'s own default text.
    //
    // Resolve to exactly what "Unrelated edit to c" already set, so replaying
    // that descendant onto the squash commit is a clean no-op — otherwise it
    // would conflict *again* against an arbitrary resolution, which is a
    // second, independent conflict rather than a property of autofixup batching.
    test.write_file("c.txt", "mid version\n");
    git_repo.stage_file("c.txt").unwrap();
    let Resume::Squash(ctx) = &state.resume else {
        panic!("a three-way overwrite conflicts at squash-tree time");
    };
    let outcome = git_repo
        .squash_finalize(
            ctx,
            &ctx.combined_message,
            &state.original_branch_oid,
            state.autofixup_context.as_ref(),
        )
        .unwrap();
    assert_rebase_complete!(outcome);

    assert_history!(&test, base, &["Add T1", "Add T2", "Unrelated edit to c"]);
    assert_file_contents_at_head!(&test.repo, "a.txt", "base\nT1\nF1\n");
    assert_file_contents_at_head!(&test.repo, "c.txt", "mid version\n");

    // One undo unwinds the whole batch, including the pair that squashed
    // cleanly before the conflict was ever hit.
    match git_repo.undo().unwrap() {
        UndoOutcome::Done { label } => assert_eq!(label, "Autofixup"),
        other => panic!("expected Done, got {other:?}"),
    }
    assert_history!(
        &test,
        base,
        &[
            "Add T1",
            "fixup! Add T1",
            "Add T2",
            "Unrelated edit to c",
            "fixup! Add T2",
        ]
    );
}

#[test]
fn a_message_override_applies_to_the_final_message_of_a_single_fixup() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "base\n", "base");
    test.commit_file("a.txt", "base\ntarget\n", "Add target line");
    test.commit_file("a.txt", "base\ntarget\nfix1\n", "fixup! Add target line");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    let overrides = std::collections::HashMap::from([(
        "Add target line".to_string(),
        "Custom final message\n".to_string(),
    )]);
    let outcome = git_repo
        .autofixup(&head_oid, &Oid::from(base), &overrides)
        .unwrap();
    assert_rebase_complete!(outcome);

    assert_history!(&test, base, &["Custom final message"]);
    assert_file_contents_at_head!(&test.repo, "a.txt", "base\ntarget\nfix1\n");
}

#[test]
fn a_message_override_replaces_the_auto_combined_squash_text() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "base\n", "base");
    test.commit_file("a.txt", "base\ntarget\n", "Add target line");
    test.commit_file("a.txt", "base\ntarget\nfix1\n", "squash! Add target line");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    let overrides = std::collections::HashMap::from([(
        "Add target line".to_string(),
        "Custom final message\n".to_string(),
    )]);
    let outcome = git_repo
        .autofixup(&head_oid, &Oid::from(base), &overrides)
        .unwrap();
    assert_rebase_complete!(outcome);

    // Without the override this would be "Add target line\n\nsquash! Add
    // target line" (the default combined text) — the override replaces it
    // outright rather than being appended to it.
    assert_history!(&test, base, &["Custom final message"]);
}

#[test]
fn a_message_override_only_applies_once_every_fixup_for_the_target_has_folded_in() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "base\n", "base");
    test.commit_file("a.txt", "base\ntarget\n", "Add target line");
    test.commit_file("a.txt", "base\ntarget\nfix1\n", "fixup! Add target line");
    test.commit_file(
        "a.txt",
        "base\ntarget\nfix1\nfix2\n",
        "fixup! Add target line",
    );

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    let overrides = std::collections::HashMap::from([(
        "Add target line".to_string(),
        "Custom final message\n".to_string(),
    )]);
    let outcome = git_repo
        .autofixup(&head_oid, &Oid::from(base), &overrides)
        .unwrap();
    assert_rebase_complete!(outcome);

    // Both fixups still matched and folded in — applying the override to the
    // first (intermediate) step would have renamed the target before the
    // second fixup's re-scan could match it by summary text.
    assert_history!(&test, base, &["Custom final message"]);
    assert_file_contents_at_head!(&test.repo, "a.txt", "base\ntarget\nfix1\nfix2\n");
}

#[test]
fn a_message_override_survives_a_conflict_resume_and_applies_on_completion() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "base\n", "base");
    // Three overwrites of the same line force a real conflict when the
    // fixup is cherry-picked onto its target (mirrors
    // squash_returns_conflict_when_source_and_target_conflict).
    test.commit_file("a.txt", "base\ntarget version\n", "Add target line");
    test.commit_file("a.txt", "base\nmid version\n", "Unrelated edit");
    test.commit_file("a.txt", "base\nsource version\n", "fixup! Add target line");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    let overrides = std::collections::HashMap::from([(
        "Add target line".to_string(),
        "Custom final message\n".to_string(),
    )]);
    let outcome = git_repo
        .autofixup(&head_oid, &Oid::from(base), &overrides)
        .unwrap();
    let state = expect_rebase_conflict!(outcome);

    // The override must have survived into the persisted conflict state —
    // this is the only copy of it once the confirmation dialog is gone.
    let ctx = state
        .autofixup_context
        .as_ref()
        .expect("an autofixup batch conflict carries its context");
    assert_eq!(
        ctx.message_overrides
            .get("Add target line")
            .map(String::as_str),
        Some("Custom final message\n")
    );

    // The single fixup for this target was the *last* (only) one queued, so
    // the override was already folded into the conflict's own combined
    // message on the first attempt — main.rs uses exactly this field
    // (skipping the editor for an autofixup batch) to finalize.
    let Resume::Squash(squash_ctx) = &state.resume else {
        panic!("a three-way overwrite conflicts at squash-tree time");
    };
    assert_eq!(squash_ctx.combined_message, "Custom final message\n");

    test.write_file("a.txt", "base\nmid version\n");
    git_repo.stage_file("a.txt").unwrap();
    let outcome = git_repo
        .squash_finalize(
            squash_ctx,
            &squash_ctx.combined_message,
            &state.original_branch_oid,
            state.autofixup_context.as_ref(),
        )
        .unwrap();
    assert_rebase_complete!(outcome);

    // "Unrelated edit" sits between the target and the fixup, so it's
    // replayed as a descendant on top of the squashed (renamed) target.
    assert_history!(&test, base, &["Custom final message", "Unrelated edit"]);
}

#[test]
fn nothing_to_autofixup_is_a_clean_no_op() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "base\n", "base");
    test.commit_file("a.txt", "base\ntarget\n", "Add target line");

    let git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    let outcome = git_repo
        .autofixup(&head_oid, &Oid::from(base), &Default::default())
        .unwrap();
    assert_rebase_complete!(outcome);
    assert_history!(&test, base, &["Add target line"]);
}
