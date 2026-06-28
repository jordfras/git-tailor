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
- [x] T223 P3 feat - Add a `--clean-journal` CLI option that wipes all
  git-tailor recovery state: delete the journal file
  (`<gitdir>/git-tailor/journal.json`, and the `git-tailor` dir if it ends up
  empty) and every ref git-tailor writes under `refs/git-tailor/*` — the undo
  pins (`refs/git-tailor/undo/*`) and the in-progress pin
  (`refs/git-tailor/orig`) — discovering refs by globbing `refs/git-tailor/*`
  rather than from the journal contents, so stray refs are removed even if the
  journal is missing, corrupt, or out of sync. This is a manual escape hatch for
  when recovery state gets stuck. The option must NOT start the TUI: it performs
  the cleanup and exits (like the static-output path), and is meant to run on its
  own — combining it with the normal browse arguments should be rejected with a
  clear error (or those args ignored). Write a short summary to stdout when
  finished (e.g. whether a journal file was removed and how many refs were
  deleted). Implementation: add the flag in `cli.rs`; branch early in `main.rs`
  before terminal setup; enumerate-and-delete the refs via `references_glob`
  (best-effort, continue past individual failures) and remove the journal file,
  reusing/extending the `journal` module rather than duplicating ref names. Add
  integration tests that seed a journal file plus undo/orig refs (including a
  stray `refs/git-tailor/undo/*` not referenced by the journal), run the
  cleanup, and assert the file and all refs are gone and the summary reports
  them.

## Interactivity — Commit List
- [x] T225 P3 feat - Scroll the commit list with `Ctrl-Up` / `Ctrl-Down` without
  moving the selection: bind `Ctrl-Up` / `Ctrl-Down` (currently unused —
  `Ctrl-Left`/`Right` adjust the separator and `Ctrl-PageUp`/`Down` half-page
  scroll) to scroll the list viewport by one row while keeping the selected
  commit highlighted, like vim's `Ctrl-Y` / `Ctrl-E`. Only scroll as far as the
  selection stays visible — the selected row must never leave the visible window.
  Today the scroll offset always follows the selection, so this needs an
  independent scroll offset clamped against `commit_list_visible_height` (and the
  fragmap/detail layout). Make it behave intuitively in reverse-order mode
  (`--reverse`) too, and document the keys in the help dialog.

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
- [x] T224 P3 feat - Show diff context around staged/unstaged changes in the
  commit detail view: the synthetic Staged/Unstaged rows render their diff with
  no surrounding context, while real commits show the default context, so the
  detail view is inconsistent. `reads::staged_diff` / `unstaged_diff` set
  `context_lines(0)` (needed for tight fragmap span extraction), and the detail
  view reuses that same diff. Show the same amount of context as a commit diff
  (`commit_diff`, default 3) for the detail view while keeping the 0-context
  spans for the fragmap — e.g. thread a context-lines parameter through the
  synthetic-diff reads, or add a detail-specific variant mirroring the existing
  `commit_diff` vs `commit_diff_for_fragmap` split. Relates to T166 (adjustable
  context), which should then also apply to the staged/unstaged rows.

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
