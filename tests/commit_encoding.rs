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
    test.repo
        .reference("refs/heads/master", oid, true, "raw commit")
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
