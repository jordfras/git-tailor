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

// Loading the commit list, at startup and after each operation: commit walking,
// progress display, and matrix confirmation.

use anyhow::{Context, Result};
use git_tailor::repo::RepoRead;
use git_tailor::{
    CommitDiff, CommitInfo, Oid, VirtualOid,
    app::{AppMode, AppState, LoadingEscape, is_force_quit},
    fragmap::{self, FragMapProgress},
    views,
};

use crate::cli::Cli;
use crate::progress_screen::{ProgressScreen, ShowProgress, waiting_key};
use crate::terminal_guard::TerminalGuard;

pub fn load_initial_commits(
    git_repo: &impl RepoRead,
    cli: &Cli,
) -> Result<Option<(Vec<CommitInfo>, Oid, bool)>> {
    let head_oid = git_repo.head_oid()?;

    if cli.all {
        let root_oid = git_repo.root_commit_oid()?;
        let all_commits = git_repo.list_commits(&head_oid, &root_oid)?;
        if all_commits.is_empty() {
            eprintln!("No commits to display.");
            return Ok(None);
        }
        return Ok(Some((all_commits, root_oid, true)));
    }

    let base = resolve_base_branch(git_repo, cli)?;
    if let Some(note) = &base.fallback_note {
        eprintln!("{note}");
    }
    let base = base.name;
    let reference_oid = git_repo.find_reference_point(&base)?;
    let raw = git_repo.list_commits(&head_oid, &reference_oid)?;
    // Exclude the merge-base commit — it's shared with the target branch
    // and must not be modified (squashed, moved, or split).
    let commits: Vec<CommitInfo> = raw
        .into_iter()
        .filter(|c| c.oid != VirtualOid::Real(reference_oid.clone()))
        .collect();
    if commits.is_empty() {
        eprintln!(
            "No commits to display: HEAD is at the merge-base with '{}'",
            base
        );
        eprintln!("The current branch has no commits beyond the common ancestor.");
        return Ok(None);
    }
    Ok(Some((commits, reference_oid, false)))
}

/// The range the TUI loads, from [`resolve_oid_bounds`].
pub struct OidBounds {
    pub head_oid: Oid,
    pub reference_oid: Oid,
    pub include_reference_oid: bool,
    /// What to tell the user when the base is a fallback: which one, and why.
    pub fallback_note: Option<String>,
}

/// Resolve the OID bounds for the TUI loading path (fast: no commit listing),
/// or `Ok(None)` when there are definitely no commits to display.
pub fn resolve_oid_bounds(git_repo: &impl RepoRead, cli: &Cli) -> Result<Option<OidBounds>> {
    let head_oid = git_repo.head_oid()?;

    if cli.all {
        let root_oid = git_repo.root_commit_oid()?;
        return Ok(Some(OidBounds {
            head_oid,
            reference_oid: root_oid,
            include_reference_oid: true,
            fallback_note: None,
        }));
    }
    let Base {
        name: base,
        fallback_note,
    } = resolve_base_branch(git_repo, cli)?;
    let reference_oid = git_repo.find_reference_point(&base)?;

    // Fast empty-branch check: if HEAD equals the merge-base, no branch commits exist.
    if head_oid == reference_oid {
        eprintln!(
            "No commits to display: HEAD is at the merge-base with '{}'",
            base
        );
        eprintln!("The current branch has no commits beyond the common ancestor.");
        return Ok(None);
    }

    Ok(Some(OidBounds {
        head_oid,
        reference_oid,
        include_reference_oid: false,
        fallback_note,
    }))
}

/// The base to list commits from.
struct Base {
    name: String,
    fallback_note: Option<String>,
}

impl Base {
    fn named(name: String) -> Self {
        Self {
            name,
            fallback_note: None,
        }
    }

    fn fallen_back_to(name: &str) -> Self {
        let set_head = if name.starts_with("origin/") {
            " `git remote set-head origin --auto` sets origin/HEAD."
        } else {
            ""
        };
        Self {
            name: name.to_string(),
            fallback_note: Some(format!(
                "origin/HEAD is not set, so the base is {name}. Pass a base to choose \
                 another.{set_head}"
            )),
        }
    }
}

