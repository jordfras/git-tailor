mod common;

use git2::Signature;
use git_tailor::{repo::GitRepo, DiffLineKind};
use std::fs;

#[test]
fn test_commit_diff_root_commit_all_additions() {
    let test = common::TestRepo::new();
    let c1 = test.commit_file("hello.txt", "Hello, world!\n", "Initial commit");

    let diff = test.git_repo().commit_diff(&c1.to_string()).unwrap();

    assert_eq!(diff.commit.oid, c1.to_string());
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

    let diff = test.git_repo().commit_diff(&c2.to_string()).unwrap();

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

    let diff = test.git_repo().commit_diff(&c2.to_string()).unwrap();

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

    let mut index = test.repo.index().unwrap();
    let repo_path = test.repo.workdir().unwrap();

    fs::write(repo_path.join("b.txt"), "b\n").unwrap();
    fs::write(repo_path.join("c.txt"), "c\n").unwrap();

    index.add_path(std::path::Path::new("b.txt")).unwrap();
    index.add_path(std::path::Path::new("c.txt")).unwrap();
    index.write().unwrap();

    let tree_oid = index.write_tree().unwrap();
    let tree = test.repo.find_tree(tree_oid).unwrap();
    let sig = Signature::now("Test User", "test@example.com").unwrap();
    let parent = test
        .repo
        .find_commit(test.repo.head().unwrap().target().unwrap())
        .unwrap();

    let c2 = test
        .repo
        .commit(Some("HEAD"), &sig, &sig, "Add two files", &tree, &[&parent])
        .unwrap();

    let diff = test.git_repo().commit_diff(&c2.to_string()).unwrap();

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

    let diff = test.git_repo().commit_diff(&c2.to_string()).unwrap();

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

    let diff = test.git_repo().commit_diff(&c1.to_string()).unwrap();

    assert_eq!(diff.commit.oid, c1.to_string());
    assert_eq!(diff.commit.summary, "Test commit");
    assert_eq!(diff.commit.author, "Test User");
    assert!(!diff.commit.date.is_empty());
    assert_eq!(diff.commit.parent_oids.len(), 0);
}
