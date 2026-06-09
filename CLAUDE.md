# Claude Guidelines for git-tailor

This document describes the architecture, design decisions, and conventions for
the git-tailor project.

## Project Overview

git-tailor is an open-source console tool for working with Git commits,
combining features from **tig** (interactive commit browsing) and **fragmap**
(chunk-cluster visualization showing how commits relate). It enables users to
browse, analyze, reorder, squash, and split commits on a branch.

- **License**: Apache-2.0
- **Language**: Rust
- **Key crates**: `ratatui`, `crossterm`, `git2`, `clap`, `anyhow`

Every `.rs` file must begin with the Apache-2.0 license header. Use the
`new-rust-file` skill when creating new files.

## Architecture

### Crate Structure

```
git-tailor/
├── Cargo.toml              # package manifest
├── src/
│   ├── lib.rs              # Library root (re-exports domain types, module declarations)
│   ├── main.rs             # Binary entry point (event loop, side effects)
│   ├── cli.rs              # Command-line argument definitions (clap)
│   ├── loader.rs           # Startup loading: commit walking, progress display
│   ├── terminal_guard.rs   # RAII guard owning TUI terminal setup / teardown
│   ├── external_tool.rs    # Suspend/restore TUI around external processes
│   ├── tests.rs            # In-library unit tests (MockRepo stubs)
│   ├── app.rs              # AppMode enum, AppAction enum, SquashMode
│   ├── app/
│   │   ├── keymap.rs       # KeyCommand enum + read_event()
│   │   └── state.rs        # AppState struct
│   ├── domain.rs           # Domain module declarations
│   ├── domain/
│   │   ├── commit.rs       # CommitInfo, Oid, VirtualOid
│   │   └── diff.rs         # FileDiff, Hunk, DiffLine, CommitDiff, DeltaStatus, DiffLineKind
│   ├── editor.rs           # External editor integration (commit message editing)
│   ├── mergetool.rs        # External merge tool integration
│   ├── repo.rs             # GitRepo trait definition
│   ├── repo/
│   │   ├── git2_impl.rs    # Git2Repo: libgit2-backed GitRepo implementation
│   │   └── git2_impl/
│   │       ├── cherry_pick.rs  # Cherry-pick chain helpers
│   │       ├── conflict.rs     # Conflict detection and state
│   │       ├── drop_op.rs      # Drop commit operation
│   │       ├── hunks.rs        # Hunk extraction and patch building
│   │       ├── move_op.rs      # Move commit operation
│   │       ├── reads.rs        # Read-only git operations
│   │       ├── reword_op.rs    # Reword commit operation
│   │       ├── split_op.rs     # Split commit operation
│   │       └── squash_op.rs    # Squash/fixup operation
│   ├── fragmap.rs          # Span extraction, clustering, matrix generation
│   ├── fragmap/
│   │   └── spg.rs          # Span Propagation Graph algorithm
│   ├── views.rs            # View module declarations
│   ├── views/
│   │   ├── commit_list.rs  # Scrollable commit log with fragmap
│   │   ├── commit_detail.rs # Commit metadata + scrollable colored diff
│   │   ├── conflict.rs     # Rebase conflict resolution dialog
│   │   ├── dialog.rs       # Shared dialog rendering helpers
│   │   ├── drop.rs         # Drop commit confirmation
│   │   ├── help.rs         # Help overlay
│   │   ├── hunk_groups.rs  # Hunk group detail rendering
│   │   ├── list_nav.rs     # Shared navigation helper for list-picker dialogs
│   │   ├── loading.rs      # Loading screen rendering
│   │   ├── main_view.rs    # Shared layout (commit list + fragmap + detail)
│   │   ├── move_select.rs  # Move commit target selection
│   │   ├── split_select.rs # Split strategy selection dialog
│   │   ├── squash_select.rs # Squash/fixup target selection
│   │   └── theme.rs        # Fragmap rendering theme trait and built-in themes
│   ├── static_views.rs     # Static (non-interactive) view module declarations
│   └── static_views/
│       └── fragmap.rs      # CLI fragmap output (non-TUI)
└── tests/                   # Integration tests (TempDir repos + TUI snapshots)
```

