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
