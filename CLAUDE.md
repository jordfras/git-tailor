# Claude Guidelines for git-tailor

This document describes the architecture, design decisions, and conventions for
the git-tailor project.

## Project Overview

git-tailor is an open-source console tool for working with Git commits,
combining features from **tig** (interactive commit browsing) and **fragmap**
(chunk-cluster visualization showing how commits relate). It enables users to
browse, analyze, reorder, squash, and split commits on a branch.

Every `.rs` file must begin with the Apache-2.0 license header. Use the
`new-rust-file` skill when creating new files.

## Architecture

The project combines a **library** (src/lib.rs) containing all git logic, domain
types, and the rebase engine with a **binary** (src/main.rs) providing the TUI
interface. The split is what keeps the git logic testable without a terminal.

**The library is not a public API.** The `gt` command line is the stable
interface; `git_tailor::*` is an implementation detail, and the version number
describes only the former. The library may change shape in any release — 3.1.0
moved `RepoWrite` to `&mut self`, moved methods between traits, and switched
message parameters from `&str` to `&[u8]`, in a minor bump. Never preserve a
library signature, re-export, or type for compatibility's sake: there is nobody
to be compatible with, and pretending otherwise is how the awkward parts of an
API calcify.

### Module Organization Convention

**Never use `mod.rs` files** in `src/` — follow Rust 2018+ module style:

- A module without sub-modules: `src/repo.rs`
- A module with sub-modules: `src/repo.rs` + `src/repo/*.rs`

**Exception — integration test helpers:** `tests/common/mod.rs` (and its
sub-modules like `tests/common/fake.rs`) use the `mod.rs` style intentionally.
This keeps every file directly inside `tests/` an actual test binary entry
point, making the layout unambiguous at a glance.

### Code Comments Convention

**Prefer code that needs no comment.** A clearer function name, a named
intermediate value, or a smaller function beats a paragraph explaining a long
one. Reach for a comment only when the code cannot be made to say it.

**Comments explain *why*, never *what*.** If it restates the line below it,
delete it.

**Keep it short enough that it gets read.** An inline comment is a line or two.
A function's doc may run longer when its contract genuinely needs it, but never
as narrative: state the fact, not the story. Keep what a reader cannot recover
from the code; drop the alternatives weighed, the bug that motivated it, and how
it was found — that belongs in the commit message or TASKS.md.

**One fact, one place.** Don't repeat at the call site what the field's doc, the
function's doc, or the test's name already says.

**Put it on what it describes.** When adding a function or statement, check you
haven't stranded an existing comment above the wrong thing.

**No separator comments.** A banner such as `// ===== Section =====` marks a
boundary the module structure should draw. When a file seems to want one, split
it into submodules instead: a banner goes stale as code moves around it, and a
module boundary does not.

Don't reference the current task, a review, or a PR discussion; that context is
gone once the commit lands.

❌ Bad (restates the obvious):
```rust
// Open repository from current directory
let repo = git2::Repository::open(".")?;
```

✅ Good (explains *why* or provides non-obvious context):
```rust
// HEAD might be detached, so target() can fail
let head_oid = repo.head()?.target()?;
```

❌ Bad (true, and nobody will read it):
```rust
// A deletion's hunk removes every line, so applying it leaves nothing.
// The path has to go with it: writing the empty result back as a blob
// produces a piece that truncates the file rather than deleting it, and
// the pieces still sum to the original commit so nothing downstream
// notices.
```

✅ Good (same fact, one breath):
```rust
// A deletion's hunk removes every line, so the path has to go with it:
// writing the empty result back as a blob truncates the file instead of
// deleting it.
```

### Spelling Convention

**American English everywhere** — not only in what a user reads. Documentation,
commit messages, code comments, identifiers, and status-bar strings all use
`color`, `behavior`, `canceled`, `gray`, `normalize`, `center`. British spelling
in an identifier is worse than in prose, because it has to be matched exactly
from then on.

### Newtypes for Indices and Ids

An index or id into one particular collection gets a newtype, not a bare
`usize` or `u32`: a commit position, a hunk position, a file lineage, a list in
a `SharedTailLists`. The compiler then rejects passing a hunk position where a
commit position is expected, or a list id where a node index is expected. As
bare integers those mistakes type-check, and the bug is silent. `CommitPos`,
`ChangePos`, `HunkPos` (`src/fragmap/position.rs`), `FileId` and `ListId` are
examples.

Not every integer needs one: a count, or a loop index used within a few lines,
stays plain.