The project combines a **library** (src/lib.rs) containing all git logic, domain
types, and the rebase engine with a **binary** (src/main.rs) providing the TUI
interface. The library is independently testable and can be used for future
non-TUI frontends (CLI batch mode, CI tooling, etc.).

### Module Organization Convention

**Never use `mod.rs` files** in `src/` — follow Rust 2018+ module style:

- A module without sub-modules: `src/repo.rs`
- A module with sub-modules: `src/repo.rs` + `src/repo/*.rs`

**Exception — integration test helpers:** `tests/common/mod.rs` (and its
sub-modules like `tests/common/fake.rs`) use the `mod.rs` style intentionally.
This keeps every file directly inside `tests/` an actual test binary entry
point, making the layout unambiguous at a glance.

### Code Comments Convention

**Avoid redundant comments.** Comments should explain *why* or provide context,
not restate what the code already clearly expresses.

❌ Bad (comment restates the obvious):
```rust
// Open repository from current directory
let repo = git2::Repository::open(".")?;
```

✅ Good (explains *why* or provides non-obvious context):
```rust
// HEAD might be detached, so target() can fail
let head_oid = repo.head()?.target()?;
```

### Code Quality

After any Rust code change, run `cargo fmt`, `cargo clippy --all-targets`, and
`cargo test`. The codebase maintains zero clippy warnings.

### Commit Conventions

Use conventional commit prefixes: `feat:`, `fix:`, `test:`, `refactor:`,
`docs:`, `chore:`, `tasks:`. Each commit represents one logical change.

**Bug fixes — TDD:** write a failing test first, commit it with `test:` prefix,
then implement the fix. Skip only if the bug cannot be exercised by a test.

**Design fit over diff size.** If the existing structure is a poor fit for a
change — fragile, duplicated, or poorly abstracted — propose a preparatory
refactoring commit first. Unrelated cleanup is out of scope.

### Commit & Diff Types

```
CommitInfo   { oid: VirtualOid, summary, author, date, parent_oids, message,
               author_email, author_date, committer, committer_email, commit_date }
FileDiff     { old_path, new_path, status: DeltaStatus, hunks: Vec<Hunk> }
Hunk         { old_start, old_lines, new_start, new_lines, lines: Vec<DiffLine> }
CommitDiff   { commit: CommitInfo, files: Vec<FileDiff> }
Oid          — newtype wrapper around a 40-hex-char SHA string
VirtualOid   ∈ { Real(Oid), Staged, Unstaged }  — unifies real commits and synthetic working-tree rows
```

### Fragmap (chunk clustering)

Each hunk is represented as a **FileSpan** (file path + line range). Overlapping
or adjacent spans across commits are merged into **SpanClusters**. A matrix of
`commits × clusters` shows which commits touch which clusters. Two commits
"conflict" (relate) when they share a cluster.

```
FileSpan     { path, start_line, end_line }
SpanCluster  { spans: Vec<FileSpan>, commit_oids: Vec<Oid> }
FragMap      { commits, clusters, matrix: Vec<Vec<TouchKind>> }
TouchKind    ∈ { Added, Modified, Deleted, None }
```

**Algorithm:**
1. For each commit, extract all hunks → convert to FileSpans.
2. Merge overlapping/adjacent spans across commits into clusters.
3. Build the matrix: for each (commit, cluster), mark the TouchKind.
4. Two commits conflict if they share a cluster.

## Design Decisions

### Git interaction: pure git2 (no git CLI dependency)

All git operations — both reads and mutations — use the `git2` crate (libgit2
bindings). The tool does **not** shell out to the `git` CLI.

For mutations (reorder, squash, split), the rebase engine builds new commit
chains using `Repository::cherrypick_commit` (the in-memory variant) rather than
the `git2::Rebase` API. This cherry-pick chain approach was chosen because
operations like split-per-hunk, split-per-hunk-group, and squash require custom
tree surgery (`apply_to_tree`, `cherrypick_commit` for combining trees) that
cannot be expressed through the rebase todo-list model. The cherry-pick loop is
also simpler to reason about — all state lives in Rust structs rather than
libgit2's opaque rebase state machine.

