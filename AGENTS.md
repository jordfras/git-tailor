# AI Agent Guidelines for git-tailor

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

## Architecture

### Crate Structure

```
git-tailor/
├── Cargo.toml              # package manifest
├── src/
│   ├── lib.rs              # Library root (domain types + module declarations)
│   ├── main.rs             # Binary entry point (CLI, event loop, side effects)
│   ├── app.rs              # TUI state machine (AppMode, AppState, key parsing)
│   ├── editor.rs           # External editor integration (commit message editing)
│   ├── mergetool.rs        # External merge tool integration
│   ├── repo.rs             # GitRepo trait definition
│   ├── repo/
│   │   └── git2_impl.rs    # Git2Repo: libgit2-backed GitRepo implementation
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
│   │   ├── main_view.rs    # Shared layout (commit list + fragmap + detail)
│   │   ├── move_select.rs  # Move commit target selection
│   │   ├── split_select.rs # Split strategy selection dialog
│   │   └── squash_select.rs # Squash/fixup target selection
│   ├── static_views.rs     # Static (non-interactive) view module declarations
│   └── static_views/
│       └── fragmap.rs      # CLI fragmap output (non-TUI)
└── tests/                   # Integration tests (TempDir repos + TUI snapshots)
```

The project combines a **library** (src/lib.rs) containing all git logic, domain
types, and the rebase engine with a **binary** (src/main.rs) providing the TUI
interface. The library is independently testable and can be used for future
non-TUI frontends (CLI batch mode, CI tooling, etc.).

### Library Modules

Domain types (`CommitInfo`, `FileDiff`, `Hunk`, `DiffLine`, `CommitDiff`,
`DeltaStatus`, `DiffLineKind`) are defined directly in `lib.rs`.

| Module         | Responsibility                                                |
|----------------|---------------------------------------------------------------|
| `repo`         | `GitRepo` trait + `Git2Repo` implementation (all git ops)     |
| `fragmap`      | Span extraction, SPG algorithm, clustering, matrix generation |
| `editor`       | External editor integration (commit message editing)          |
| `mergetool`    | External merge tool integration (conflict resolution)         |
| `static_views` | Non-interactive CLI output (fragmap rendering)                |

### TUI Modules

| Module                    | Responsibility                                      |
|---------------------------|-----------------------------------------------------|
| `app`                     | Application state machine, key parsing, event reading |
| `views::main_view`        | Shared layout (commit list + fragmap + detail pane) |
| `views::commit_list`      | Scrollable one-line-per-commit log with fragmap     |
| `views::commit_detail`    | Commit metadata + scrollable colored diff           |
| `views::squash_select`    | Squash/fixup target picker                          |
| `views::move_select`      | Move commit target selection                        |
| `views::split_select`     | Split strategy selection dialog                     |
| `views::drop`             | Drop commit confirmation dialog                     |
| `views::conflict`         | Rebase conflict resolution dialog                   |
| `views::help`             | Help overlay                                        |
| `views::hunk_groups`      | Hunk group detail rendering                         |
| `views::dialog`           | Shared dialog rendering helpers                     |

### Module Organization Convention

**Never use `mod.rs` files.** Follow Rust 2018+ module style:

- A module without sub-modules: `src/repo.rs`
- A module with sub-modules: `src/repo.rs` + `src/repo/*.rs`

Example:
```
src/
  lib.rs
  repo.rs              # declares: pub mod git2_impl; pub mod traits;
  repo/
    git2_impl.rs
    traits.rs
  views.rs             # declares sub-modules
  views/
    commit_list.rs
    commit_detail.rs
    fragmap.rs
```

This keeps the module tree clear and avoids the old `mod.rs` pattern.

### Code Comments Convention

**Avoid redundant comments.** Comments should explain *why* or provide context,
not restate what the code already clearly expresses.

❌ Bad (comment restates the obvious):
```rust
// Open repository from current directory
let repo = git2::Repository::open(".")?;

// Get HEAD as Oid
let head = repo.head()?;
```

✅ Good (explains *why* or provides non-obvious context):
```rust
// Use current directory since we want to operate on the active repo
let repo = git2::Repository::open(".")?;

// HEAD might be detached, so target() can fail
let head_oid = repo.head()?.target()?;
```

✅ Also good (doc comments for public APIs):
```rust
/// Find the merge-base (reference point) between HEAD and a given commit-ish.
/// Returns the OID of the common ancestor.
pub fn find_reference_point(commit_ish: &str) -> Result<String> {
```

When in doubt, let the code speak for itself. Use meaningful variable names and
clear structure instead of comments.

### Code Quality Workflow

**Always run formatting and linting after code changes.**

After modifying any Rust code, run these commands in order:

1. **`cargo fmt`** — Format code according to Rust style guidelines
2. **`cargo clippy --all-targets`** — Run linter to catch common mistakes and
   suggest improvements (includes examples, tests, and benchmarks)
3. **`cargo test`** — Run test suite

Fix any clippy warnings before committing. The codebase should maintain zero
warnings.

### Changelog

`CHANGELOG.md` follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
format. When completing a task, ask the user whether the change warrants a
changelog entry before committing. User-visible changes that typically need an
entry:

- New CLI flags or arguments → **Added**
- New TUI features or interactive behaviors → **Added**
- Changed behavior users will notice → **Changed**
- Bug fixes → **Fixed**

Internal-only changes (refactors, test additions, CI tweaks, doc corrections)
generally do **not** need a changelog entry — confirm with the user if unsure.



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

For mutations (reorder, squash, split), the rebase engine builds new commit
chains using `Repository::cherrypick_commit` (the in-memory variant) rather than
the `git2::Rebase` API. This cherry-pick chain approach was chosen because
operations like split-per-hunk, split-per-hunk-group, and squash require custom
tree surgery (`apply_to_tree`, `cherrypick_commit` for combining trees) that
cannot be expressed through the rebase todo-list model. The cherry-pick loop is
also simpler to reason about — all state lives in Rust structs rather than
libgit2's opaque rebase state machine.

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
│            TUI (main.rs + views)             │
│  (thin: renders AppState, dispatches keys)   │
│  tested with: TestBackend + insta snapshots  │
├──────────────────────────────────────────────┤
│          Library (lib.rs + modules)          │
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

Don't call `git2` directly from business logic. Define traits in the library:

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
