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

use bstr::ByteSlice;
use common::prelude::*;
use git_tailor::autofixup::{self, MessageOverrides};
use git_tailor::repo::UndoOutcome;

/// Final messages for the targets named by summary, set the way the
/// confirmation dialog sets them: through the planned groups.
fn overrides_for(
    git_repo: &impl GitRepo,
    head_oid: &Oid,
    base: git2::Oid,
    messages: &[(&str, bstr::BString)],
) -> MessageOverrides {
    let commits = git_repo.list_commits(head_oid, &Oid::from(base)).unwrap();
    let groups = autofixup::group_by_target(&autofixup::plan_autofixup(&commits).pairs);
    let mut overrides = MessageOverrides::default();
    for (summary, message) in messages {
        let group = groups
            .iter()
            .find(|g| g.target_summary == *summary)
            .unwrap_or_else(|| panic!("no autofixup target named {summary:?}"));
        overrides.set(group, message.clone());
    }
    overrides
}

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

    let mut git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    let outcome =
        common::autofixup_as_shown(&mut git_repo, &head_oid, base, &Default::default()).unwrap();
    assert_rebase_complete!(outcome);

    assert_history!(&test, base, &["Add target line"]);
    assert_file_contents_at_head!(&test.repo, "a.txt", "base\ntarget\nfix1\nfix2\n");
}

/// The commit a fixup of a fixup names is gone by the time it runs, folded into
/// the target the first fixup was aimed at — so that is where it goes too.
#[test]
fn a_fixup_of_a_fixup_folds_into_the_original_target() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "base\n", "base");
    test.commit_file("a.txt", "base\ntarget\n", "Add target line");
    test.commit_file("b.txt", "other\n", "Unrelated");
    test.commit_file("a.txt", "base\ntarget\nfix1\n", "fixup! Add target line");
    test.commit_file(
        "a.txt",
        "base\ntarget\nfix1\nfix2\n",
        "fixup! fixup! Add target line",
    );

    let mut git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();
    let outcome =
        common::autofixup_as_shown(&mut git_repo, &head_oid, base, &Default::default()).unwrap();
    assert_rebase_complete!(outcome);

    assert_history!(&test, base, &["Add target line", "Unrelated"]);
    assert_file_contents_at_head!(&test.repo, "a.txt", "base\ntarget\nfix1\nfix2\n");
}

/// Of two commits with the same summary, a fixup reaches the later one only by
/// its hash — and must still find it after an earlier step rewrote it.
#[test]
fn a_fixup_named_by_hash_finds_its_target_after_earlier_steps_rewrote_it() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "base\n", "base");
    test.commit_file("a.txt", "base\ntarget\n", "Add target line");
    test.commit_file("t1.txt", "first\n", "Tweak");
    let second = test.commit_file("t2.txt", "second\n", "Tweak");
    test.commit_file("a.txt", "base\ntarget\nfix\n", "fixup! Add target line");
    let short = &second.to_string()[..7];
    test.commit_file("t2.txt", "second\nfixed\n", &format!("fixup! {short}"));

    let mut git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();
    let outcome =
        common::autofixup_as_shown(&mut git_repo, &head_oid, base, &Default::default()).unwrap();
    assert_rebase_complete!(outcome);

    assert_history!(&test, base, &["Add target line", "Tweak", "Tweak"]);
    assert_file_contents_at_head!(&test.repo, "t2.txt", "second\nfixed\n");
    let commits = test.commits_from_head(base);
    let first_tweak = test.repo.find_commit(commits[1]).unwrap().tree().unwrap();
    assert!(
        first_tweak
            .get_path(std::path::Path::new("t2.txt"))
            .is_err(),
        "the fix belongs to the second Tweak, not the first"
    );
}

/// Two commits share a summary, and each has its own fixup, naming it by hash.
/// A message edited for one of them lands on that one only.
#[test]
fn a_message_edited_for_one_of_two_same_named_targets_lands_on_that_one() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "base\n", "base");
    let first = test.commit_file("t1.txt", "first\n", "Tweak");
    let second = test.commit_file("t2.txt", "second\n", "Tweak");
    let short = |oid: git2::Oid| oid.to_string()[..7].to_string();
    test.commit_file(
        "t1.txt",
        "first\nfixed\n",
        &format!("fixup! {}", short(first)),
    );
    test.commit_file(
        "t2.txt",
        "second\nfixed\n",
        &format!("fixup! {}", short(second)),
    );

    let mut git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();
    let commits = git_repo.list_commits(&head_oid, &Oid::from(base)).unwrap();
    let groups = autofixup::group_by_target(&autofixup::plan_autofixup(&commits).pairs);
    let second_group = groups
        .iter()
        .find(|g| g.target_oid == Oid::from(second))
        .expect("the hash names the second Tweak");
    let mut overrides = MessageOverrides::default();
    overrides.set(second_group, bstr::BString::from("Second tweak\n"));

    let outcome = common::autofixup_as_shown(&mut git_repo, &head_oid, base, &overrides).unwrap();
    assert_rebase_complete!(outcome);

    assert_history!(&test, base, &["Tweak", "Second tweak"]);
}

