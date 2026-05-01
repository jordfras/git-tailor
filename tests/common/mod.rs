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
pub mod prelude;
#[allow(unused_imports)]
pub use fake::{StubRepo, StubRepoBuilder};

use git_tailor::{
    CommitDiff, CommitInfo, DeltaStatus, FileDiff, Hunk, Oid, VirtualOid,
    app::AppState,
    fragmap::{FileSpan, FragMap, SpanCluster, TouchKind},
    repo::{ConflictState, Git2Repo, GitRepo},
};
use git2::{Repository, Signature};
use ratatui::{Frame, Terminal, backend::TestBackend, buffer::Buffer};
use std::fs;
use tempfile::TempDir;

/// Shared git repository fixture for integration tests.
///
/// Keeps a `git2::Repository` for low-level setup (creating commits, branches,
/// tags) and exposes [`git_repo()`][TestRepo::git_repo] to obtain a `Git2Repo`
/// handle for calling library functions under test.
pub struct TestRepo {
    _temp_dir: TempDir,
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

    /// Write `content` to `path` in the working tree, creating parent
    /// directories as needed.
    ///
    /// This is a raw filesystem operation — it does **not** touch the index.
    /// Use it to create unstaged dirty state (e.g. to verify that an operation
    /// preserves working-tree changes) or as the first step before
    /// [`stage_file`][Self::stage_file].
    pub fn write_file(&self, path: &str, content: &str) {
        let file_path = self.repo.workdir().unwrap().join(path);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&file_path, content).unwrap();
    }

    /// Stage `path` by adding it to the index with `add_path`.
    ///
    /// Unlike [`GitRepo::stage_file`], this is a minimal test-fixture helper:
    /// it does not refresh the index from disk first, does not handle deleted
    /// files (those need `remove_path`), and does not clear conflict stages.
    /// Use it only to set up pre-operation dirty state in tests, never to
    /// simulate conflict resolution — for that call `git_repo.stage_file()`
    /// which is the production implementation under test.
    pub fn stage_file(&self, path: &str) {
        let mut index = self.repo.index().unwrap();
        index.add_path(std::path::Path::new(path)).unwrap();
        index.write().unwrap();
    }

    /// Commit whatever is currently staged, advancing HEAD.
    ///
    /// Unlike [`GitRepo`] methods, this operates via the raw `git2::Repository`
    /// handle and is intended purely for building test histories. It always
    /// uses the fixed "Test User / test@example.com" identity and works on
    /// both an initial (parentless) commit and subsequent commits.
    pub fn commit(&self, message: &str) -> git2::Oid {
        let mut index = self.repo.index().unwrap();
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

    pub fn commit_file(&self, path: &str, content: &str, message: &str) -> git2::Oid {
        self.write_file(path, content);
        self.stage_file(path);
        self.commit(message)
    }

    /// Create a single commit that writes (or overwrites) multiple files at once.
    pub fn commit_files(&self, files: &[(&str, &str)], message: &str) -> git2::Oid {
        for (path, content) in files {
            self.write_file(path, content);
            self.stage_file(path);
        }
        self.commit(message)
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

    /// Walk commits from HEAD back to (but not including) the given stop OID.
    /// Returns commits in oldest-first order.
    pub fn commits_from_head(&self, stop_oid: git2::Oid) -> Vec<git2::Oid> {
        let head_oid = self.repo.head().unwrap().target().unwrap();
        let mut revwalk = self.repo.revwalk().unwrap();
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

    /// Stage a gitlink (submodule pointer) entry in the repo's index at `path`
    /// pointing at `target_oid`, without making a commit.
    ///
    /// This simulates the state produced by `git submodule update` or a manual
    /// submodule bump — the working tree inside the submodule directory is
    /// unaffected; only the gitlink entry in the index changes.
    pub fn stage_gitlink(&self, path: &str, target_oid: git2::Oid) {
        let mut index = self.repo.index().unwrap();
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

    /// Create a 3-commit drop-conflict scenario and return the resulting
    /// [`ConflictState`].
    ///
    /// Commit history (oldest → newest):
    /// - base: `"a.txt"` = `"base\n"`
    /// - to_drop: `"a.txt"` = `"base\ndropped\n"` (the commit that will be dropped)
    /// - head: `"a.txt"` = `"base\ndropped\nhead\n"` (depends on the dropped line)
    ///
    /// Dropping `to_drop` conflicts when cherry-picking `head` onto `base`.
    pub fn make_drop_conflict(&self) -> ConflictState {
        let _base = self.commit_file("a.txt", "base\n", "base");
        let to_drop = self.commit_file("a.txt", "base\ndropped\n", "add dropped line");
        let head = self.commit_file("a.txt", "base\ndropped\nhead\n", "add head line");
        let git_repo = self.git_repo();
        crate::expect_rebase_conflict!(
            git_repo
                .drop_commit(&Oid::from(to_drop), &Oid::from(head))
                .unwrap()
        )
    }
}

/// Test harness that wraps `Terminal<TestBackend>` to eliminate per-test
/// boilerplate: create backend, wrap in terminal, draw, clone buffer.
pub struct TuiTestHarness {
    terminal: Terminal<TestBackend>,
}

impl TuiTestHarness {
    pub fn new(width: u16, height: u16) -> Self {
        let terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        Self { terminal }
    }

    /// Typical 80×24 terminal used by most detail and selector tests.
    pub fn typical() -> Self {
        Self::new(80, 24)
    }

    /// Wide 120×20 terminal for tests that need extra horizontal space.
    pub fn wide() -> Self {
        Self::new(120, 20)
    }

    /// Short 80×10 terminal for tests that need limited vertical space.
    pub fn short() -> Self {
        Self::new(80, 10)
    }

    /// Narrow 60×10 terminal for tests that need reduced horizontal space.
    pub fn narrow() -> Self {
        Self::new(60, 10)
    }

    /// Very narrow 40×15 terminal for tests that exercise very tight widths.
    pub fn very_narrow() -> Self {
        Self::new(40, 15)
    }

    /// Draw one frame using `f` and return the resulting buffer.
    ///
    /// Pass the returned `Buffer` to `insta::assert_debug_snapshot!` at the
    /// call site — calling that macro directly in the test ensures insta uses
    /// the test function name (not a helper location) for the snapshot file.
    pub fn render(&mut self, f: impl FnOnce(&mut Frame)) -> Buffer {
        self.terminal.draw(|frame| f(frame)).unwrap();
        self.terminal.backend().buffer().clone()
    }
}

/// Build an `AppState` with synthesised commits for use in TUI tests.
///
/// Each element of `summaries` becomes one commit. OIDs are derived
/// deterministically from the index: index `i` gets the 12-hex-char OID
/// `format!("{:012x}", (i + 1) * 0x111111111111)` — e.g. `"111111111111"`,
/// `"222222222222"`, `"333333333333"`, etc.
pub fn app_state_from_commit_summaries(summaries: &[&str]) -> AppState {
    let commits = summaries
        .iter()
        .enumerate()
        .map(|(i, &summary)| {
            let oid = format!("{:012x}", (i + 1) * 0x111111111111_usize);
            create_test_commit(&oid, summary)
        })
        .collect();
    let mut app = AppState::new();
    app.commits = commits;
    app
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

/// Build a `FragMap` with the given commit OIDs, clusters, and matrix.
pub fn create_fragmap(
    commit_oids: Vec<&str>,
    clusters: Vec<SpanCluster>,
    matrix: Vec<Vec<TouchKind>>,
) -> FragMap {
    FragMap {
        commits: commit_oids
            .into_iter()
            .map(|s| VirtualOid::Real(Oid::from(s)))
            .collect(),
        clusters,
        matrix,
    }
}

/// Build a `SpanCluster` covering a single file span, touched by the given commits.
pub fn simple_cluster(path: &str, start: u32, end: u32, oids: &[&str]) -> SpanCluster {
    SpanCluster {
        spans: vec![FileSpan {
            path: path.to_string(),
            start_line: start,
            end_line: end,
        }],
        commit_oids: oids
            .iter()
            .copied()
            .map(|s| VirtualOid::Real(Oid::from(s)))
            .collect(),
    }
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
