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

//! Commit messages that are not valid UTF-8.
//!
//! git stores a commit message as bytes and says so: `i18n.commitEncoding` and
//! the per-commit `encoding` header exist precisely because messages predate
//! everyone agreeing on UTF-8. Plenty of European history is Latin-1.
//!
//! git2 hands those back as an error rather than bytes, and reaching for
//! `unwrap_or("")` at each call site turns "I cannot read this" into "it says
//! nothing" — which a rewrite then writes back as the truth.

#[allow(dead_code)]
mod common;

use common::TestRepo;
use common::prelude::*;

/// Latin-1 "Fix för åäö handling" — valid git, invalid UTF-8.
const LATIN1_MESSAGE: &[u8] = b"Fix f\xf6r \xe5\xe4\xf6 handling\n";

/// Write a commit object by hand: git2's safe API cannot express a message
/// that is not UTF-8, which is the whole problem.
fn commit_with_raw_message(test: &TestRepo, parent: git2::Oid, message: &[u8]) -> git2::Oid {
    let tree = test.repo.find_commit(parent).unwrap().tree_id();
    let mut raw = Vec::new();
    raw.extend_from_slice(format!("tree {tree}\n").as_bytes());
    raw.extend_from_slice(format!("parent {parent}\n").as_bytes());
    raw.extend_from_slice(b"author T <t@e> 1700000000 +0000\n");
    raw.extend_from_slice(b"committer T <t@e> 1700000000 +0000\n");
    raw.extend_from_slice(b"encoding ISO-8859-1\n");
    raw.extend_from_slice(b"\n");
    raw.extend_from_slice(message);
    let oid = test
        .repo
        .odb()
        .unwrap()
        .write(git2::ObjectType::Commit, &raw)
        .unwrap();
    // Whatever branch the fixture is on, not a hardcoded name — otherwise this
    // lands on a sibling branch and later commits build on the wrong parent.
    let branch = test
        .repo
        .head()
        .unwrap()
        .resolve()
        .unwrap()
        .name()
        .unwrap()
        .to_string();
    test.repo
        .reference(&branch, oid, true, "raw commit")
        .unwrap();
    oid
}

fn message_bytes(test: &TestRepo, oid: git2::Oid) -> Vec<u8> {
    test.repo.find_commit(oid).unwrap().message_bytes().to_vec()
}

fn encoding(test: &TestRepo, oid: git2::Oid) -> Option<String> {
    test.repo
        .find_commit(oid)
        .unwrap()
        .message_encoding()
        .ok()
        .flatten()
        .map(String::from)
}

/// Dropping one commit replays the ones above it. Their messages are not
/// git-tailor's to edit, and a message it cannot read is still not empty.
#[test]
fn a_replayed_commit_keeps_a_non_utf8_message() {
    let test = common::TestRepo::new();
    test.commit_file("a.txt", "v1\n", "base");
    let to_drop = test.commit_file("b.txt", "b\n", "to drop");
    let head = commit_with_raw_message(&test, to_drop, LATIN1_MESSAGE);

    let mut git_repo = test.git_repo();
    assert_rebase_complete!(
        git_repo
            .drop_commit(&Oid::from(to_drop), &Oid::from(head))
            .unwrap()
    );

    let new_head = git2::Oid::from(&git_repo.head_oid().unwrap());
    assert_eq!(
        message_bytes(&test, new_head),
        LATIN1_MESSAGE.to_vec(),
        "the replayed commit's message must come through byte for byte"
    );
    assert_eq!(
        encoding(&test, new_head).as_deref(),
        Some("ISO-8859-1"),
        "and with the header that says how to read it"
    );
}

/// The same through a move, which replays over a reordered chain.
#[test]
fn a_moved_chain_keeps_a_non_utf8_message() {
    let test = common::TestRepo::new();
    test.commit_file("a.txt", "v1\n", "base");
    let first = test.commit_file("b.txt", "b\n", "first");
    let second = test.commit_file("c.txt", "c\n", "second");
    let head = commit_with_raw_message(&test, second, LATIN1_MESSAGE);

    let mut git_repo = test.git_repo();
    assert_rebase_complete!(
        git_repo
            .move_commit(
                &Oid::from(first),
                Some(&Oid::from(second)),
                &Oid::from(head)
            )
            .unwrap()
    );

    let new_head = git2::Oid::from(&git_repo.head_oid().unwrap());
    assert_eq!(message_bytes(&test, new_head), LATIN1_MESSAGE.to_vec());
}