/// Where the default branch is looked for when `origin/HEAD` is not set, in
/// order: what a pull request targets first, then branches of a repository with
/// no remote. As full reference names, so that a tag cannot stand in for one.
const FALLBACK_BASES: [(&str, &str); 4] = [
    ("refs/remotes/origin/main", "origin/main"),
    ("refs/remotes/origin/master", "origin/master"),
    ("refs/heads/main", "main"),
    ("refs/heads/master", "master"),
];

fn resolve_base_branch(git_repo: &impl RepoRead, cli: &Cli) -> Result<Base> {
    if let Some(base) = &cli.base {
        return Ok(Base::named(base.clone()));
    }
    if let Some(base) = git_repo
        .default_branch()
        .context("failed to resolve default branch from origin/HEAD")?
    {
        return Ok(Base::named(base));
    }
    for (full_name, base) in FALLBACK_BASES {
        if git_repo.has_reference(full_name)? {
            return Ok(Base::fallen_back_to(base));
        }
    }
    anyhow::bail!(
        "origin/HEAD is not set, and none of origin/main, origin/master, main or master \
         exists. Pass the base to list commits from, as in `gt <branch>`."
    )
}

/// Load commits and diffs with a live counter dialog, then optionally build the
/// hunk group matrix. Updates `app` in place and sets `app.mode = CommitList`;
/// on a cancel or an error the loading dialog is left for the caller to replace.
///
/// When `app.list` has rows (reload case), the dialog is shown as an
/// overlay on top of the existing commit list. When empty (initial load), the
/// dialog is shown full-screen.
///
/// Returns `Ok(false)` if the user pressed Ctrl-C during loading.
pub fn load_with_progress(
    git_repo: &impl RepoRead,
    terminal_guard: &mut TerminalGuard,
    app: &mut AppState,
    from_oid: &Oid,
    reference_oid: &Oid,
    include_reference_oid: bool,
    full: bool,
) -> Result<bool> {
    let total = git_repo.commit_walker(from_oid, reference_oid)?.count();
    app.mode = AppMode::Loading {
        title: "Loading Commits",
        message: "Loading commits\u{2026}",
        progress: Some((0, total)),
        escape: None,
    };
    terminal_guard
        .terminal()
        .draw(|frame| views::loading::render(app, frame))?;

    let Some(raw) = walk_commits(
        git_repo,
        terminal_guard,
        app,
        from_oid,
        reference_oid,
        total,
    )?
    else {
        return Ok(false);
    };

    let mut diffs = raw;
    diffs.reverse();
    diffs.retain(|d| {
        include_reference_oid || d.commit.oid != VirtualOid::Real(reference_oid.clone())
    });

    diffs.extend(
        [
            git_repo.staged_diff_for_fragmap()?,
            git_repo.unstaged_diff_for_fragmap()?,
        ]
        .into_iter()
        .flatten(),
    );
    let matrix = match build_hunk_group_matrix(terminal_guard, app, &diffs, full)? {
        HunkGroupMatrixBuild::Built(matrix) => Some(matrix),
        HunkGroupMatrixBuild::Skipped => None,
        HunkGroupMatrixBuild::Interrupted => return Ok(false),
    };

    app.list.set_rows(diffs);
    app.list.tip = from_oid.clone();
    app.list.branch = git_repo.head_branch().unwrap_or_default();
    app.fragmap = matrix;
    app.fragmap_scroll_offset = 0;
    app.detail.v.offset = 0;
    app.list.reset_viewport();
    app.list.selection_index = select_initial_index(app.list.rows());
    app.mode = AppMode::CommitList;

    Ok(true)
}

/// Reload the list from HEAD, after an operation or on `R`, keeping the range
/// and options it was loaded with. `select` is the row to land on, clamped to
/// the new list; `None` takes the natural position. Refreshing the detail view
/// ignores it and stays on the commit shown, wherever that row has moved to.
pub fn reload(
    git_repo: &impl RepoRead,
    terminal_guard: &mut TerminalGuard,
    app: &mut AppState,
    select: Option<usize>,
) {
    let before = app.mode.clone();
    let shown = match before {
        AppMode::CommitDetail => app.list.selected().map(|c| Shown {
            oid: c.oid.clone(),
            offset: app.detail.v.offset,
        }),
        _ => None,
    };
    let loaded = git_repo.head_oid().and_then(|head_oid| {
        let reference_oid = app.reference_oid.clone();
        let include_reference_oid = app.include_reference_oid;
        let full = app.full_fragmap;
        load_with_progress(
            git_repo,
            terminal_guard,
            app,
            &head_oid,
            &reference_oid,
            include_reference_oid,
            full,
        )
    });
    settle_reload(git_repo, app, before, shown, loaded, select);
}

