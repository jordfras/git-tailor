# TASKS Checklist

Guidelines:
- Each task line: `- [ ] T### P? category - Title (Flags: ...)`
- Priorities: P0 (urgent) → P3 (low).
- Categories: bug | feat | fix | idea | human.
- Flags (optional): CLARIFICATION, HUMAN INPUT, HUMAN TASK, DUPLICATE.
- Mark completion by [ ] → [X]. Keep changes atomic (one commit per task).
- Mark won't-do tasks by [ ] → [-] and add `WONT DO` to Flags.
- Completed tasks are archived in TASKS-COMPLETED.md.


## UNCATEGORIZED

## Architecture & Robustness
- [X] T216 P2 feat - Add a persistent operation journal for crash safety: the
  cherry-pick rebase operations (move, drop, squash, fixup, reword, split) hold
  their in-flight state only in memory — in particular `ConflictState`
  (`original_branch_oid`, `new_tip_oid`, `remaining_oids`, the conflicting commit
  and files, etc.) lives in `AppState` while the user resolves a conflict. By
  that point the branch ref has already been advanced to a partial tip and the
  working tree holds conflict markers, so if gt is killed mid-operation the
  remaining-work state is lost: the operation cannot be resumed and the repo is
  left mid-conflict. Persist operation state to a durable journal under `.git/`
  (e.g. `.git/git-tailor/journal` for the serialized `ConflictState`, plus a ref
  such as `refs/git-tailor/orig` recording the pre-operation tip so the original
  commits are pinned against `git gc`). Write/refresh the journal when a mutating
  operation starts and when it enters a conflict; clear it on successful
  completion or abort. On startup, detect a leftover journal entry (an
  interrupted operation) and offer the user a recovery dialog: **resume** the
  rebase from the persisted `ConflictState` / `remaining_oids`, or **abort** by
  restoring the branch ref to the recorded original tip and cleaning the working
  tree. Keep this git2-native — do NOT write or depend on git's private
  `.git/rebase-merge/` format, so `git rebase --continue` / `--abort` will not
  act on this journal (recovery is via gt); the reflog remains a manual fallback
  (`git reset --hard <branch>@{1}`). Add integration tests that build a
  `ConflictState`, persist the journal, drop and reopen the repo handle, and
  assert the interrupted operation is detected and that both resume and abort
  restore correct state.
  NOTE: replacing the cherry-pick engine with `git2::Rebase` was investigated
  and rejected — libgit2 only exposes the non-interactive, range-based rebase
  (`git_rebase_init` over `upstream..branch`) and cannot express git-tailor's
  reordering operations (move, non-adjacent squash), which require an arbitrary
  commit order; its in-memory mode also writes no on-disk recovery state. A
  native journal delivers the crash-safety goal for *all* operations and is the
  shared foundation for undo (T218).
- [X] T218 P2 feat - Add undo/redo of history-rewriting operations via an
  operation stack: because every gt mutation (move, drop, squash, fixup, reword,
  split) only builds new commits and advances the branch ref — the previous
  commits remain in the object database — undo needs no per-operation inverse; it
  simply restores the branch ref to the tip OID recorded before the operation and
  checks out. Maintain a stack of operation records `{ label, tip_before,
  tip_after }` persisted alongside the T216 journal; **undo** pops the top record
  and restores `tip_before`, **redo** restores `tip_after` and pushes it back,
  with multiple levels supported by walking the stack. Pin the recorded tips
  against `git gc` by writing refs under `refs/git-tailor/undo/<n>` (a plain file
  holding a SHA does not protect objects from gc — only refs/reflogs do). Bind
  undo and redo to free keys in the commit-list view (`u` is taken by reload and
  `r` by reword, so choose unused keys) and document them in the help dialog.
  Safety: run the same dirty-state guard the operations use before undoing (a
  hard reset would clobber uncommitted changes), and validate that HEAD still
  matches the expected `tip_after` before allowing undo — if the user rewrote
  history via external git the stack is stale and must be invalidated or trimmed.
  Add integration tests: perform each operation, undo and assert history/file
  contents match the pre-operation state, redo and assert they match the
  post-operation state, plus multi-level undo/redo and stale-stack invalidation.
  Depends on T216 (journal infrastructure).
