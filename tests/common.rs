use git2::{Repository, Signature};
use git_tailor::repo::Git2Repo;
use std::fs;
use tempfile::TempDir;

/// Shared git repository fixture for integration tests.
///
/// Keeps a `git2::Repository` for low-level setup (creating commits, branches,
/// tags) and exposes [`git_repo()`][TestRepo::git_repo] to obtain a `Git2Repo`
/// handle for calling library functions under test.
pub struct TestRepo {
    pub _temp_dir: TempDir,
    pub repo: Repository,
}

impl TestRepo {
    pub fn new() -> Self {
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

    /// Open a `Git2Repo` handle to this repository for use with library functions.
    ///
    /// Each call opens a fresh handle to the same on-disk repository, which is
    /// valid — libgit2 supports multiple concurrent handles to the same repo.
    #[allow(dead_code)]
    pub fn git_repo(&self) -> Git2Repo {
        Git2Repo::open(self._temp_dir.path().to_path_buf()).unwrap()
    }

    pub fn commit_file(&self, path: &str, content: &str, message: &str) -> git2::Oid {
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

    #[allow(dead_code)]
    pub fn delete_file(&self, path: &str, message: &str) -> git2::Oid {
        let repo_path = self.repo.workdir().unwrap();
        let file_path = repo_path.join(path);

        fs::remove_file(&file_path).unwrap();

        let mut index = self.repo.index().unwrap();
        index.remove_path(std::path::Path::new(path)).unwrap();
        index.write().unwrap();

        let tree_oid = index.write_tree().unwrap();
        let tree = self.repo.find_tree(tree_oid).unwrap();

        let sig = Signature::now("Test User", "test@example.com").unwrap();

        let parent_commit = self.repo.head().unwrap();
        let parent = self
            .repo
            .find_commit(parent_commit.target().unwrap())
            .unwrap();

        self.repo
            .commit(Some("HEAD"), &sig, &sig, message, &tree, &[&parent])
            .unwrap()
    }

    #[allow(dead_code)]
    pub fn create_branch(&self, name: &str, target: git2::Oid) {
        let commit = self.repo.find_commit(target).unwrap();
        self.repo.branch(name, &commit, false).unwrap();
    }

    #[allow(dead_code)]
    pub fn create_tag(&self, name: &str, target: git2::Oid) {
        let commit = self.repo.find_commit(target).unwrap();
        let sig = Signature::now("Test User", "test@example.com").unwrap();
        self.repo
            .tag(name, commit.as_object(), &sig, "test tag", false)
            .unwrap();
    }

    #[allow(dead_code)]
    pub fn checkout(&self, refname: &str) {
        self.repo.set_head(refname).unwrap();
        self.repo.checkout_head(None).unwrap();
    }
}