- **Reorder**: Cherry-pick commits in new order onto merge-base.
- **Squash**: Cherry-pick squash-target on top of destination commit, combine messages.
- **Split per-file**: Create N commits each applying only one file's hunks via `Diff::apply_to_tree`.
- **Split per-hunk**: Same approach at hunk granularity.

All mutations build new commit chains and advance the branch ref immediately.
Confirmation dialogs (drop, large split) are shown before the operation starts.

### Default scope

By default, the tool shows commits from `HEAD` back to the merge-base with
the upstream default branch. The base is auto-detected via `origin/HEAD`
(e.g. `origin/main`), falling back to `main` when that ref is not set.
The base branch can be overridden with a positional CLI argument, or `--all`
can be passed to browse the complete repository history down to the root commit.

### TUI state machine

The application uses a modal state machine (`AppMode` enum) with these modes:

- `Loading { title, message, progress, skippable }` — startup loading screen
- `CommitList` — default view, scrollable commit log with fragmap
- `CommitDetail` — diff + metadata for the selected commit
- `SplitSelect { strategy_index }` — per-file / per-hunk / per-hunk-group picker
- `SplitConfirm(PendingSplit)` — confirmation for large splits
- `DropConfirm(PendingDrop)` — drop commit confirmation
- `RebaseConflict(Box<ConflictState>)` — merge conflict resolution dialog
- `SquashSelect { source_index, squash_mode: SquashMode }` — squash/fixup target picker
- `MoveSelect { source_index, insert_before }` — move commit target selection
- `Help(Box<AppMode>)` — help overlay (wraps previous mode)

### Standard ratatui event loop

```
loop {
    terminal.draw(|f| render(&app, f))?;
    let event = app::read_event()?;
    let action = app.mode.parse_key(event);
    app.clear_status_message();
    let result = match app.mode {
        CommitList => views::commit_list::handle_key(action, &mut app),
        CommitDetail => views::commit_detail::handle_key(action, &mut app),
        // ... other modes dispatch to their view module
    };
    match result {
        AppAction::Handled => {}
        AppAction::Quit => app.should_quit = true,
        AppAction::ReloadCommits => reload_commits(&git_repo, &mut app),
        // ... other side effects executed by main.rs
    }
    if app.should_quit { break; }
}
```

## Testing Strategy

### Principle: separate "what to do" from "how to do it in git"

The fragmap algorithm, rebase plan computation, and split selection logic are
pure functions over domain types — easily unit tested. The git2 interaction is
behind a trait boundary, integration tested with real temporary repos.

### Trait-based abstraction over git2

Don't call `git2` directly from business logic. All git operations go through
the `GitRepo` trait (defined in `repo.rs`). Two implementations exist:

- `Git2Repo` — the real one wrapping `git2::Repository`
- Mock/fake implementations for unit tests of higher-level logic

### Fixture repos for integration tests

For testing the real `Git2Repo` implementation and end-to-end flows, use
`tempfile::TempDir` with `git2::Repository::init()`:

```rust
pub struct TestRepo {
    pub _temp_dir: TempDir,  // dropped = cleaned up
    pub repo: Repository,
}

impl TestRepo {
    pub fn new() -> Self { /* init repo, configure user, create initial state */ }
    pub fn git_repo(&self) -> Git2Repo { /* open a Git2Repo handle */ }
    pub fn commit_file(&self, path: &str, content: &str, message: &str) -> git2::Oid { ... }
    pub fn create_branch(&self, name: &str) { ... }
}
```

### What to test at each layer

| Layer                          | How to test                                           |
|--------------------------------|-------------------------------------------------------|
| **Domain types**               | Plain unit tests, no git                              |
| **Fragmap clustering**         | Unit tests with fabricated `CommitDiff` data          |
| **Rebase planner**             | Unit tests with mock `GitRepo` trait                  |
| **`Git2Repo` implementation**  | Integration tests with `TempDir` repos                |
| **Rebase engine e2e**          | Integration tests with `TempDir` repos                |
| **Conflict detection**         | Integration with repos having overlapping edits       |
| **TUI views**                  | Snapshot testing with `ratatui::backend::TestBackend` |

### Test dependencies

```toml
[dev-dependencies]
insta = "1"              # Snapshot testing (TUI + diff output)
```