- [ ] T219 P2 feat - Add opt-in auto-stash so dirty-working-tree operations just
  work: operations that currently refuse when the working tree has staged or
  unstaged changes (`move`, `drop`, `squash`, `fixup` via `check_no_dirty_state`,
  and `undo`/`redo`, which hard-reset the tree) should, when auto-stash is
  enabled, automatically stash the dirty state, run the operation, then restore
  it afterwards instead of bailing. Gate it behind a new CLI flag `--autostash`
  with a `GT_AUTOSTASH` env binding (default off, mirroring `--reverse` /
  `GT_REVERSE`), matching git's own `rebase.autoStash` ergonomics. Requirements:
  * Preserve the staged/unstaged split exactly: changes staged before the
    operation must be staged again afterwards, and unstaged changes must come
    back unstaged. (git2 supports this via `stash_save` then
    `stash_apply`/`stash_pop` with `REINSTATE_INDEX`; alternatively unstage the
    index and take a second stash so the two sets restore independently.) Include
    untracked files so nothing is lost.
  * Conflict-bearing operations: when the operation enters `RebaseConflict` the
    working tree holds conflict markers and the stash cannot be popped yet —
    defer the unstash until the operation truly finishes (after
    `rebase_continue` completes) or is aborted (`rebase_abort`), restoring the
    original staged/unstaged state in both cases. Surface a clear error if the
    stash cannot be reapplied cleanly (it conflicts with the rebased result)
    rather than silently dropping it.
  * Crash safety: record the stash reference in the operation journal (T216) so
    that if gt is killed between stashing and restoring, the recovery flow can
    reapply (or at least point the user at) the stash instead of leaving work
    stranded in the stash list.
  * Undo/redo (T218): `undo` / `redo` reset the working tree, so with auto-stash
    on they must stash before and restore after, the same as forward operations,
    keeping the user's in-progress edits intact across an undo/redo. The
    dirty-state guard in `apply_undo` / `apply_redo` should defer to the
    auto-stash path when enabled.
  * Plumb the flag from `cli.rs` into `AppState` / the repo layer and thread it
    to every guarded operation; when disabled, behaviour is unchanged (still
    refuse with the current message).
  Add integration tests covering: staged-only, unstaged-only, and mixed dirty
  state restored exactly after move/squash; the conflict path (stash reapplied
  after continue and after abort); untracked files preserved; and an
  undo-with-dirty-tree round trip. Depends on T216 (journal) and interacts with
  T218 (undo/redo).
- [ ] T222 P3 feat - Support gix (gitoxide) as an alternative git backend behind
  a lower-level raw-git trait: investigate and lay the groundwork for building
  git-tailor against `gix` (pure-Rust, no libgit2/C dependency — simpler static
  builds, no C/OpenSSL build deps) instead of `git2`. The current `GitRepo`
  trait is high-level (it exposes whole operations like `drop_commit`,
  `squash_commits`, the cherry-pick chain, journal, and undo), so implementing it
  directly for a second backend would duplicate all the backend-agnostic
  orchestration (planning, cherry-pick replay, journal, undo). Instead introduce
  a *lower* trait — e.g. `GitBackend` / `RawGit` — capturing only the raw
  primitives the orchestration needs: open repo and expose `.git`/workdir paths;
  read HEAD and resolve refs; walk commits and read commit metadata; read trees
  and blobs; diff two trees; three-way merge / cherry-pick a commit in memory;
  apply a diff to a tree; create commits; create/update/delete refs (with reflog
  messages); read/write the index and conflict stages; checkout (incl. writing
  conflict markers); stash save/apply; read git config. Refactor today's
  `Git2Repo` so the orchestration in `git2_impl/*` (cherry-pick chain,
  drop/move/squash/split planning, journal, undo) depends only on this lower
  trait, and the two backends implement just the raw operations.
  Phased:
  * Spike: audit which git2 calls the orchestration relies on and whether gix has
    equivalents. The known risk is gix's maturity for tree merges / cherry-pick,
    `apply_to_tree`, the index/conflict-stage model, checkout-with-conflicts, and
    stash — if those are missing we may have to reimplement them in Rust or
    conclude gix is not yet viable (a valid outcome to document). Choose the
    trait-seam granularity (too low ⇒ we reimplement merge logic ourselves; too
    high ⇒ duplication).
  * Refactor: extract the `GitBackend` trait and move the raw git2 calls behind
    it, leaving the orchestration backend-agnostic. This has standalone value and
    de-risks the rest even before gix lands.
  * gix backend: implement `GitBackend` for gix behind a Cargo feature
    (`backend-git2` default vs `backend-gix`, likely mutually exclusive so a
    build pulls only one backend's deps; gix is MIT/Apache so `cargo deny` stays
    green, but it adds many crates — keep it behind the feature).
  * Parity tests: run the existing `tests/` integration suite against both
    backends (e.g. rstest `#[case]` per backend, or a feature-gated CI matrix) to
    prove identical end-state behaviour.
  Keep the `GitRepo` trait interface unchanged so the TUI, journal, and undo are
  unaffected regardless of backend.

