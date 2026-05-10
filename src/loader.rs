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

// Startup loading: commit walking, progress display, and matrix confirmation.

use anyhow::{Context, Result};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use git_tailor::repo::GitRepo;
use git_tailor::{
    CommitDiff, CommitInfo, Oid, VirtualOid,
    app::{AppMode, AppState},
    fragmap, views,
};

use crate::cli::Cli;
use crate::terminal_guard::TerminalGuard;

/// Commits as loaded from git, before synthetic working-tree rows and virtual
/// OIDs are added to produce the final `app.commits` list.
type CommitsWithDiffs = Vec<(CommitInfo, Option<CommitDiff>)>;

pub fn load_initial_commits(
    git_repo: &impl GitRepo,
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

    let base = cli.base.clone().unwrap_or_else(|| {
        git_repo
            .default_branch()
            .unwrap_or_else(|| "main".to_string())
    });
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

/// Resolve the OID bounds for the TUI loading path (fast: no commit listing).
///
/// Returns `(head_oid, reference_oid, include_reference_oid)`, or `Ok(None)`
/// when there are definitely no commits to display.
pub fn resolve_oid_bounds(git_repo: &impl GitRepo, cli: &Cli) -> Result<Option<(Oid, Oid, bool)>> {
    let head_oid = git_repo.head_oid()?;

    if cli.all {
        let root_oid = git_repo.root_commit_oid()?;
        return Ok(Some((head_oid, root_oid, true)));
    }
    let base = cli.base.clone().unwrap_or_else(|| {
        git_repo
            .default_branch()
            .unwrap_or_else(|| "main".to_string())
    });
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

    Ok(Some((head_oid, reference_oid, false)))
}

/// Load commits and diffs with a live counter dialog, then optionally build the
/// hunk group matrix. Updates `app` in place and sets `app.mode = CommitList`.
///
/// When `app.commits` is non-empty (reload case), the dialog is shown as an
/// overlay on top of the existing commit list. When empty (initial load), the
/// dialog is shown full-screen.
///
/// Returns `Ok(false)` if the user pressed Ctrl-C during loading.
pub fn load_with_progress(
    git_repo: &impl GitRepo,
    terminal_guard: &mut TerminalGuard,
    app: &mut AppState,
    from_oid: &Oid,
    reference_oid: &Oid,
    include_reference_oid: bool,
    full: bool,
) -> Result<bool> {
    let total = git_repo.commit_walker(from_oid, reference_oid)?.count();
    let saved_mode = std::mem::replace(
        &mut app.mode,
        AppMode::Loading {
            title: "Loading Commits",
            message: "Loading commits\u{2026}",
            progress: Some((0, total)),
            skippable: false,
        },
    );
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
        app.mode = saved_mode;
        return Ok(false);
    };

    let (commits, diff_opts): (Vec<CommitInfo>, Vec<Option<CommitDiff>>) = {
        let mut ordered = raw;
        ordered.reverse();
        ordered
            .into_iter()
            .filter(|(c, _)| {
                include_reference_oid || c.oid != VirtualOid::Real(reference_oid.clone())
            })
            .unzip()
    };

    if commits.is_empty() {
        app.mode = saved_mode;
        return Ok(true);
    }

    let extra_diffs: Vec<CommitDiff> = [git_repo.staged_diff(), git_repo.unstaged_diff()]
        .into_iter()
        .flatten()
        .collect();

    let matrix = build_hunk_group_matrix(terminal_guard, app, diff_opts, &extra_diffs, full)?;

    let mut all_commits = commits;
    for d in &extra_diffs {
        all_commits.push(d.commit.clone());
    }
    app.commits = all_commits;
    app.fragmap = matrix;
    app.fragmap_scroll_offset = 0;
    app.detail_scroll_offset = 0;
    app.selection_index = select_initial_index(&app.commits);
    app.mode = AppMode::CommitList;

    Ok(true)
}

