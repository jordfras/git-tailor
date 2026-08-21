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

#[allow(dead_code)]
mod common;

use git_tailor::{DiffLineKind, Oid, VirtualOid, repo::RepoRead};

#[test]
fn test_commit_diff_root_commit_all_additions() {
    let test = common::TestRepo::new();
    let c1 = test.commit_file("hello.txt", "Hello, world!\n", "Initial commit");

    let diff = test.git_repo().commit_diff(&Oid::from(c1), 3).unwrap();

    assert_eq!(diff.commit.oid, VirtualOid::Real(Oid::from(c1)));
    assert_eq!(diff.commit.summary, "Initial commit");
    assert_eq!(diff.files.len(), 1);

    let file = &diff.files[0];
    assert_eq!(file.new_path, Some("hello.txt".to_string()));
    assert_eq!(file.hunks.len(), 1);

    let hunk = &file.hunks[0];
    assert_eq!(hunk.old_start, 0);
    assert_eq!(hunk.old_lines, 0);
    assert_eq!(hunk.new_start, 1);
    assert_eq!(hunk.new_lines, 1);
    assert_eq!(hunk.lines.len(), 1);

    let line = &hunk.lines[0];
    assert!(matches!(line.kind, DiffLineKind::Addition));
    assert_eq!(line.content, "Hello, world!\n");
}

#[test]
fn test_commit_diff_file_modification() {
    let test = common::TestRepo::new();
    test.commit_file("file.txt", "line 1\nline 2\nline 3\n", "First");
    let c2 = test.commit_file("file.txt", "line 1\nmodified line 2\nline 3\n", "Modify");

    let diff = test.git_repo().commit_diff(&Oid::from(c2), 3).unwrap();

    assert_eq!(diff.commit.summary, "Modify");
    assert_eq!(diff.files.len(), 1);

    let file = &diff.files[0];
    assert_eq!(file.old_path, Some("file.txt".to_string()));
    assert_eq!(file.new_path, Some("file.txt".to_string()));
    assert_eq!(file.hunks.len(), 1);

    let hunk = &file.hunks[0];
    assert!(hunk.lines.len() >= 3);

    let has_deletion = hunk
        .lines
        .iter()
        .any(|l| matches!(l.kind, DiffLineKind::Deletion) && l.content.contains("line 2"));
    let has_addition = hunk
        .lines
        .iter()
        .any(|l| matches!(l.kind, DiffLineKind::Addition) && l.content.contains("modified line 2"));

    assert!(has_deletion, "Should have deletion of old line");
    assert!(has_addition, "Should have addition of new line");
}

#[test]
fn test_commit_diff_file_deletion() {
    let test = common::TestRepo::new();
    test.commit_file("to_delete.txt", "This will be deleted\n", "Add file");
    let c2 = test.delete_file("to_delete.txt", "Delete file");

    let diff = test.git_repo().commit_diff(&Oid::from(c2), 3).unwrap();

    assert_eq!(diff.commit.summary, "Delete file");
    assert_eq!(diff.files.len(), 1);

    let file = &diff.files[0];
    assert_eq!(file.old_path, Some("to_delete.txt".to_string()));
    assert_eq!(file.hunks.len(), 1);

    let hunk = &file.hunks[0];
    assert_eq!(hunk.old_lines, 1);
    assert_eq!(hunk.new_lines, 0);

    let all_deletions = hunk
        .lines
        .iter()
        .all(|l| matches!(l.kind, DiffLineKind::Deletion));
    assert!(all_deletions, "All lines should be deletions");
}

#[test]
fn test_commit_diff_multiple_files() {
    let test = common::TestRepo::new();
    test.commit_file("a.txt", "a\n", "First");

    test.write_file("b.txt", "b\n");
    test.write_file("c.txt", "c\n");
    test.stage_file("b.txt");
    test.stage_file("c.txt");
    let c2 = test.commit("Add two files");

    let diff = test.git_repo().commit_diff(&Oid::from(c2), 3).unwrap();

    assert_eq!(diff.commit.summary, "Add two files");
    assert_eq!(diff.files.len(), 2);

    let filenames: Vec<String> = diff
        .files
        .iter()
        .filter_map(|f| f.new_path.clone())
        .collect();
    assert!(filenames.contains(&"b.txt".to_string()));
    assert!(filenames.contains(&"c.txt".to_string()));
}