## Interactivity — Staging & Committing
- [ ] T220 P2 feat - Stage all unstaged changes from within git-tailor: add a
  key binding in the commit list (e.g. `a` for "add", currently unused) that
  stages every unstaged working-tree change — modifications, additions
  (untracked files), and deletions — equivalent to `git add -A`. Add a
  `stage_all` method to the `GitRepo` trait (git2: `Index::add_all(["*"], …)`
  plus `update_all` to capture deletions, then `Index::write`) and wire the key
  through `commit_list::handle_key` and a new `AppAction`, reloading afterwards
  so the synthetic "staged" / "unstaged" rows refresh. Show a status message,
  including a no-op message when there is nothing to stage. Document the key in
  the help dialog. Scope: staging *all* changes at once is enough for now —
  per-file or per-hunk staging is out of scope.
- [ ] T221 P2 feat - Commit staged changes from within git-tailor: add a key
  binding in the commit list (e.g. `c` for "commit", currently unused) that
  creates a new commit from the currently staged changes. Open the configured
  editor (reuse `edit_message_in_editor`) for the commit message; if the message
  is non-empty, build a tree from the index and create a commit with the current
  HEAD as parent, advancing the branch ref (cancel on an empty message, as
  reword does). Add a `commit_staged(message)` method to the `GitRepo` trait, and
  refuse with a clear message when nothing is staged. Reload afterwards so the
  new commit appears and the "staged" synthetic row clears; document the key in
  help. Scope: committing all staged changes with an editor-provided message is
  enough for now. Decide how this interacts with undo/redo (T218): a plain commit
  is additive rather than history-rewriting, so it need not be undoable in this
  task — but record the decision rather than leaving it implicit.

## Interactivity — Commit Detail View
- [ ] T138 P3 feat - Add syntax highlighting to diff code in commit detail view:
  use `syntect` (already a transitive dependency) to highlight the code portions
  of diff hunks based on the file extension / language; convert syntect's
  `(Style, &str)` token pairs to ratatui `Span`s with mapped foreground colors;
  diff-specific styling (green/red for added/removed lines, hunk headers) should
  remain and take precedence — syntax colors apply to the code content within
  those lines; add a `syntect::parsing::SyntaxSet` and
  `syntect::highlighting::ThemeSet` to the application state (loaded once at
  startup) so highlighting is performed per-hunk on demand without re-loading
  assets; consider caching highlighted output per commit to avoid
  re-highlighting on every render
- [ ] T166 P3 feat - Increase and decrease diff context lines in commit detail
  view with `+` and `-`: pressing `+` should increase the number of context
  lines shown around each hunk (default 3, matching git's default), and `-`
  should decrease it (minimum 0); store the context line count in `AppState` and
  pass it through to `commit_diff` (or re-render the cached diff with the new
  context); changing the value should trigger a re-fetch or re-render of the
  diff so the change is immediately visible; show the current context line count
  in the footer or status line so the user knows the active value

## CLI — Shell Completion
- [ ] T140 P3 feat - Add shell completion for CLI options: use `clap_complete`
  to generate static completion scripts (bash, zsh, fish) for all flags and
  value_enum variants (e.g. `--squashable-scope`). NOTE: zero-setup completions
  require distribution via a package manager (apt, brew, etc.) that can deposit
  the script in the right system directory at install time; users installing via
  `cargo install` will still need a manual one-time setup step.
- [ ] T141 P3 feat - Add branch/tag completion for the BASE argument: extend the
  completion mechanism from T140 so that the positional `base` argument offers
  branch and tag candidates by querying `git2` for local branches,
  remote-tracking refs, and tags; degrade gracefully if the current directory is
  not inside a git repository. Same distribution requirement as T140.
- [ ] T210 P3 feat - Add `gt completions` subcommand to generate and install
  shell completion scripts: `gt completions --shell <bash|zsh|fish>` prints the
  generated script to stdout; adding `--install` writes it to the conventional
  user-local path without requiring root — bash:
  `~/.local/share/bash-completion/completions/gt`, zsh:
  `~/.local/share/zsh/site-functions/_gt`, fish:
  `~/.config/fish/completions/gt.fish`; print a hint after install explaining
  any shell-reload step needed (e.g. `source ~/.bashrc`); this removes the
  manual setup burden for `cargo install` users and makes T140/T141 completions
  self-contained without depending on a package manager

## Build & CI
- [ ] T118 P2 feat - Set up GitHub Releases with pre-built binaries: create
  `.github/workflows/release.yml` that triggers on version tags (`v*`), builds
  the `gt` binary for `x86_64-unknown-linux-musl` (fully static, covers WSL2 and
  all Linux distros), `x86_64-pc-windows-msvc` (Windows native), and optionally
  `aarch64-unknown-linux-gnu` and `aarch64-apple-darwin`; use
  `taiki-e/upload-rust-binary-action` to strip, archive, and attach binaries to
  the GitHub Release automatically; the musl target should produce a zero
  shared-library binary (add `RUSTFLAGS=-C target-feature=+crt-static` if
  needed) so no system libs beyond the kernel are required

## Notes
