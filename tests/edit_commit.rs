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

// Repository tests for the "Edit a commit" operation. The shell is spawned by
// main.rs; here we exercise the pure-git begin_edit/finish_edit backend by
// simulating the user's in-shell edits directly on the temp repo between the
// two calls.

#[allow(dead_code)]
mod common;

use common::TestRepo;
use git_tailor::Oid;
use git_tailor::repo::{EditOutcome, GitRepo};

/// A four-commit history: base(root) → c1 → edited → c3(head). `edited` is the
/// middle commit we exercise; its parent is c1 and its descendant is c3.
struct Fixture {
    test: TestRepo,
    base: git2::Oid,
    c1: git2::Oid,
    edited: git2::Oid,
    head: git2::Oid,
}

fn fixture() -> Fixture {
    let test = TestRepo::new();
    let base = test.commit_file("README", "readme\n", "base");
    let c1 = test.commit_file("a.txt", "a\n", "add a");
    let edited = test.commit_file("b.txt", "b\n", "add b");
    let head = test.commit_file("c.txt", "c\n", "add c");
    Fixture {
        test,
        base,
        c1,
        edited,
        head,
    }
}

/// Reset the current branch (and, for Hard, the working tree) to `oid`.
fn reset(test: &TestRepo, oid: git2::Oid, kind: git2::ResetType) {
    let obj = test.repo.find_object(oid, None).unwrap();
    test.repo.reset(&obj, kind, None).unwrap();
}

fn file_at(test: &TestRepo, commit: git2::Oid, path: &str) -> Option<String> {
    let tree = test.repo.find_commit(commit).unwrap().tree().unwrap();
    let entry = tree.get_path(std::path::Path::new(path)).ok()?;
    let blob = test.repo.find_blob(entry.id()).unwrap();
    Some(String::from_utf8_lossy(blob.content()).into_owned())
}

#[test]
fn edit_amend_replaces_commit_and_replays_descendants() {
    let f = fixture();
    let git_repo = f.test.git_repo();
    let head = Oid::from(f.head);

    git_repo.begin_edit(&Oid::from(f.edited), &head).unwrap();

    // Simulate: reset to the parent, re-author the commit with edited content.
    reset(&f.test, f.c1, git2::ResetType::Mixed);
    f.test.write_file("b.txt", "b EDITED\n");
    f.test.stage_file("b.txt");
    f.test.commit("add b (edited)");

    let outcome = git_repo.finish_edit(&Oid::from(f.edited)).unwrap();
    assert!(matches!(outcome, EditOutcome::Complete));

    let commits = f.test.commits_from_head(f.base);
    assert_eq!(commits.len(), 3, "c1 + edited-replacement + replayed c3");
    let tip = *commits.last().unwrap();
    assert_eq!(
        file_at(&f.test, tip, "b.txt").as_deref(),
        Some("b EDITED\n")
    );
    assert_eq!(file_at(&f.test, tip, "c.txt").as_deref(), Some("c\n"));
    assert_eq!(file_at(&f.test, tip, "a.txt").as_deref(), Some("a\n"));
}

#[test]
fn edit_can_split_into_several_commits() {
    let f = fixture();
    let git_repo = f.test.git_repo();

    git_repo
        .begin_edit(&Oid::from(f.edited), &Oid::from(f.head))
        .unwrap();

    // Simulate `git reset HEAD~` then re-commit the content in two pieces.
    reset(&f.test, f.c1, git2::ResetType::Mixed);
    f.test.write_file("b.txt", "b\n");
    f.test.stage_file("b.txt");
    f.test.commit("add b part 1");
    f.test.write_file("b2.txt", "b2\n");
    f.test.stage_file("b2.txt");
    f.test.commit("add b part 2");

    let outcome = git_repo.finish_edit(&Oid::from(f.edited)).unwrap();
    assert!(matches!(outcome, EditOutcome::Complete));

    let commits = f.test.commits_from_head(f.base);
    assert_eq!(commits.len(), 4, "c1 + two split pieces + replayed c3");
    let tip = *commits.last().unwrap();
    assert_eq!(file_at(&f.test, tip, "b.txt").as_deref(), Some("b\n"));
    assert_eq!(file_at(&f.test, tip, "b2.txt").as_deref(), Some("b2\n"));
    assert_eq!(file_at(&f.test, tip, "c.txt").as_deref(), Some("c\n"));
}

#[test]
fn edit_noop_cancels_and_restores_the_branch() {
    let f = fixture();
    let git_repo = f.test.git_repo();

    git_repo
        .begin_edit(&Oid::from(f.edited), &Oid::from(f.head))
        .unwrap();
    // Exit the shell without touching the commit.
    let outcome = git_repo.finish_edit(&Oid::from(f.edited)).unwrap();
    assert!(matches!(outcome, EditOutcome::Cancelled));

    assert_eq!(git_repo.head_oid().unwrap(), Oid::from(f.head));
    let commits = f.test.commits_from_head(f.base);
    assert_eq!(commits, vec![f.c1, f.edited, f.head]);
}