/// Walk commits from `from_oid` back to `reference_oid`, collecting each
/// commit paired with its fragmap diff. Renders a live counter dialog at
/// ~60 fps and polls for Ctrl-C between frames.
///
/// Returns `Ok(None)` if the user pressed Ctrl-C, `Ok(Some(raw))` on success.
/// The returned vec is in reverse-chronological order (newest first).
fn walk_commits(
    git_repo: &impl GitRepo,
    terminal_guard: &mut TerminalGuard,
    app: &mut AppState,
    from_oid: &Oid,
    reference_oid: &Oid,
    total: usize,
) -> Result<Option<CommitsWithDiffs>> {
    let walker = git_repo.commit_walker(from_oid, reference_oid)?;
    let mut raw: Vec<(CommitInfo, Option<CommitDiff>)> = Vec::new();

    let render_interval = std::time::Duration::from_millis(16);
    let mut last_render = std::time::Instant::now()
        .checked_sub(render_interval)
        .unwrap_or_else(std::time::Instant::now);

    for commit_result in walker {
        let commit = commit_result.context("Failed to load commits")?;
        let diff = commit
            .oid
            .as_oid()
            .and_then(|oid| git_repo.commit_diff_for_fragmap(oid).ok());
        raw.push((commit, diff));

        let now = std::time::Instant::now();
        if now.duration_since(last_render) >= render_interval {
            last_render = now;
            app.mode = AppMode::Loading {
                title: "Loading Commits",
                message: "Loading commits\u{2026}",
                progress: Some((raw.len(), total)),
                skippable: false,
            };
            terminal_guard
                .terminal()
                .draw(|frame| views::loading::render(app, frame))?;

            if crossterm::event::poll(std::time::Duration::ZERO)?
                && let Ok(Event::Key(KeyEvent {
                    code: KeyCode::Char('c'),
                    modifiers,
                    kind: KeyEventKind::Press,
                    ..
                })) = crossterm::event::read()
                && modifiers.contains(KeyModifiers::CONTROL)
            {
                return Ok(None);
            }
        }
    }

    Ok(Some(raw))
}

/// If `count` exceeds the threshold, drain buffered events, render the Y/N
/// confirmation dialog, and wait for the user's answer. Returns `true` if the
/// matrix should be built.
/// Build the fragmap incrementally in three phases, showing a progress dialog
/// for each phase. Returns `Ok(None)` when any diff is missing (matrix skipped)
/// or when the user presses Ctrl-C during file clustering (phase 1).
fn build_hunk_group_matrix(
    terminal_guard: &mut TerminalGuard,
    app: &mut AppState,
    diff_opts: Vec<Option<CommitDiff>>,
    extra_diffs: &[CommitDiff],
    full: bool,
) -> Result<Option<git_tailor::fragmap::FragMap>> {
    let Some(mut diffs) = diff_opts.into_iter().collect::<Option<Vec<CommitDiff>>>() else {
        return Ok(None);
    };
    diffs.extend_from_slice(extra_diffs);

    let mut builder = fragmap::FragMapBuilder::new(diffs, !full);
    let total = builder.total_files();

    // Phase 1: cluster files (interruptible with Ctrl-C).
    let render_interval = std::time::Duration::from_millis(16);
    let mut last_render = std::time::Instant::now()
        .checked_sub(render_interval)
        .unwrap_or_else(std::time::Instant::now);
    loop {
        let done = builder.step();
        let now = std::time::Instant::now();
        if now.duration_since(last_render) >= render_interval || done {
            last_render = now;
            app.mode = AppMode::Loading {
                title: "Hunk Group Matrix",
                message: "Clustering files\u{2026}",
                progress: Some((builder.files_done(), total)),
                skippable: true,
            };
            terminal_guard
                .terminal()
                .draw(|frame| views::loading::render(app, frame))?;
            if !done
                && crossterm::event::poll(std::time::Duration::ZERO)?
                && let Ok(Event::Key(KeyEvent {
                    code: KeyCode::Char('s') | KeyCode::Char('S'),
                    kind: KeyEventKind::Press,
                    ..
                })) = crossterm::event::read()
            {
                return Ok(None);
            }
        }
        if done {
            break;
        }
    }

    // Phase 2: deduplicate clusters.
    app.mode = AppMode::Loading {
        title: "Hunk Group Matrix",
        message: "Deduplicating clusters\u{2026}",
        progress: None,
        skippable: false,
    };
    terminal_guard
        .terminal()
        .draw(|frame| views::loading::render(app, frame))?;
    builder.run_dedup();

    // Phase 3: build matrix.
    app.mode = AppMode::Loading {
        title: "Hunk Group Matrix",
        message: "Building matrix\u{2026}",
        progress: None,
        skippable: false,
    };
    terminal_guard
        .terminal()
        .draw(|frame| views::loading::render(app, frame))?;
    Ok(Some(builder.finish_matrix()))
}

/// Choose the initial selection index for a commit list:
/// unstaged row if present, else staged row if present, else the last commit.
fn select_initial_index(commits: &[CommitInfo]) -> usize {
    if let Some(i) = commits.iter().rposition(|c| c.oid == VirtualOid::Unstaged) {
        return i;
    }
    if let Some(i) = commits.iter().rposition(|c| c.oid == VirtualOid::Staged) {
        return i;
    }
    commits.len().saturating_sub(1)
}
