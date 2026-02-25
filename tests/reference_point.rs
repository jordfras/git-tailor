use git2::{Repository, Signature};
use std::fs;
use tempfile::TempDir;

/// Helper to create a test repository with a basic commit history
struct TestRepo {
    _temp_dir: TempDir, // Kept alive for cleanup
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

    fn create_branch(&self, name: &str, target: git2::Oid) {
        let commit = self.repo.find_commit(target).unwrap();
        self.repo.branch(name, &commit, false).unwrap();
    }

    fn create_tag(&self, name: &str, target: git2::Oid) {
        let commit = self.repo.find_commit(target).unwrap();
        let sig = Signature::now("Test User", "test@example.com").unwrap();
        self.repo
            .tag(name, commit.as_object(), &sig, "test tag", false)
            .unwrap();
    }

    fn checkout(&self, refname: &str) {
        self.repo.set_head(refname).unwrap();
        self.repo.checkout_head(None).unwrap();
    }
}

#[test]
fn test_merge_base_with_branch_name() {
    let test = TestRepo::new();

    let c1 = test.commit_file("file.txt", "initial", "Initial commit");
    let _c2 = test.commit_file("file.txt", "on main", "Main commit");

    test.create_branch("feature", c1);
    test.checkout("refs/heads/feature");
    let _c3 = test.commit_file("feature.txt", "feature work", "Feature commit");

    let merge_base = test
        .repo
        .merge_base(
            test.repo.refname_to_id("refs/heads/feature").unwrap(),
            test.repo.refname_to_id("refs/heads/master").unwrap(),
        )
        .unwrap();

    assert_eq!(merge_base, c1);
}

#[test]
fn test_merge_base_with_tag() {
    let test = TestRepo::new();

    let c1 = test.commit_file("file.txt", "v1", "Version 1");
    test.create_tag("v1.0", c1);

    let c2 = test.commit_file("file.txt", "v2", "Version 2");

    let head_oid = test.repo.head().unwrap().target().unwrap();
    let tag_obj = test.repo.revparse_single("v1.0").unwrap();
    // Peel the tag to get the commit it points to
    let tag_commit_oid = tag_obj.peel_to_commit().unwrap().id();

    let merge_base = test.repo.merge_base(head_oid, tag_commit_oid).unwrap();

    assert_eq!(merge_base, c1);
    assert_eq!(head_oid, c2);
}

#[test]
fn test_merge_base_with_short_hash() {
    let test = TestRepo::new();

    let c1 = test.commit_file("file.txt", "base", "Base commit");
    let c1_short = &c1.to_string()[..7];

    let c2 = test.commit_file("file.txt", "next", "Next commit");

    let resolved = test.repo.revparse_single(c1_short).unwrap();
    assert_eq!(resolved.id(), c1);

    let head_oid = test.repo.head().unwrap().target().unwrap();
    let merge_base = test.repo.merge_base(head_oid, resolved.id()).unwrap();

    assert_eq!(merge_base, c1);
    assert_eq!(head_oid, c2);
}

#[test]
fn test_merge_base_with_long_hash() {
    let test = TestRepo::new();

    let c1 = test.commit_file("file.txt", "base", "Base commit");
    let c1_long = c1.to_string();

    let c2 = test.commit_file("file.txt", "next", "Next commit");

    let resolved = test.repo.revparse_single(&c1_long).unwrap();
    assert_eq!(resolved.id(), c1);

    let head_oid = test.repo.head().unwrap().target().unwrap();
    let merge_base = test.repo.merge_base(head_oid, resolved.id()).unwrap();

    assert_eq!(merge_base, c1);
    assert_eq!(head_oid, c2);
}

#[test]
fn test_merge_base_same_commit() {
    let test = TestRepo::new();

    let c1 = test.commit_file("file.txt", "content", "Commit");

    let merge_base = test.repo.merge_base(c1, c1).unwrap();
    assert_eq!(merge_base, c1);
}

#[test]
fn test_merge_base_with_diverged_branches() {
    let test = TestRepo::new();

    let base = test.commit_file("file.txt", "base", "Base");

    test.create_branch("branch-a", base);
    test.checkout("refs/heads/branch-a");
    let _a1 = test.commit_file("a.txt", "a1", "A1");
    let _a2 = test.commit_file("a.txt", "a2", "A2");

    test.checkout("refs/heads/master");
    test.create_branch("branch-b", base);
    test.checkout("refs/heads/branch-b");
    let _b1 = test.commit_file("b.txt", "b1", "B1");
    let _b2 = test.commit_file("b.txt", "b2", "B2");

    let a_oid = test.repo.refname_to_id("refs/heads/branch-a").unwrap();
    let b_oid = test.repo.refname_to_id("refs/heads/branch-b").unwrap();

    let merge_base = test.repo.merge_base(a_oid, b_oid).unwrap();
    assert_eq!(merge_base, base);
}