#[test]
fn test_commit_diff_multiple_hunks() {
    let test = common::TestRepo::new();

    // Create a file with clearly separated regions (10+ lines of context)
    let initial = "line 1\nline 2\nline 3\nkeep 1\nkeep 2\nkeep 3\nkeep 4\nkeep 5\nkeep 6\nkeep 7\nkeep 8\nkeep 9\nkeep 10\nline 20\nline 21\nline 22\n";
    test.commit_file("multi.txt", initial, "First");

    let modified = "MODIFIED 1\nline 2\nline 3\nkeep 1\nkeep 2\nkeep 3\nkeep 4\nkeep 5\nkeep 6\nkeep 7\nkeep 8\nkeep 9\nkeep 10\nline 20\nMODIFIED 21\nline 22\n";
    let c2 = test.commit_file("multi.txt", modified, "Modify two regions");

    let diff = test.git_repo().commit_diff(&Oid::from(c2), 3).unwrap();

    assert_eq!(diff.files.len(), 1);
    let file = &diff.files[0];

    // Should have 2 hunks (two separate modification regions with sufficient context between)
    assert_eq!(
        file.hunks.len(),
        2,
        "Should have 2 hunks for two separate changes"
    );
}

#[test]
fn test_commit_diff_metadata() {
    let test = common::TestRepo::new();
    let c1 = test.commit_file("test.txt", "content\n", "Test commit");

    let diff = test.git_repo().commit_diff(&Oid::from(c1), 3).unwrap();

    assert_eq!(diff.commit.oid, VirtualOid::Real(Oid::from(c1)));
    assert_eq!(diff.commit.summary, "Test commit");
    assert_eq!(diff.commit.author.as_deref(), Some("Test User"));
    assert!(diff.commit.date.as_deref().is_some_and(|d| !d.is_empty()));
    assert_eq!(diff.commit.parent_oids.len(), 0);
}

/// `commit_diff_for_fragmap` should detect renames via `find_similar`
/// and produce a single `FileDiff` with `old_path ≠ new_path`.
#[test]
fn test_commit_diff_for_fragmap_detects_rename() {
    let test = common::TestRepo::new();
    // Need an initial commit so the rename commit has a parent.
    test.commit_file("old_name.rs", "line 1\nline 2\nline 3\n", "initial");
    let c2 = test.rename_file(
        "old_name.rs",
        "new_name.rs",
        Some("line 1\nline 2 modified\nline 3\n"),
        "rename file",
    );

    let diff = test
        .git_repo()
        .commit_diff_for_fragmap(&Oid::from(c2))
        .unwrap();

    // find_similar should collapse the delete+add into a single renamed entry
    assert_eq!(
        diff.files.len(),
        1,
        "expected single file delta after rename"
    );
    let f = &diff.files[0];
    assert_eq!(f.old_path.as_deref(), Some("old_name.rs"));
    assert_eq!(f.new_path.as_deref(), Some("new_name.rs"));
}

#[test]
fn test_staged_diff_has_context_but_fragmap_variant_does_not() {
    let test = common::TestRepo::new();
    test.commit_file("file.txt", "a\nb\nc\nd\ne\n", "init");
    // Stage a change to the middle line.
    test.write_file("file.txt", "a\nb\nCHANGED\nd\ne\n");
    test.stage_file("file.txt");
    let repo = test.git_repo();

    let full = repo
        .staged_diff(3)
        .unwrap()
        .expect("staged changes present");
    let hunk = &full.files[0].hunks[0];
    assert!(
        hunk.lines
            .iter()
            .any(|l| matches!(l.kind, DiffLineKind::Context)),
        "the detail-view staged diff should include surrounding context"
    );

    let tight = repo
        .staged_diff_for_fragmap()
        .unwrap()
        .expect("staged changes present");
    let thunk = &tight.files[0].hunks[0];
    assert!(
        thunk
            .lines
            .iter()
            .all(|l| !matches!(l.kind, DiffLineKind::Context)),
        "the fragmap staged diff should have no context lines"
    );
}