/// Reading such a history must work at all. Erroring out means git-tailor
/// simply refuses to open the repository.
#[test]
fn such_a_history_can_be_listed_and_read() {
    let test = common::TestRepo::new();
    let base = test.commit_file("a.txt", "v1\n", "base");
    let mid = test.commit_file("b.txt", "b\n", "mid");
    let head = commit_with_raw_message(&test, mid, LATIN1_MESSAGE);

    let git_repo = test.git_repo();
    let commits = git_repo
        .list_commits(&Oid::from(head), &Oid::from(base))
        .expect("a history with a Latin-1 message must still list");
    assert_eq!(commits.len(), 3, "no commit may go missing: {commits:?}");

    git_repo
        .commit_diff(&Oid::from(head), 3)
        .expect("and its diff must still open");
}

/// Splitting does not copy a message, it *derives* one — "summary (1/3)". That
/// has to go through a `&str`, and the only `&str` available is the lossy one,
/// which would bake replacement characters into the new commits.
///
/// Replaying is safe because the bytes pass through untouched; deriving is not,
/// so it refuses and says why. Reword is the way out: give the commit a message
/// git-tailor can read, then split it.
#[test]
fn splitting_a_commit_whose_message_is_not_utf8_is_refused() {
    let test = common::TestRepo::new();
    test.commit_file("a.txt", "v1\n", "base");
    let parent = test.commit_file("b.txt", "b\n", "parent");
    let to_split = commit_with_raw_message(&test, parent, LATIN1_MESSAGE);

    let mut git_repo = test.git_repo();
    let before = git_repo.head_oid().unwrap();
    let result = git_repo.split_commit_per_file(&Oid::from(to_split), &Oid::from(to_split));

    let error = format!("{:#}", result.expect_err("splitting must refuse"));
    assert!(
        error.contains("message"),
        "the refusal must name the reason: {error}"
    );
    assert_eq!(
        git_repo.head_oid().unwrap(),
        before,
        "and nothing may have moved"
    );
}

/// A fixup keeps the target's message, so it is the target's *bytes* that have
/// to come through — not the lossy rendering the list draws.
#[test]
fn a_fixup_keeps_the_targets_non_utf8_message() {
    let test = common::TestRepo::new();
    test.commit_file("a.txt", "1\n", "base");
    let parent = test.commit_file("a.txt", "1\n2\n", "parent");
    let target = commit_with_raw_message(&test, parent, LATIN1_MESSAGE);
    let source = test.commit_file("b.txt", "b\n", "source to fold in");

    let mut git_repo = test.git_repo();
    // What a fixup passes: the target's message, read from the repository.
    let target_message = git_repo.commit_message_bytes(&Oid::from(target)).unwrap();
    assert_eq!(target_message, LATIN1_MESSAGE.to_vec());

    assert_rebase_complete!(
        git_repo
            .squash_commits(
                &Oid::from(source),
                &Oid::from(target),
                &target_message,
                &Oid::from(source),
            )
            .unwrap()
    );

    let new_head = git2::Oid::from(&git_repo.head_oid().unwrap());
    assert_eq!(
        message_bytes(&test, new_head),
        LATIN1_MESSAGE.to_vec(),
        "the folded commit must keep the target's message byte for byte"
    );
    assert_eq!(
        encoding(&test, new_head).as_deref(),
        Some("ISO-8859-1"),
        "and the header describing those bytes"
    );
}

/// Bulk autofixup builds its default message from the same commit list the
/// summary-matching planner uses to find pairs — which is lossy by design,
/// since it is otherwise only ever shown, never written. The fixup/squash
/// commit's *summary* still has to be plain text for matching to work, but a
/// squash also folds in the rest of its message, which is free to be raw
/// bytes.
#[test]
fn bulk_autofixup_keeps_a_squash_sources_non_utf8_body() {
    let test = common::TestRepo::new();
    let base = test.commit_file("a.txt", "1\n", "base");
    let target = test.commit_file("a.txt", "1\n2\n", "Add target line");
    let source_message: &[u8] = b"squash! Add target line\n\nBody f\xf6r \xe5\xe4\xf6.\n";
    let source = commit_with_raw_message(&test, target, source_message);

    let mut git_repo = test.git_repo();
    let head_oid = git_repo.head_oid().unwrap();
    assert_eq!(head_oid, Oid::from(source));

    let outcome = git_repo
        .autofixup(&head_oid, &Oid::from(base), &Default::default())
        .unwrap();
    assert_rebase_complete!(outcome);

    let new_head = git2::Oid::from(&git_repo.head_oid().unwrap());
    let combined = message_bytes(&test, new_head);
    assert!(
        combined
            .windows(source_message.len())
            .any(|w| w == source_message),
        "the folded commit must carry the source's body byte for byte: {combined:?}"
    );
}

