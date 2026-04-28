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

#[allow(unused_imports)]
pub mod assert;

#[allow(unused_imports)]
pub mod fake;
#[allow(unused_imports)]
pub use fake::{FakeDiffRepo, NoOpRepo};

use git_tailor::{
    CommitDiff, CommitInfo, DeltaStatus, FileDiff, Hunk, Oid, VirtualOid, repo::Git2Repo,
};
use git2::{Repository, Signature};
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

impl Default for TestRepo {
    fn default() -> Self {
        Self::new()
    }
}

impl TestRepo {
    pub fn new() -> Self {
        let temp_dir = TempDir::new().unwrap();
        let mut opts = git2::RepositoryInitOptions::new();
        opts.initial_head("main");
        let repo = Repository::init_opts(temp_dir.path(), &opts).unwrap();

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

    /// Create a single commit that writes (or overwrites) multiple files at once.
    pub fn commit_files(&self, files: &[(&str, &str)], message: &str) -> git2::Oid {
        let repo_path = self.repo.workdir().unwrap();
        let mut index = self.repo.index().unwrap();

        for (path, content) in files {
            let file_path = repo_path.join(path);
            if let Some(parent) = file_path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&file_path, content).unwrap();
            index.add_path(std::path::Path::new(path)).unwrap();
        }
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

    /// Rename a file and optionally modify its content in a single commit.
    /// If `new_content` is `None`, the file keeps its current content.
    pub fn rename_file(
        &self,
        old_path: &str,
        new_path: &str,
        new_content: Option<&str>,
        message: &str,
    ) -> git2::Oid {
        let repo_path = self.repo.workdir().unwrap();
        let old_file = repo_path.join(old_path);
        let new_file = repo_path.join(new_path);

        let content = match new_content {
            Some(c) => c.to_string(),
            None => fs::read_to_string(&old_file).unwrap(),
        };

        fs::remove_file(&old_file).unwrap();
        if let Some(parent) = new_file.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&new_file, content).unwrap();

        let mut index = self.repo.index().unwrap();
        index.remove_path(std::path::Path::new(old_path)).unwrap();
        index.add_path(std::path::Path::new(new_path)).unwrap();
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

    pub fn create_branch(&self, name: &str, target: git2::Oid) {
        let commit = self.repo.find_commit(target).unwrap();
        self.repo.branch(name, &commit, false).unwrap();
    }

    pub fn create_tag(&self, name: &str, target: git2::Oid) {
        let commit = self.repo.find_commit(target).unwrap();
        let sig = Signature::now("Test User", "test@example.com").unwrap();
        self.repo
            .tag(name, commit.as_object(), &sig, "test tag", false)
            .unwrap();
    }

    /// Write a key/value pair to the repository-local git config.
    pub fn set_config(&self, key: &str, value: &str) {
        let mut config = self.repo.config().unwrap();
        config.set_str(key, value).unwrap();
    }

    pub fn checkout(&self, refname: &str) {
        self.repo.set_head(refname).unwrap();
        self.repo.checkout_head(None).unwrap();
    }
}

/// Build a minimal `CommitDiff` with a single file hunk for use in static fragmap tests.
///
/// `hunk_range` is `(start_line, line_count)` for both old and new sides.
pub fn create_test_commit_diff(
    oid: &str,
    summary: &str,
    path: &str,
    hunk_range: (u32, u32),
) -> CommitDiff {
    CommitDiff {
        commit: create_test_commit(oid, summary),
        files: vec![FileDiff {
            old_path: Some(path.to_string()),
            new_path: Some(path.to_string()),
            status: DeltaStatus::Modified,
            hunks: vec![Hunk {
                old_start: hunk_range.0,
                old_lines: hunk_range.1,
                new_start: hunk_range.0,
                new_lines: hunk_range.1,
                lines: vec![],
            }],
        }],
    }
}

/// Stage a gitlink (submodule pointer) entry in the repo's index at `path`
/// pointing at `target_oid`, without making a commit.
///
/// This simulates the state produced by `git submodule update` or a manual
/// submodule bump — the working tree inside the submodule directory is
/// unaffected; only the gitlink entry in the index changes.
pub fn stage_gitlink(repo: &git2::Repository, path: &str, target_oid: git2::Oid) {
    let mut index = repo.index().unwrap();
    index.read(true).unwrap();
    index
        .add(&git2::IndexEntry {
            ctime: git2::IndexTime::new(0, 0),
            mtime: git2::IndexTime::new(0, 0),
            dev: 0,
            ino: 0,
            mode: 0o160000,
            uid: 0,
            gid: 0,
            file_size: 0,
            id: target_oid,
            flags: 0,
            flags_extended: 0,
            path: path.as_bytes().to_vec(),
        })
        .unwrap();
    index.write().unwrap();
}

/// Read a file from a specific commit tree.
pub fn file_content_at(repo: &git2::Repository, commit_oid: git2::Oid, path: &str) -> String {
    let commit = repo.find_commit(commit_oid).unwrap();
    let tree = commit.tree().unwrap();
    let entry = tree.get_path(std::path::Path::new(path)).unwrap();
    let blob = repo
        .find_blob(entry.id())
        .expect("tree entry should be a blob");
    String::from_utf8_lossy(blob.content()).into_owned()
}

/// Walk commits from HEAD back to (but not including) the given stop OID.
/// Returns commits in oldest-first order.
pub fn commits_from_head(repo: &git2::Repository, stop_oid: git2::Oid) -> Vec<git2::Oid> {
    let head_oid = repo.head().unwrap().target().unwrap();
    let mut revwalk = repo.revwalk().unwrap();
    revwalk.push(head_oid).unwrap();
    let mut oids = Vec::new();
    for result in revwalk {
        let oid = result.unwrap();
        if oid == stop_oid {
            break;
        }
        oids.push(oid);
    }
    oids.reverse(); // oldest first
    oids
}

/// Build a minimal `CommitInfo` for use in TUI snapshot tests.
pub fn create_test_commit(oid: &str, summary: &str) -> CommitInfo {
    CommitInfo {
        oid: VirtualOid::Real(Oid::from(oid)),
        summary: summary.to_string(),
        author: Some("Test Author".to_string()),
        date: Some("1705318200".to_string()),
        parent_oids: vec![Oid::from("parent123")],
        message: summary.to_string(),
        author_email: Some("test@example.com".to_string()),
        author_date: Some(time::OffsetDateTime::from_unix_timestamp(1705318200).unwrap()),
        committer: Some("Test Committer".to_string()),
        committer_email: Some("committer@example.com".to_string()),
        commit_date: Some(time::OffsetDateTime::from_unix_timestamp(1705318200).unwrap()),
    }
}
