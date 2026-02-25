# AI Agent Guidelines for git-scissors

This document describes the architecture, design decisions, and conventions for
the git-scissors project.

## Project Overview

git-scissors is an open-source console tool for working with Git commits,
combining features from **tig** (interactive commit browsing) and **fragmap**
(chunk-cluster visualization showing how commits relate). It enables users to
browse, analyze, reorder, squash, and split commits on a branch.

- **License**: MIT
- **Language**: Rust
- **Key crates**: `ratatui`, `crossterm`, `git2`, `clap`, `anyhow`

## Architecture

### Crate Structure (Cargo workspace)

```
git-scissors/
├── Cargo.toml              # workspace root
├── crates/
│   ├── scissors-core/       # Domain model + git operations (library)
│   └── scissors-tui/        # TUI application (binary)
└── tests/
    └── fixtures/            # Script-generated test git repos
```

**scissors-core** is a pure library containing all git logic, domain types, and
the rebase engine. It is independently testable without the TUI. This enables
future non-TUI frontends (CLI batch mode, CI tooling, etc.).

**scissors-tui** is a thin TUI shell that renders state from core and dispatches
user actions back to core.

### scissors-core Modules

| Module       | Responsibility                                                |
|--------------|---------------------------------------------------------------|
| `repo`       | Open repository, find merge-base, list commits                |
| `branch`     | Branch and merge-base utilities                               |
| `commit`     | `CommitInfo` type — oid, summary, author, date, parent_oids   |
| `diff`       | `FileDiff`, `Hunk`, `DiffLine`, `CommitDiff` types            |
| `fragmap`    | Span extraction, overlap clustering, matrix generation        |
| `rebase`     | Cherry-pick-based reorder, squash, and split engine           |

### scissors-tui Modules

| Module                 | Responsibility                                   |
|------------------------|--------------------------------------------------|
| `app`                  | Application state machine (`AppMode` enum)       |
| `event`                | Input event reading and dispatch                 |
| `views::commit_list`   | Scrollable one-line-per-commit log               |
| `views::commit_detail` | Commit metadata + scrollable colored diff        |
| `views::fragmap`       | Chunk-cluster grid visualization                 |
| `views::squash`        | Squash source → target picker with preview       |
| `views::reorder`       | Grab-and-move commit reordering                  |
| `views::split`         | Per-file / per-hunk commit splitting             |
| `widgets::diff`        | Syntax-highlighted diff rendering widget         |
| `widgets::grid`        | Fragmap grid rendering widget                    |
| `widgets::scrollable`  | Generic scrollable list widget                   |

## Key Domain Model

### Commit & Diff Types