/// The commit the detail view showed when a refresh started, and how far down.
struct Shown {
    oid: VirtualOid,
    offset: usize,
}

/// Put the screen right once a reload is over. `before` is the mode the reload
/// started from, `shown` what the detail view showed if that is where, and
/// `loaded` what [`load_with_progress`] returned.
fn settle_reload(
    git_repo: &impl RepoRead,
    app: &mut AppState,
    before: AppMode,
    shown: Option<Shown>,
    loaded: Result<bool>,
    select: Option<usize>,
) {
    match loaded {
        Err(e) => {
            app.mode = before;
            app.set_error_message(format!("Reload failed: {e:#}"));
        }
        // A load lands on the list, which is right after an operation but not
        // after refreshing the detail view.
        Ok(true) => {
            match shown {
                Some(shown) => match app.list.commits().position(|c| c.oid == shown.oid) {
                    Some(index) => {
                        app.list.selection_index = index;
                        app.detail.v.offset = shown.offset;
                        app.mode = before;
                    }
                    None => {
                        app.detail.search.clear();
                        app.set_error_message(
                            "The commit the detail view showed is no longer on the branch",
                        );
                    }
                },
                None => {
                    if let Some(index) = select {
                        app.list.selection_index =
                            index.min(app.list.rows().len().saturating_sub(1));
                    }
                }
            }
            if app.list.search.state.active {
                app.list.point_search_at_selection();
            }
        }
        Ok(false) => {
            app.mode = before;
            app.set_error_message(
                "Reload canceled — the list may be out of date; press R to reload",
            );
        }
    }
    if matches!(app.mode, AppMode::CommitDetail) {
        views::commit_detail::load_diff(git_repo, app);
    }
}

/// Walk commits from `from_oid` back to `reference_oid`, collecting each
/// commit's fragmap diff. Renders a live counter dialog at ~60 fps and polls
/// for Ctrl-C between frames.
///
/// Returns `Ok(None)` if the user pressed Ctrl-C, `Ok(Some(raw))` on success.
/// The returned vec is in reverse-chronological order (newest first).
fn walk_commits(
    git_repo: &impl RepoRead,
    terminal_guard: &mut TerminalGuard,
    app: &mut AppState,
    from_oid: &Oid,
    reference_oid: &Oid,
    total: usize,
) -> Result<Option<Vec<CommitDiff>>> {
    let walker = git_repo.commit_walker(from_oid, reference_oid)?;
    let mut raw: Vec<CommitDiff> = Vec::new();

    let render_interval = std::time::Duration::from_millis(16);
    let mut last_render = std::time::Instant::now()
        .checked_sub(render_interval)
        .unwrap_or_else(std::time::Instant::now);

    for commit_result in walker {
        let commit = commit_result.context("Failed to load commits")?;
        let oid = commit
            .oid
            .as_oid()
            .context("The commit walker yielded a staged or unstaged row")?;
        let files = git_repo.commit_diff_for_fragmap(oid)?.files;
        raw.push(CommitDiff { commit, files });

        let now = std::time::Instant::now();
        if now.duration_since(last_render) >= render_interval {
            last_render = now;
            app.mode = AppMode::Loading {
                title: "Loading Commits",
                message: "Loading commits\u{2026}",
                progress: Some((raw.len(), total)),
                escape: None,
            };
            terminal_guard
                .terminal()
                .draw(|frame| views::loading::render(app, frame))?;

            if waiting_key().is_some_and(|key| is_force_quit(&key)) {
                return Ok(None);
            }
        }
    }

    Ok(Some(raw))
}

/// How building the hunk group matrix ended.
enum HunkGroupMatrixBuild {
    Built(fragmap::FragMap),
    /// `s`/`S` was pressed during file clustering: the list goes on without a
    /// matrix.
    Skipped,
    /// Ctrl-C, which abandons the whole load, as it does in [`walk_commits`].
    Interrupted,
}