#[test]
fn edit_discarding_content_drops_the_commit() {
    let f = fixture();
    let git_repo = f.test.git_repo();

    git_repo
        .begin_edit(&Oid::from(f.edited), &Oid::from(f.head))
        .unwrap();
    // Reset to the parent and commit nothing — the edited content is discarded.
    reset(&f.test, f.c1, git2::ResetType::Hard);

    let outcome = git_repo.finish_edit(&Oid::from(f.edited)).unwrap();
    assert!(matches!(outcome, EditOutcome::Complete));

    let commits = f.test.commits_from_head(f.base);
    assert_eq!(commits.len(), 2, "c1 + replayed c3 (edited commit gone)");
    let tip = *commits.last().unwrap();
    assert!(file_at(&f.test, tip, "b.txt").is_none());
    assert_eq!(file_at(&f.test, tip, "c.txt").as_deref(), Some("c\n"));
}

#[test]
fn edit_undo_restores_the_original_branch() {
    let f = fixture();
    let git_repo = f.test.git_repo();

    git_repo
        .begin_edit(&Oid::from(f.edited), &Oid::from(f.head))
        .unwrap();
    reset(&f.test, f.c1, git2::ResetType::Mixed);
    f.test.write_file("b.txt", "b EDITED\n");
    f.test.stage_file("b.txt");
    f.test.commit("add b (edited)");
    git_repo.finish_edit(&Oid::from(f.edited)).unwrap();

    assert_ne!(git_repo.head_oid().unwrap(), Oid::from(f.head));
    git_repo.undo().unwrap();
    assert_eq!(git_repo.head_oid().unwrap(), Oid::from(f.head));
}

#[test]
fn edit_aborts_and_restores_on_a_merge_commit() {
    let f = fixture();
    let git_repo = f.test.git_repo();

    git_repo
        .begin_edit(&Oid::from(f.edited), &Oid::from(f.head))
        .unwrap();
    // Author a merge commit (two parents) on the branch.
    let edited_commit = f.test.repo.find_commit(f.edited).unwrap();
    let c1_commit = f.test.repo.find_commit(f.c1).unwrap();
    let sig = git2::Signature::now("Test User", "test@example.com").unwrap();
    let tree = edited_commit.tree().unwrap();
    f.test
        .repo
        .commit(
            Some("HEAD"),
            &sig,
            &sig,
            "a merge",
            &tree,
            &[&edited_commit, &c1_commit],
        )
        .unwrap();

    let result = git_repo.finish_edit(&Oid::from(f.edited));
    assert!(result.is_err(), "a merge commit must abort the edit");

    // The branch was restored to its original tip.
    assert_eq!(git_repo.head_oid().unwrap(), Oid::from(f.head));
    assert_eq!(
        f.test.commits_from_head(f.base),
        vec![f.c1, f.edited, f.head]
    );
}

#[test]
fn edit_aborts_and_restores_when_off_parent() {
    let f = fixture();
    let git_repo = f.test.git_repo();

    git_repo
        .begin_edit(&Oid::from(f.edited), &Oid::from(f.head))
        .unwrap();
    // Point the branch at the root — not a descendant of the edited commit's parent.
    reset(&f.test, f.base, git2::ResetType::Hard);

    let result = git_repo.finish_edit(&Oid::from(f.edited));
    assert!(result.is_err(), "an off-parent tip must abort the edit");

    assert_eq!(git_repo.head_oid().unwrap(), Oid::from(f.head));
    assert_eq!(
        f.test.commits_from_head(f.base),
        vec![f.c1, f.edited, f.head]
    );
}

#[test]
fn edit_descendant_conflict_returns_conflict() {
    let test = TestRepo::new();
    let base = test.commit_file("README", "r\n", "base");
    let _c1 = test.commit_file("a.txt", "a\n", "add a");
    let edited = test.commit_file("shared.txt", "line1\noriginal\nline3\n", "add shared");
    let head = test.commit_file(
        "shared.txt",
        "line1\ndescendant\nline3\n",
        "touch shared in descendant",
    );
    let _ = base;

    let git_repo = test.git_repo();
    git_repo
        .begin_edit(&Oid::from(edited), &Oid::from(head))
        .unwrap();

    // Re-author the edited commit, changing the same line the descendant touches.
    let c1_oid = test.repo.find_commit(edited).unwrap().parent_id(0).unwrap();
    reset(&test, c1_oid, git2::ResetType::Mixed);
    test.write_file("shared.txt", "line1\nedited-in-place\nline3\n");
    test.stage_file("shared.txt");
    test.commit("edit shared in place");

    let outcome = git_repo.finish_edit(&Oid::from(edited)).unwrap();
    assert!(
        matches!(outcome, EditOutcome::Conflict(_)),
        "replaying the descendant onto the edited line should conflict"
    );
}