```
CommitInfo   { oid, summary, author, date, parent_oids }
FileDiff     { old_path, new_path, hunks: Vec<Hunk> }
Hunk         { old_start, old_lines, new_start, new_lines, lines: Vec<DiffLine> }
CommitDiff   { commit: CommitInfo, files: Vec<FileDiff> }
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

For mutations (reorder, squash, split), the rebase engine works at the libgit2
level:
- **Reorder**: Cherry-pick commits in new order onto merge-base using
  `Repository::cherrypick_commit` to produce new trees, then create new commits.
- **Squash**: Cherry-pick squash-target on top of destination commit, combine
  messages.
- **Split per-file**: From a commit's diff, create N commits each applying only
  one file's hunks. Uses `Diff::apply_to_tree` with filtered patches.
- **Split per-hunk**: Same approach at hunk granularity.

All mutations create new commits on a detached HEAD or temporary branch, then
fast-forward the original branch ref only on user confirmation (preview before
apply).

### Default scope

By default, the tool shows commits from `HEAD` back to the merge-base with
`main`. The base branch is configurable via CLI argument.

### TUI state machine

The application uses a modal state machine (`AppMode` enum) with these modes:

- `CommitList` — default view, scrollable commit log
- `CommitDetail(Oid)` — diff + metadata for one commit
- `FragMapView` — grid visualization
- `Squash { source: Oid }` — pick squash target
- `Reorder` — commit list with grab-and-move
- `Split(Oid)` — per-file or per-hunk splitting
- `Confirm(PendingAction)` — preview before applying mutation

### Standard ratatui event loop

```
loop {
    terminal.draw(|f| render(&app, f))?;
    match event::read()? {
        key => app.handle_key(key),
    }
    if app.should_quit { break; }
}
```

## Open Design Decisions

These are not yet resolved and should be decided during implementation:

1. **Conflict handling during mutations**: When a cherry-pick produces
   conflicts, should the tool offer in-TUI conflict resolution, or bail out and
   leave the working tree conflicted for manual resolution?

2. **Undo model**: Simplest approach is saving the original branch ref before
   mutation and offering a single "undo". Alternative: full undo stack.

3. **Performance for large repos**: The fragmap matrix can grow large. May need
   lazy computation (only analyze visible commits) or a configurable depth
   limit.

## Implementation Phases

1. **Foundation** — Project scaffold, repo/branch/diff reading, CommitList and
   CommitDetail views, integration tests with fixture repos.
2. **Fragmap** — Span extraction, clustering, matrix generation, grid widget.
3. **Mutations** — Cherry-pick-based reorder engine, squash, reorder and confirm
   views.
4. **Split** — Per-file and per-hunk tree manipulation, split view.
5. **Polish** — Configurable base branch, undo support, commit message editing,
   themes, help screen, crates.io packaging.

## Testing Strategy

### Principle: separate "what to do" from "how to do it in git"

The fragmap algorithm, rebase plan computation, and split selection logic are
pure functions over domain types — easily unit tested. The git2 interaction is
behind a trait boundary, integration tested with real temporary repos.

```
┌──────────────────────────────────────────────┐
│              scissors-tui                    │
│  (thin: renders AppState, dispatches keys)   │
│  tested with: TestBackend + insta snapshots  │
├──────────────────────────────────────────────┤
│              scissors-core                   │
│                                              │
│  ┌──────────────┐  ┌──────────────────────┐  │
│  │ Pure logic   │  │ trait GitRepo        │  │
│  │ (fragmap,    │  │                      │  │
│  │  rebase plan)│  │  ├─ Git2Repo (real)  │  │
│  │              │  │  └─ MockRepo (test)  │  │
│  │ unit tested  │  │                      │  │
│  │ with mocks   │  │  integration tested  │  │
│  │              │  │  with TempDir repos  │  │
│  └──────────────┘  └──────────────────────┘  │
└──────────────────────────────────────────────┘
```

### Trait-based abstraction over git2

Don't call `git2` directly from business logic. Define traits in scissors-core:

```rust
pub trait GitRepo {
    fn merge_base(&self, head: Oid, upstream: Oid) -> Result<Oid>;
    fn list_commits(&self, from: Oid, to: Oid) -> Result<Vec<CommitInfo>>;
    fn commit_diff(&self, oid: Oid) -> Result<CommitDiff>;
    fn cherry_pick(&self, commit: Oid, onto: Oid) -> Result<Oid>;
    fn create_commit(&self, tree: TreeId, parents: &[Oid], message: &str) -> Result<Oid>;
    fn update_branch(&self, name: &str, target: Oid) -> Result<()>;
    // ...
}
```

Two implementations:
- `Git2Repo` — the real one wrapping `git2::Repository`
- Mock/fake implementations for unit tests of higher-level logic

### Fixture repos for integration tests

For testing the real `Git2Repo` implementation and end-to-end flows, use
`tempfile::TempDir` with `git2::Repository::init()`:

```rust
pub struct TestRepo {
    pub dir: TempDir,       // dropped = cleaned up
    pub repo: Repository,
}

impl TestRepo {
    pub fn new() -> Self { /* init repo, create initial commit on main */ }
    pub fn commit_file(&self, path: &str, content: &str, message: &str) -> git2::Oid { ... }
    pub fn create_branch(&self, name: &str) { ... }
}
```

Tests read like specifications:

```rust
#[test]
fn squash_combines_two_commits() {
    let test = TestRepo::new();
    test.create_branch("feature");
    let c1 = test.commit_file("a.txt", "hello", "first");
    let c2 = test.commit_file("a.txt", "hello world", "second");

    let engine = RebaseEngine::new(&test.repo);
    let result = engine.squash(c2, c1).unwrap();

    assert_eq!(result.parent_count(), 1);
    assert_file_content(&test.repo, result, "a.txt", "hello world");
}
```

### What to test at each layer

| Layer                          | How to test                                           | Example                                                |
|--------------------------------|-------------------------------------------------------|--------------------------------------------------------|
| **Domain types**               | Plain unit tests, no git                              | Construct structs, assert properties                   |
| **Fragmap clustering**         | Unit tests with fabricated `CommitDiff` data          | Feed hand-crafted hunks, assert cluster grouping       |
| **Rebase planner**             | Unit tests with mock `GitRepo` trait                  | Assert correct sequence of cherry-picks for a reorder  |
| **`Git2Repo` implementation**  | Integration tests with `TempDir` repos                | Verify real commits are created correctly              |
| **Rebase engine e2e**          | Integration tests with `TempDir` repos                | Squash, reorder, split → verify resulting commit graph |
| **Conflict detection**         | Integration with repos having overlapping edits       | Assert conflicts are detected and reported             |
| **TUI views**                  | Snapshot testing with `ratatui::backend::TestBackend` | Render to buffer, assert cell contents via `insta`     |

### Test dependencies

```toml
[dev-dependencies]
tempfile = "3"           # TempDir for fixture repos
insta = "1"              # Snapshot testing (TUI + diff output)
pretty_assertions = "1"  # Better assertion diffs
```