/// Build the fragmap with a live progress dialog for each phase.
fn build_hunk_group_matrix(
    terminal_guard: &mut TerminalGuard,
    app: &mut AppState,
    diffs: &[CommitDiff],
    full: bool,
) -> Result<HunkGroupMatrixBuild> {
    let mut screen = ProgressScreen::new(terminal_guard, "Hunk Group Matrix");
    let result = fragmap::build_fragmap(diffs, !full, &mut |phase| match phase {
        FragMapProgress::ClusteringFile {
            files_done,
            files_total,
        } => screen.show(
            app,
            "Clustering files\u{2026}",
            Some((files_done, files_total)),
            Some(LoadingEscape::Skip),
        ),
        FragMapProgress::Deduplicating => {
            screen.show(app, "Deduplicating clusters\u{2026}", None, None)
        }
        FragMapProgress::BuildingMatrix {
            commits_done,
            commits_total,
        } => screen.show(
            app,
            "Building matrix\u{2026}",
            Some((commits_done, commits_total)),
            Some(LoadingEscape::Skip),
        ),
        FragMapProgress::ReadingCommits { .. } => true,
    });
    let interrupted = screen.interrupted();
    screen.finish()?;
    if interrupted {
        return Ok(HunkGroupMatrixBuild::Interrupted);
    }
    Ok(result.map_or(HunkGroupMatrixBuild::Skipped, HunkGroupMatrixBuild::Built))
}

