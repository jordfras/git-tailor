use git2::{Repository, Signature};
use git_scissors::list_commits_in;
use std::fs;
use tempfile::TempDir;

/// Helper to create a test repository with a basic commit history
struct TestRepo {
    _temp_dir: TempDir,
    repo: Repository,
}

impl TestRepo {
    fn new() -> Self {
        let temp_dir = TempDir::new().unwrap();
        let repo = Repository::init(temp_dir.path()).unwrap();

        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Test User").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();

        Self {
            _temp_dir: temp_dir,
            repo,
        }
    }

    fn commit_file(&self, path: &str, content: &str, message: &str) -> git2::Oid {
        let repo_path = self.repo.workdir().unwrap();
        let file_path = repo_path.join(path);

        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }

        fs::write(&file_path, content).unwrap();

        let mut index = self.repo.index().unwrap();
        index.add_path(std::path::Path::new(path)).unwrap();
        index.write().unwrap();

        let tree_oid = index.write_tree().unwrap();
        let tree = self.repo.find_tree(tree_oid).unwrap();

        let sig = Signature::now("Test User", "test@example.com").unwrap();

        let parent_commit = if let Ok(head) = self.repo.head() {
            Some(self.repo.find_commit(head.target().unwrap()).unwrap())
        } else {
            None
        };

        let parents: Vec<&git2::Commit> = parent_commit.iter().collect();

        self.repo
            .commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
            .unwrap()
    }
}

#[test]
fn test_list_commits_returns_oldest_to_newest() {
    let test = TestRepo::new();

    let c1 = test.commit_file("file.txt", "first", "First commit");
    let c2 = test.commit_file("file.txt", "second", "Second commit");
    let c3 = test.commit_file("file.txt", "third", "Third commit");

    let c1_str = c1.to_string();
    let c2_str = c2.to_string();
    let c3_str = c3.to_string();

    let repo_path = test.repo.workdir().unwrap().to_str().unwrap();
    let commits = list_commits_in(repo_path, &c3_str, &c1_str).unwrap();

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
    let test = TestRepo::new();

    let c1 = test.commit_file("file.txt", "content", "Single commit");
    let c1_str = c1.to_string();

    let repo_path = test.repo.workdir().unwrap().to_str().unwrap();
    let commits = list_commits_in(repo_path, &c1_str, &c1_str).unwrap();

    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].oid, c1_str);
    assert_eq!(commits[0].summary, "Single commit");
}

#[test]
fn test_list_commits_metadata() {
    let test = TestRepo::new();

    let c1 = test.commit_file("file.txt", "initial", "Initial commit");
    let c2 = test.commit_file("file.txt", "updated", "Update commit");

    let c1_str = c1.to_string();
    let c2_str = c2.to_string();

    let repo_path = test.repo.workdir().unwrap().to_str().unwrap();
    let commits = list_commits_in(repo_path, &c2_str, &c1_str).unwrap();

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
    let test = TestRepo::new();

    let c1 = test.commit_file("file.txt", "first", "First");
    let c2 = test.commit_file("file.txt", "second", "Second");
    let _c3 = test.commit_file("file.txt", "third", "Third");

    let c1_str = c1.to_string();
    let c2_str = c2.to_string();

    let repo_path = test.repo.workdir().unwrap().to_str().unwrap();
    let commits = list_commits_in(repo_path, "HEAD", &c1_str).unwrap();

    assert_eq!(commits.len(), 3);
    assert_eq!(commits[0].oid, c1_str);
    assert_eq!(commits[1].oid, c2_str);
}