/// The merge-base is not on the branch, so a fixup never folds into it — even
/// though it is the oldest commit with the summary the fixup names.
#[test]
fn a_fixup_never_folds_into_the_merge_base() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "base\n", "Update deps");
    test.commit_file("a.txt", "base\nmore\n", "Update deps");
    test.commit_file("a.txt", "base\nmore\nfix\n", "fixup! Update deps");

    let mut git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();
    let outcome =
        common::autofixup_as_shown(&mut git_repo, &head_oid, base, &Default::default()).unwrap();
    assert_rebase_complete!(outcome);

    assert_history!(&test, base, &["Update deps"]);
    assert_file_contents_at_head!(&test.repo, "a.txt", "base\nmore\nfix\n");
}

/// A batch paused by a build that kept no plan cannot be resumed here, and the
/// refusal must come before the paused step is finished — or the older build,
/// which could have resumed it, finds that step already gone.
#[test]
fn resuming_a_batch_paused_without_a_plan_refuses_before_touching_anything() {
    let test = common::TestRepo::new();

    let base = test.commit_file("c.txt", "base\n", "base");
    test.commit_file("c.txt", "target version\n", "Add T");
    test.commit_file("c.txt", "mid version\n", "Unrelated edit to c");
    test.commit_file("c.txt", "source version\n", "fixup! Add T");

    let mut git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();
    let outcome =
        common::autofixup_as_shown(&mut git_repo, &head_oid, base, &Default::default()).unwrap();
    let state = expect_rebase_conflict!(outcome);
    let paused_tip = git_repo.head_oid().unwrap();

    test.write_file("c.txt", "mid version\n");
    git_repo.stage_file(std::path::Path::new("c.txt")).unwrap();
    let Resume::Squash(ctx) = &state.resume else {
        panic!("a three-way overwrite conflicts at squash-tree time");
    };
    let without_plan = git_tailor::repo::AutofixupContext {
        plan: Default::default(),
        landed: 0,
        ..state.autofixup_context.clone().unwrap()
    };
    let result = git_repo.squash_finalize(
        ctx,
        ctx.combined_message.as_bstr(),
        &state.original_branch_oid,
        Some(&without_plan),
    );

    assert!(result.is_err(), "expected a refusal, got {result:?}");
    assert_eq!(
        git_repo.head_oid().unwrap(),
        paused_tip,
        "the paused step must not have been finished"
    );
}

/// The plan comes from the caller's list. One naming a commit the range does
/// not hold is a caller out of step with the repository, reported as an error
/// rather than taking the program down with it.
#[test]
fn a_plan_naming_a_commit_outside_the_range_is_refused() {
    let test = common::TestRepo::new();

    let outside = test.commit_file("a.txt", "outside\n", "Add target line");
    let base = test.commit_file("a.txt", "base\n", "base");
    test.commit_file("a.txt", "base\nfix\n", "fixup! Add target line");

    let mut git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();
    let pair = autofixup::AutofixupPair {
        source_oid: head_oid.clone(),
        target_oid: Oid::from(outside),
        source_summary: "fixup! Add target line".to_string(),
        target_summary: "Add target line".to_string(),
        source_message: String::new(),
        target_message: String::new(),
        mode: git_tailor::app::SquashMode::Fixup,
    };

    let result = git_repo.autofixup(&head_oid, &Oid::from(base), &[pair], &Default::default());
    assert!(result.is_err(), "expected an error, got {result:?}");
}

/// The batch finds each step's commits by their place in the branch, which is
/// only well defined on a single line of history. A merge in the range it
/// rewrites is refused before anything lands, as reword and split refuse one,
/// rather than left to fail part-way in libgit2's words.
#[test]
fn a_merge_in_the_rewritten_range_is_refused_up_front() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "v1\n", "base");
    let target = test.commit_file("a.txt", "v2\n", "Add parser");
    let side = test.commit_file("b.txt", "b\n", "Side");
    let sig = git2::Signature::now("Test User", "test@example.com").unwrap();
    let side_commit = test.repo.find_commit(side).unwrap();
    let target_commit = test.repo.find_commit(target).unwrap();
    test.repo
        .commit(
            Some("HEAD"),
            &sig,
            &sig,
            "a merge",
            &side_commit.tree().unwrap(),
            &[&side_commit, &target_commit],
        )
        .unwrap();
    test.commit_file("a.txt", "v2\nfix\n", "fixup! Add parser");

    let mut git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();
    let result = common::autofixup_as_shown(&mut git_repo, &head_oid, base, &Default::default());

    let msg = format!(
        "{:#}",
        result.expect_err("a merge in the rewritten range must be refused")
    );
    assert!(!msg.contains("mainline"), "libgit2's wording leaked: {msg}");
    assert!(msg.to_lowercase().contains("merge"), "{msg}");
    assert_eq!(
        git_repo.head_oid().unwrap(),
        head_oid,
        "the branch must be left untouched"
    );
}