#[test]
fn test_unstaged_diff_has_context_but_fragmap_variant_does_not() {
    let test = common::TestRepo::new();
    test.commit_file("file.txt", "a\nb\nc\nd\ne\n", "init");
    // Unstaged change to the middle line.
    test.write_file("file.txt", "a\nb\nCHANGED\nd\ne\n");
    let repo = test.git_repo();

    let full = repo
        .unstaged_diff(3)
        .unwrap()
        .expect("unstaged changes present");
    assert!(
        full.files[0].hunks[0]
            .lines
            .iter()
            .any(|l| matches!(l.kind, DiffLineKind::Context)),
        "the detail-view unstaged diff should include surrounding context"
    );

    let tight = repo
        .unstaged_diff_for_fragmap()
        .unwrap()
        .expect("unstaged changes present");
    assert!(
        tight.files[0].hunks[0]
            .lines
            .iter()
            .all(|l| !matches!(l.kind, DiffLineKind::Context)),
        "the fragmap unstaged diff should have no context lines"
    );
}

/// A hunkless delta — an empty file, a binary file, a mode-only change — still
/// belongs on the synthetic Staged/Unstaged row. Dropping it hides the whole row
/// when it is the only thing staged.
#[test]
fn test_staged_diff_shows_a_new_empty_file() {
    let test = common::TestRepo::new();
    test.commit_file("file.txt", "a\n", "init");
    test.write_file("empty.txt", "");
    test.stage_file("empty.txt");
    let repo = test.git_repo();

    let diff = repo
        .staged_diff(3)
        .unwrap()
        .expect("staging an empty file should produce a staged row");
    assert_eq!(diff.files.len(), 1);
    assert_eq!(diff.files[0].new_path.as_deref(), Some("empty.txt"));
    assert!(repo.staged_diff_for_fragmap().unwrap().is_some());
}

#[test]
fn test_staged_diff_shows_a_new_binary_file() {
    let test = common::TestRepo::new();
    test.commit_file("file.txt", "a\n", "init");
    std::fs::write(
        test.repo.workdir().unwrap().join("blob.bin"),
        b"bin\x00data\n",
    )
    .unwrap();
    test.stage_file("blob.bin");
    let repo = test.git_repo();

    let diff = repo
        .staged_diff(3)
        .unwrap()
        .expect("staging a binary file should produce a staged row");
    assert_eq!(diff.files.len(), 1);
    assert_eq!(diff.files[0].new_path.as_deref(), Some("blob.bin"));
    assert!(repo.staged_diff_for_fragmap().unwrap().is_some());
}

#[test]
fn test_unstaged_diff_shows_a_binary_file_change() {
    let test = common::TestRepo::new();
    let workdir = std::path::PathBuf::from(test.repo.workdir().unwrap());
    std::fs::write(workdir.join("blob.bin"), b"bin\x00data\n").unwrap();
    test.stage_file("blob.bin");
    test.commit("init");
    std::fs::write(workdir.join("blob.bin"), b"bin\x00other\n").unwrap();
    let repo = test.git_repo();

    let diff = repo
        .unstaged_diff(3)
        .unwrap()
        .expect("editing a binary file should produce an unstaged row");
    assert_eq!(diff.files.len(), 1);
    assert_eq!(diff.files[0].new_path.as_deref(), Some("blob.bin"));
    assert!(repo.unstaged_diff_for_fragmap().unwrap().is_some());
}

#[test]
fn test_staged_diff_is_none_when_nothing_is_staged() {
    let test = common::TestRepo::new();
    test.commit_file("file.txt", "a\n", "init");

    assert!(test.git_repo().staged_diff(3).unwrap().is_none());
}