### Code Quality

After any Rust code change, run `cargo fmt`, `cargo clippy --all-targets`, and
`cargo test`. The codebase maintains zero clippy warnings.

### Commit Conventions

Use conventional commit prefixes: `feat:`, `fix:`, `test:`, `refactor:`,
`docs:`, `chore:`, `tasks:`. Each commit represents one logical change.

**Bug fixes — TDD:** write a failing test first, commit it alone with a
`test:` prefix, then implement the fix as a separate, following commit. Never
combine the two in one commit — verify the test actually fails before the fix
lands and passes after. Skip the test only if the bug cannot be exercised by
one.

**Design fit over diff size.** If the existing structure is a poor fit for a
change — fragile, duplicated, or poorly abstracted — propose a preparatory
refactoring commit first. Unrelated cleanup is out of scope.

### Fragmap (chunk clustering)

Each hunk is represented as a **FileSpan** (file path + line range). Overlapping
or adjacent spans across commits are merged into **SpanClusters**. A matrix of
`commits × clusters` shows which commits touch which clusters. Two commits
"conflict" (relate) when they share a cluster.

**Algorithm:**
1. For each commit, extract all hunks → convert to FileSpans.
2. Merge overlapping/adjacent spans across commits into clusters.
3. Build the matrix: for each (commit, cluster), mark the TouchKind.
4. Two commits conflict if they share a cluster.

## Design Decisions

### Git interaction: pure git2 (no git CLI dependency)

All git operations — both reads and mutations — use the `git2` crate (libgit2
bindings). The tool does **not** shell out to the `git` CLI.

**One repository handle.** `Git2Repo` owns a single `git2::Repository` and
everything goes through it. Never open a second handle onto the same repository
— not for convenience, and above all not to obtain a `&mut` where the
surrounding code only has `&self`. Two handles carry two index caches, so a
write through one leaves the other stale, and the bugs that follow are silent
and timing-dependent.

If an operation mutates, it takes `&mut self` and the signature says so. Some
libgit2 calls (`stash_save2`, `stash_apply`) require `&mut` — that is the API
being honest about what they do, and the fix is to propagate the `&mut`, never
to conjure a second handle around it. A wide but mechanical diff is the right
price for a signature that does not lie.

**Nothing reaches the working tree unchecked.** A working-tree write — checking
out a tree, checking out an index, or hard-resetting — can land on top of a file
git has no record of, and that content exists nowhere else: not in undo, not in
the reflog, not in the stash. Every such write goes through `reset_worktree`,
`refuse_index_collisions` or `refuse_tree_collisions`, which refuse first and
name the files. Do not call `checkout_head`, `checkout_index` or `reset` on the
inner repository from anywhere else, and check before moving the ref where the
caller can still back out cheaply — that is what makes a refusal free.

Never reach for `CheckoutBuilder::remove_untracked` to clean up after an
operation. libgit2 does not scope it to what the operation wrote; it removes
every untracked file under the checkout, the user's own included. Work out which
paths the operation put there and remove exactly those.

**A rewrite may only act on the repository it was shown.** Every operation is
chosen against a commit list read at some earlier moment and is handed the tip
that list was built from; `refuse_if_branch_moved` checks the branch still holds
it, at every entry point and again when a paused conflict resumes. A paused
conflict also records the branch it belongs to, because comparing tips cannot
tell two branches apart when they sit on the same commit — and `advance_branch_ref`
writes to whatever HEAD resolves to *now*. The session lock does not cover this:
it keeps another git-tailor out, not `git commit` in another terminal.

**One session per working tree.** The journal records an operation as in
progress, and nothing in that record says whether the process that wrote it is
still alive — so a second git-tailor reads a *live* operation as a crashed one.
`session_lock` supplies the missing fact with `File::try_lock` held for the
session; the operating system releases it however the process dies, so a crash
cannot strand it. Anything that can rewrite history takes it; read-only paths
such as `--static` deliberately do not. This is what sets the crate's
`rust-version`.

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

### Rendering performs no I/O

`render` functions draw what is already in `AppState` — no `GitRepo` call, no
`std::fs`, no subprocess. The draw closure runs on every frame, so a read there
is paid per keystroke and resize. Load in `dispatch` in response to an
`AppAction` instead; render signatures take no repository to keep it that way.


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

For testing the real `Git2Repo` implementation and end-to-end flows, use the
`TestRepo` helper in `tests/common/`, which wraps `tempfile::TempDir` and
`git2::Repository::init()`.

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