/// The range check starts above the oldest target, so that target being a merge
/// itself has to be refused on its own — before the steps below it land.
#[test]
fn a_merge_as_the_oldest_target_is_refused_up_front() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "v1\n", "base");
    let main_side = test.commit_file("a.txt", "v2\n", "Main side");
    let sig = git2::Signature::now("Test User", "test@example.com").unwrap();
    let base_commit = test.repo.find_commit(base).unwrap();
    test.write_file("s.txt", "s\n");
    test.stage_file("s.txt");
    let side_tree = test
        .repo
        .find_tree(test.repo.index().unwrap().write_tree().unwrap())
        .unwrap();
    let side = test
        .repo
        .commit(None, &sig, &sig, "Side", &side_tree, &[&base_commit])
        .unwrap();
    let main_commit = test.repo.find_commit(main_side).unwrap();
    let side_commit = test.repo.find_commit(side).unwrap();
    test.repo
        .commit(
            Some("HEAD"),
            &sig,
            &sig,
            "Merge side",
            &side_tree,
            &[&main_commit, &side_commit],
        )
        .unwrap();
    test.commit_file("b.txt", "b\n", "Add b");
    test.commit_file("b.txt", "b\nfix\n", "fixup! Add b");
    test.commit_file("m.txt", "m\n", "fixup! Merge side");

    let mut git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();
    let result = common::autofixup_as_shown(&mut git_repo, &head_oid, base, &Default::default());

    let msg = format!(
        "{:#}",
        result.expect_err("a merge as a target must be refused")
    );
    assert!(msg.to_lowercase().contains("merge"), "{msg}");
    assert_eq!(
        git_repo.head_oid().unwrap(),
        head_oid,
        "nothing may land before the refusal"
    );
}

#[test]
fn a_fixup_with_no_matching_target_is_left_in_place() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "base\n", "base");
    test.commit_file("a.txt", "base\ntarget\n", "Add target line");
    test.commit_file("b.txt", "orphan\n", "fixup! Nonexistent commit");

    let mut git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    let outcome =
        common::autofixup_as_shown(&mut git_repo, &head_oid, base, &Default::default()).unwrap();
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

    let mut git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    let outcome =
        common::autofixup_as_shown(&mut git_repo, &head_oid, base, &Default::default()).unwrap();
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

    let mut git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    let outcome =
        common::autofixup_as_shown(&mut git_repo, &head_oid, base, &Default::default()).unwrap();
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

    let mut git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    let outcome =
        common::autofixup_as_shown(&mut git_repo, &head_oid, base, &Default::default()).unwrap();
    let state = expect_rebase_conflict!(outcome);
    assert_eq!(
        state.original_branch_oid, head_oid,
        "conflict should carry the batch's true starting tip, not the mid-batch one"
    );
    assert!(state.autofixup_context.is_some());
    assert_eq!(
        state.operation_label, "Autofixup",
        "the dialog names the batch the user started, not the step it paused in"
    );

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
    git_repo.stage_file(std::path::Path::new("c.txt")).unwrap();
    let Resume::Squash(ctx) = &state.resume else {
        panic!("a three-way overwrite conflicts at squash-tree time");
    };
    let outcome = git_repo
        .squash_finalize(
            ctx,
            ctx.combined_message.as_bstr(),
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

    let mut git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    let overrides = overrides_for(
        &git_repo,
        &head_oid,
        base,
        &[(
            "Add target line",
            bstr::BString::from("Custom final message\n"),
        )],
    );
    let outcome = common::autofixup_as_shown(&mut git_repo, &head_oid, base, &overrides).unwrap();
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

    let mut git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    let overrides = overrides_for(
        &git_repo,
        &head_oid,
        base,
        &[(
            "Add target line",
            bstr::BString::from("Custom final message\n"),
        )],
    );
    let outcome = common::autofixup_as_shown(&mut git_repo, &head_oid, base, &overrides).unwrap();
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

    let mut git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    let overrides = overrides_for(
        &git_repo,
        &head_oid,
        base,
        &[(
            "Add target line",
            bstr::BString::from("Custom final message\n"),
        )],
    );
    let outcome = common::autofixup_as_shown(&mut git_repo, &head_oid, base, &overrides).unwrap();
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

    let mut git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    let overrides = overrides_for(
        &git_repo,
        &head_oid,
        base,
        &[(
            "Add target line",
            bstr::BString::from("Custom final message\n"),
        )],
    );
    let outcome = common::autofixup_as_shown(&mut git_repo, &head_oid, base, &overrides).unwrap();
    let state = expect_rebase_conflict!(outcome);

    // The override must have survived into the persisted conflict state —
    // this is the only copy of it once the confirmation dialog is gone.
    let ctx = state
        .autofixup_context
        .as_ref()
        .expect("an autofixup batch conflict carries its context");
    assert_eq!(ctx.message_overrides, overrides);

    // The single fixup for this target was the *last* (only) one queued, so
    // the override was already folded into the conflict's own combined
    // message on the first attempt — main.rs uses exactly this field
    // (skipping the editor for an autofixup batch) to finalize.
    let Resume::Squash(squash_ctx) = &state.resume else {
        panic!("a three-way overwrite conflicts at squash-tree time");
    };
    assert_eq!(
        squash_ctx.combined_message,
        b"Custom final message\n".to_vec()
    );

    test.write_file("a.txt", "base\nmid version\n");
    git_repo.stage_file(std::path::Path::new("a.txt")).unwrap();
    let outcome = git_repo
        .squash_finalize(
            squash_ctx,
            squash_ctx.combined_message.as_bstr(),
            &state.original_branch_oid,
            state.autofixup_context.as_ref(),
        )
        .unwrap();
    assert_rebase_complete!(outcome);

    // "Unrelated edit" sits between the target and the fixup, so it's
    // replayed as a descendant on top of the squashed (renamed) target.
    assert_history!(&test, base, &["Custom final message", "Unrelated edit"]);
}