#[test]
fn edit_crash_recovery_is_an_abortable_edit_state() {
    use git_tailor::repo::JournalStatus;
    let f = fixture();
    let git_repo = f.test.git_repo();
    git_repo
        .begin_edit(&Oid::from(f.edited), &Oid::from(f.head))
        .unwrap();

    // Simulate a crash mid-edit: a fresh handle sees the in-progress record.
    let recovered = f.test.git_repo();
    match recovered.read_journal().unwrap() {
        JournalStatus::Recovered(state) => {
            assert!(state.edit_context.is_some(), "recovered state is an Edit");
        }
        other => panic!("expected a recoverable Edit, got {other:?}"),
    }

    // Recovery restores the original branch and clears the journal.
    recovered.abort_edit().unwrap();
    assert_eq!(recovered.head_oid().unwrap(), Oid::from(f.head));
    assert_eq!(
        f.test.commits_from_head(f.base),
        vec![f.c1, f.edited, f.head]
    );
    assert!(matches!(
        recovered.read_journal().unwrap(),
        JournalStatus::None
    ));
}

#[test]
fn edit_with_uncommitted_changes_is_not_applied_and_preserves_the_work() {
    let f = fixture();
    let git_repo = f.test.git_repo();

    git_repo
        .begin_edit(&Oid::from(f.edited), &Oid::from(f.head))
        .unwrap();
    // The user edits a file in the shell but forgets to commit it, then exits.
    f.test.write_file("b.txt", "b WORK IN PROGRESS\n");

    let result = git_repo.finish_edit(&Oid::from(f.edited));
    assert!(result.is_err(), "a dirty tree must not be finished over");

    // Crucially, the uncommitted change is still on disk — nothing was discarded.
    let workdir = f.test.repo.workdir().unwrap();
    let content = std::fs::read_to_string(workdir.join("b.txt")).unwrap();
    assert_eq!(
        content, "b WORK IN PROGRESS\n",
        "the user's work was preserved"
    );

    // The edit is still in progress (branch left at the edited commit), so the
    // caller can re-open the shell rather than aborting.
    assert_eq!(git_repo.head_oid().unwrap(), Oid::from(f.edited));
    assert!(git_repo.is_worktree_dirty().unwrap());
}

#[test]
fn edit_refuses_a_dirty_working_tree() {
    let f = fixture();
    let git_repo = f.test.git_repo();

    f.test.write_file("a.txt", "dirty\n");
    f.test.stage_file("a.txt");

    let result = git_repo.begin_edit(&Oid::from(f.edited), &Oid::from(f.head));
    assert!(
        result.is_err(),
        "a dirty working tree must refuse begin_edit"
    );
}

#[test]
fn edit_the_root_commit_amends_and_replays_descendants() {
    let f = fixture();
    let git_repo = f.test.git_repo();

    // Edit the root commit (only reachable under --all).
    git_repo
        .begin_edit(&Oid::from(f.base), &Oid::from(f.head))
        .unwrap();

    // Re-author the root as a new orphan root with edited content.
    f.test.write_file("README", "readme EDITED\n");
    f.test.stage_file("README");
    let mut index = f.test.repo.index().unwrap();
    let tree = f.test.repo.find_tree(index.write_tree().unwrap()).unwrap();
    let sig = git2::Signature::now("Test User", "test@example.com").unwrap();
    // Build the new orphan root detached, then point the branch at it (git2
    // refuses to move HEAD onto an orphan whose parent isn't the current tip).
    let new_root = f
        .test
        .repo
        .commit(None, &sig, &sig, "base (edited)", &tree, &[])
        .unwrap();
    reset(&f.test, new_root, git2::ResetType::Hard);

    let outcome = git_repo.finish_edit(&Oid::from(f.base)).unwrap();
    assert!(matches!(outcome, EditOutcome::Complete));

    // The new root's edit stuck, and every descendant was replayed onto it.
    let commits = f.test.commits_from_head(new_root);
    assert_eq!(
        commits.len(),
        3,
        "c1 + edited + c3 replayed onto the new root"
    );
    let tip = *commits.last().unwrap();
    assert_eq!(
        file_at(&f.test, tip, "README").as_deref(),
        Some("readme EDITED\n")
    );
    assert_eq!(file_at(&f.test, tip, "a.txt").as_deref(), Some("a\n"));
    assert_eq!(file_at(&f.test, tip, "b.txt").as_deref(), Some("b\n"));
    assert_eq!(file_at(&f.test, tip, "c.txt").as_deref(), Some("c\n"));
}