/// Choose the initial selection index for a commit list:
/// unstaged row if present, else staged row if present, else the last commit.
fn select_initial_index(rows: &[CommitDiff]) -> usize {
    if let Some(i) = rows
        .iter()
        .rposition(|r| r.commit.oid == VirtualOid::Unstaged)
    {
        return i;
    }
    if let Some(i) = rows
        .iter()
        .rposition(|r| r.commit.oid == VirtualOid::Staged)
    {
        return i;
    }
    rows.len().saturating_sub(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock_repo::MockRepo;
    use git_tailor::app::CommitListState;

    fn resolve(repo: &MockRepo, args: &[&str]) -> Result<Base> {
        use clap::Parser;
        let cli = Cli::parse_from(std::iter::once("gt").chain(args.iter().copied()));
        resolve_base_branch(repo, &cli)
    }

    fn base_of(repo: &MockRepo, args: &[&str]) -> Result<String> {
        resolve(repo, args).map(|base| base.name)
    }

    fn note_of(repo: &MockRepo, args: &[&str]) -> Option<String> {
        resolve(repo, args).unwrap().fallback_note
    }

    fn with_references(references: &[&str]) -> MockRepo {
        MockRepo {
            references: references.iter().map(ToString::to_string).collect(),
            ..MockRepo::default()
        }
    }

    #[test]
    fn a_base_given_on_the_command_line_wins() {
        let repo = MockRepo {
            origin_head: Some("origin/main".to_string()),
            ..with_references(&["refs/heads/main"])
        };
        assert_eq!(base_of(&repo, &["v1.0"]).unwrap(), "v1.0");
    }

    #[test]
    fn origin_head_is_the_base_when_set() {
        let repo = MockRepo {
            origin_head: Some("origin/trunk".to_string()),
            ..with_references(&["refs/remotes/origin/main"])
        };
        assert_eq!(base_of(&repo, &[]).unwrap(), "origin/trunk");
    }

    #[test]
    fn without_origin_head_a_remote_branch_comes_before_a_local_one() {
        let repo = with_references(&["refs/heads/main", "refs/remotes/origin/master"]);
        assert_eq!(base_of(&repo, &[]).unwrap(), "origin/master");
    }

    #[test]
    fn without_origin_head_main_comes_before_master() {
        let both = with_references(&["refs/heads/master", "refs/heads/main"]);
        assert_eq!(base_of(&both, &[]).unwrap(), "main");
        let master = with_references(&["refs/heads/master"]);
        assert_eq!(base_of(&master, &[]).unwrap(), "master");
    }

    #[test]
    fn a_base_given_or_named_by_origin_head_goes_unremarked() {
        let repo = MockRepo {
            origin_head: Some("origin/main".to_string()),
            ..MockRepo::default()
        };
        assert_eq!(note_of(&repo, &[]), None);
        assert_eq!(note_of(&with_references(&[]), &["v1.0"]), None);
    }

    #[test]
    fn a_fallback_says_which_base_it_picked() {
        let note = note_of(&with_references(&["refs/heads/master"]), &[]).unwrap();
        assert!(note.contains("origin/HEAD is not set"), "{note}");
        assert!(note.contains("the base is master"), "{note}");
        assert!(!note.contains("set-head"), "{note}");
    }

    /// With a remote branch picked, setting origin/HEAD is what stops it.
    #[test]
    fn a_fallback_to_a_remote_branch_says_how_to_set_origin_head() {
        let repo = with_references(&["refs/remotes/origin/master"]);
        let note = note_of(&repo, &[]).unwrap();
        assert!(note.contains("the base is origin/master"), "{note}");
        assert!(note.contains("git remote set-head origin --auto"), "{note}");
    }

    /// A repository at `HEAD` on `feature`, two commits past the branch
    /// `master`, which is one past the root; a tag `master` on the root.
    fn branch_and_tag_named_master() -> (tempfile::TempDir, git2::Oid) {
        let dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        let sig = git2::Signature::now("T", "t@e").unwrap();
        let tree = repo
            .find_tree(repo.index().unwrap().write_tree().unwrap())
            .unwrap();
        let commit = |parent: Option<git2::Oid>, message: &str| {
            let parents: Vec<_> = parent
                .map(|p| repo.find_commit(p).unwrap())
                .into_iter()
                .collect();
            let parents: Vec<_> = parents.iter().collect();
            repo.commit(None, &sig, &sig, message, &tree, &parents)
                .unwrap()
        };
        let root = commit(None, "root");
        let branch_tip = commit(Some(root), "on master");
        let feature = commit(Some(branch_tip), "feature 1");
        let feature = commit(Some(feature), "feature 2");
        repo.branch("master", &repo.find_commit(branch_tip).unwrap(), true)
            .unwrap();
        repo.tag_lightweight("master", &repo.find_object(root, None).unwrap(), true)
            .unwrap();
        repo.reference("refs/heads/feature", feature, true, "test")
            .unwrap();
        repo.set_head("refs/heads/feature").unwrap();
        (dir, branch_tip)
    }

    /// The fallback picks the branch by its full name, so it must look it up by
    /// that name too: by its short one, git finds the tag first.
    #[test]
    fn a_fallback_branch_is_not_mistaken_for_a_tag_of_the_same_name() {
        use clap::Parser;
        let (dir, branch_tip) = branch_and_tag_named_master();
        let git_repo = git_tailor::repo::Git2Repo::open(dir.path().to_path_buf()).unwrap();

        let bounds = resolve_oid_bounds(&git_repo, &Cli::parse_from(["gt"]))
            .unwrap()
            .expect("the feature commits");

        assert_eq!(bounds.reference_oid, Oid::from(branch_tip));
    }

    #[test]
    fn without_any_candidate_the_error_names_them_all() {
        let error = base_of(&MockRepo::default(), &[]).unwrap_err().to_string();
        for name in [
            "origin/main",
            "origin/master",
            "main",
            "master",
            "gt <branch>",
        ] {
            assert!(error.contains(name), "{error}");
        }
    }

    /// What [`load_with_progress`] leaves behind when it fails partway.
    fn loading() -> AppMode {
        AppMode::Loading {
            title: "Loading Commits",
            message: "Loading commits\u{2026}",
            progress: None,
            escape: None,
        }
    }

    /// The loading dialog ignores every key but Ctrl-C, which quits.
    #[test]
    fn a_failed_reload_goes_back_to_the_view_it_started_from() {
        let mut app = AppState {
            mode: loading(),
            ..AppState::default()
        };

        settle_reload(
            &MockRepo::default(),
            &mut app,
            AppMode::CommitList,
            None,
            Err(anyhow::anyhow!("unreadable file")),
            None,
        );

        assert!(matches!(app.mode, AppMode::CommitList), "{:?}", app.mode);
        assert!(app.status.is_error);
    }

    fn commit(oid: &str) -> CommitInfo {
        CommitInfo {
            oid: VirtualOid::Real(Oid::new(oid.repeat(40))),
            summary: String::new(),
            summary_key: Default::default(),
            author: None,
            date: None,
            parent_oids: vec![],
            message: String::new(),
            author_email: None,
            author_date: None,
            committer: None,
            committer_email: None,
            commit_date: None,
        }
    }

    /// What [`load_with_progress`] leaves after loading `commits`: the list,
    /// with the cursor at the natural position.
    fn loaded_list(oids: &[&str]) -> AppState {
        loaded_list_of(oids.iter().map(|oid| commit(oid)).collect())
    }

    fn loaded_list_of(commits: Vec<CommitInfo>) -> AppState {
        let selection_index = commits.len() - 1;
        let rows = commits
            .into_iter()
            .map(|commit| CommitDiff {
                commit,
                files: vec![],
            })
            .collect();
        AppState {
            mode: AppMode::CommitList,
            list: CommitListState::with_selection(rows, selection_index),
            ..AppState::default()
        }
    }

    fn matching(oids: &[&str]) -> Vec<CommitInfo> {
        oids.iter()
            .map(|oid| CommitInfo {
                message: "match".to_string(),
                ..commit(oid)
            })
            .collect()
    }

    fn shown(oid: &str, offset: usize) -> Option<Shown> {
        Some(Shown {
            oid: commit(oid).oid,
            offset,
        })
    }

    /// The current match is the one at the row the reload lands on, not the
    /// row the cursor was on while the list was replaced.
    #[test]
    fn a_reload_points_an_active_search_at_the_row_it_lands_on() {
        let mut app = loaded_list_of(matching(&["a", "b", "c"]));
        app.list.adopt_search("match".to_string());
        assert_eq!(app.list.search.state.match_index, Some(2));

        settle_reload(
            &MockRepo::default(),
            &mut app,
            AppMode::CommitList,
            None,
            Ok(true),
            Some(0),
        );

        assert_eq!(app.list.search.state.match_index, Some(0));
    }

    /// A canceled reload leaves the rows, and so the current match, as they were.
    #[test]
    fn a_canceled_reload_keeps_the_current_match() {
        let mut app = loaded_list_of(matching(&["a", "b", "c"]));
        app.list.adopt_search("match".to_string());
        app.list.search.state.match_index = Some(0);

        settle_reload(
            &MockRepo::default(),
            &mut app,
            AppMode::CommitList,
            None,
            Ok(false),
            None,
        );

        assert_eq!(app.list.search.state.match_index, Some(0));
    }

    /// A commit made in the meantime moves the row the detail view was on, so
    /// keeping the row would quietly show another commit.
    #[test]
    fn refreshing_the_detail_view_stays_on_the_commit_it_shows() {
        let mut app = loaded_list(&["a", "b", "new"]);

        settle_reload(
            &MockRepo::default(),
            &mut app,
            AppMode::CommitDetail,
            shown("b", 0),
            Ok(true),
            None,
        );

        assert!(matches!(app.mode, AppMode::CommitDetail), "{:?}", app.mode);
        assert_eq!(app.list.selection_index, 1);
    }

    #[test]
    fn refreshing_the_detail_view_keeps_its_scroll_position() {
        let mut app = loaded_list(&["a", "b"]);

        settle_reload(
            &MockRepo::default(),
            &mut app,
            AppMode::CommitDetail,
            shown("b", 300),
            Ok(true),
            None,
        );

        assert_eq!(app.detail.v.offset, 300);
    }

    /// Rewritten or committed away: there is nothing left to stay on.
    #[test]
    fn refreshing_a_commit_no_longer_listed_goes_back_to_the_list() {
        let mut app = loaded_list(&["a", "c"]);

        settle_reload(
            &MockRepo::default(),
            &mut app,
            AppMode::CommitDetail,
            shown("b", 0),
            Ok(true),
            None,
        );

        assert!(matches!(app.mode, AppMode::CommitList), "{:?}", app.mode);
        assert!(app.status.message.is_some());
    }

    /// Leaving the detail view ends its search, or the next commit opened
    /// would show the old one's matches.
    #[test]
    fn refreshing_a_commit_no_longer_listed_ends_the_search() {
        let mut app = loaded_list(&["a", "c"]);
        app.detail.search.query = "foo".to_string();
        app.detail.search.active = true;
        app.detail.search.matches = vec![3, 7];

        settle_reload(
            &MockRepo::default(),
            &mut app,
            AppMode::CommitDetail,
            shown("b", 0),
            Ok(true),
            None,
        );

        assert!(!app.detail.search.active);
        assert!(app.detail.search.query.is_empty());
        assert!(app.detail.search.matches.is_empty());
    }

    /// The branch may already have moved, so the list on screen may name commits
    /// that are gone.
    #[test]
    fn a_canceled_reload_says_the_list_may_be_out_of_date() {
        let mut app = AppState::default();

        settle_reload(
            &MockRepo::default(),
            &mut app,
            AppMode::CommitList,
            None,
            Ok(false),
            None,
        );

        let message = app.status.message.as_deref().unwrap_or_default();
        assert!(message.contains("press R"), "{message:?}");
    }
}
