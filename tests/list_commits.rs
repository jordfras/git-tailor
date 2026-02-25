mod common;

use git_tailor::repo::GitRepo;

#[test]
fn test_list_commits_returns_oldest_to_newest() {
    let test = common::TestRepo::new();

    let c1 = test.commit_file("file.txt", "first", "First commit");
    let c2 = test.commit_file("file.txt", "second", "Second commit");
    let c3 = test.commit_file("file.txt", "third", "Third commit");

    let c1_str = c1.to_string();
    let c2_str = c2.to_string();
    let c3_str = c3.to_string();

    let commits = test.git_repo().list_commits(&c3_str, &c1_str).unwrap();

    assert_eq!(commits.len(), 3);
    assert_eq!(commits[0].oid, c1_str);
    assert_eq!(commits[1].oid, c2_str);
    assert_eq!(commits[2].oid, c3_str);

    assert_eq!(commits[0].summary, "First commit");
    assert_eq!(commits[1].summary, "Second commit");
    assert_eq!(commits[2].summary, "Third commit");
}

#[test]
fn test_list_commits_with_same_commit() {
    let test = common::TestRepo::new();

    let c1 = test.commit_file("file.txt", "content", "Single commit");
    let c1_str = c1.to_string();

    let commits = test.git_repo().list_commits(&c1_str, &c1_str).unwrap();

    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].oid, c1_str);
    assert_eq!(commits[0].summary, "Single commit");
}

#[test]
fn test_list_commits_metadata() {
    let test = common::TestRepo::new();

    let c1 = test.commit_file("file.txt", "initial", "Initial commit");
    let c2 = test.commit_file("file.txt", "updated", "Update commit");

    let c1_str = c1.to_string();
    let c2_str = c2.to_string();

    let commits = test.git_repo().list_commits(&c2_str, &c1_str).unwrap();

    assert_eq!(commits.len(), 2);

    assert_eq!(commits[0].author, "Test User");
    assert!(!commits[0].date.is_empty());
    assert_eq!(commits[0].parent_oids.len(), 0);

    assert_eq!(commits[1].author, "Test User");
    assert!(!commits[1].date.is_empty());
    assert_eq!(commits[1].parent_oids.len(), 1);
    assert_eq!(commits[1].parent_oids[0], c1_str);
}

#[test]
fn test_list_commits_with_branch_name() {
    let test = common::TestRepo::new();

    let c1 = test.commit_file("file.txt", "first", "First");
    let c2 = test.commit_file("file.txt", "second", "Second");
    let _c3 = test.commit_file("file.txt", "third", "Third");

    let c1_str = c1.to_string();
    let c2_str = c2.to_string();

    let commits = test.git_repo().list_commits("HEAD", &c1_str).unwrap();

    assert_eq!(commits.len(), 3);
    assert_eq!(commits[0].oid, c1_str);
    assert_eq!(commits[1].oid, c2_str);
}