/// The first pair lands; the second then refuses. The landed squash must still
/// be undoable, or the user is left with a rewrite they cannot get back from.
#[test]
fn an_error_after_a_pair_has_landed_still_records_undo() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "base\n", "base");
    test.commit_file("a.txt", "base\nT1\n", "Add T1");
    test.commit_file("a.txt", "base\nT1\nF1\n", "fixup! Add T1");
    test.commit_file("t.txt", "T2\n", "Add T2");
    test.commit_file("c.txt", "c\n", "Add c");
    // Folding this deletion into "Add T2" lets the replayed "Add c" bring
    // c.txt back, so the second pair's checkout runs into the untracked file.
    test.delete_file("c.txt", "fixup! Add T2");
    test.write_file("c.txt", "untracked\n");

    let mut git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    let result = common::autofixup_as_shown(&mut git_repo, &head_oid, base, &Default::default());
    assert!(result.is_err(), "expected a refusal, got {result:?}");
    assert_history!(&test, base, &["Add T1", "Add T2", "Add c", "fixup! Add T2"]);

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
            "Add c",
            "fixup! Add T2"
        ]
    );
}

#[test]
fn nothing_to_autofixup_is_a_clean_no_op() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "base\n", "base");
    test.commit_file("a.txt", "base\ntarget\n", "Add target line");

    let mut git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    let outcome =
        common::autofixup_as_shown(&mut git_repo, &head_oid, base, &Default::default()).unwrap();
    assert_rebase_complete!(outcome);
    assert_history!(&test, base, &["Add target line"]);
}

/// An edited autofixup message that is not valid UTF-8 must reach the commit
/// byte for byte.
///
/// The override used to be carried as a `String`, so the editor's bytes were
/// decoded lossily on the way in — a Latin-1 message came back with U+FFFD
/// where the user's characters had been. `String` could not even express this
/// input, which is why the bug had no test until the type changed.
#[test]
fn a_non_utf8_message_override_reaches_the_commit_unchanged() {
    let test = common::TestRepo::new();

    let base = test.commit_file("a.txt", "base\n", "base");
    test.commit_file("a.txt", "base\ntarget\n", "Add target line");
    test.commit_file("a.txt", "base\ntarget\nfix1\n", "fixup! Add target line");

    let mut git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();

    // Latin-1 "Fix för åäö handling": valid git, invalid UTF-8.
    let message = bstr::BString::from(&b"Fix f\xf6r \xe5\xe4\xf6 handling\n"[..]);
    let overrides = overrides_for(
        &git_repo,
        &head_oid,
        base,
        &[("Add target line", message.clone())],
    );

    assert_rebase_complete!(
        common::autofixup_as_shown(&mut git_repo, &head_oid, base, &overrides).unwrap()
    );

    let head = git_repo.head_oid().unwrap();
    assert_eq!(
        git_repo.commit_message_bytes(&head).unwrap(),
        message,
        "the bytes the user typed must be the bytes committed"
    );
}