/// Rewording *to* readable text drops the `encoding` header, because the header
/// described bytes that are no longer there.
#[test]
fn rewording_to_utf8_drops_the_stale_encoding_header() {
    let test = common::TestRepo::new();
    test.commit_file("a.txt", "1\n", "base");
    let parent = test.commit_file("a.txt", "1\n2\n", "parent");
    let to_reword = commit_with_raw_message(&test, parent, LATIN1_MESSAGE);

    let mut git_repo = test.git_repo();
    git_repo
        .reword_commit(
            &Oid::from(to_reword),
            "plain ascii now\n".as_bytes(),
            &Oid::from(to_reword),
        )
        .unwrap();

    let new_head = git2::Oid::from(&git_repo.head_oid().unwrap());
    assert_eq!(
        message_bytes(&test, new_head),
        b"plain ascii now\n".to_vec()
    );
    assert_eq!(
        encoding(&test, new_head),
        None,
        "UTF-8 is git's default and needs no header"
    );
}

/// A fixup that keeps the target's message unchanged must keep its `encoding`
/// header too, even when those exact bytes also happen to be well-formed
/// UTF-8. The header describes how the *original* bytes were meant to be
/// read; whether they can *also* be parsed a different way is not something a
/// fixup that never touches the message gets to decide.
#[test]
fn a_fixup_keeps_the_targets_encoding_header_even_when_it_reads_as_utf8() {
    // Latin-1 "Café fix": 0xC3 0xA9 is also a well-formed UTF-8 encoding of
    // U+00E9, so this message is valid UTF-8 despite its declared encoding.
    const AMBIGUOUS_MESSAGE: &[u8] = b"Caf\xc3\xa9 fix\n";
    assert!(std::str::from_utf8(AMBIGUOUS_MESSAGE).is_ok());

    let test = common::TestRepo::new();
    test.commit_file("a.txt", "1\n", "base");
    let parent = test.commit_file("a.txt", "1\n2\n", "parent");
    let target = commit_with_raw_message(&test, parent, AMBIGUOUS_MESSAGE);
    let source = test.commit_file("b.txt", "b\n", "source to fold in");

    let mut git_repo = test.git_repo();
    let target_message = git_repo.commit_message_bytes(&Oid::from(target)).unwrap();

    assert_rebase_complete!(
        git_repo
            .squash_commits(
                &Oid::from(source),
                &Oid::from(target),
                &target_message,
                &Oid::from(source),
            )
            .unwrap()
    );

    let new_head = git2::Oid::from(&git_repo.head_oid().unwrap());
    assert_eq!(message_bytes(&test, new_head), AMBIGUOUS_MESSAGE.to_vec());
    assert_eq!(
        encoding(&test, new_head).as_deref(),
        Some("ISO-8859-1"),
        "the message never changed, so neither should how it's read"
    );
}

/// And rewording while keeping unreadable bytes keeps the header.
#[test]
fn rewording_within_latin1_keeps_the_encoding_header() {
    let test = common::TestRepo::new();
    test.commit_file("a.txt", "1\n", "base");
    let parent = test.commit_file("a.txt", "1\n2\n", "parent");
    let to_reword = commit_with_raw_message(&test, parent, LATIN1_MESSAGE);

    let edited: &[u8] = b"Ny rubrik f\xf6r \xe5\xe4\xf6\n";
    let mut git_repo = test.git_repo();
    git_repo
        .reword_commit(&Oid::from(to_reword), edited, &Oid::from(to_reword))
        .unwrap();

    let new_head = git2::Oid::from(&git_repo.head_oid().unwrap());
    assert_eq!(message_bytes(&test, new_head), edited.to_vec());
    assert_eq!(encoding(&test, new_head).as_deref(), Some("ISO-8859-1"));
}
