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

## Interactivity — Commit List & Operations
- [ ] T190 P2 feat - Support dropping the root commit: currently `drop_commit`
  bails with "Cannot drop a merge or root commit" when `commit.parent_count()`
  `== 0`; update `drop_op.rs` to handle the root case separately — collect all
  descendants, make the first descendant an orphan root commit (using its
  existing tree and metadata, reusing the `plan_move_root_to_later` pattern from
  `move_op.rs`), then cherry-pick the rest of the chain on top; split the
  parent-count guard into two branches: `parent_count > 1` bails with "Cannot
  drop a merge commit", `parent_count == 0` takes the root path, `parent_count
  == 1` is the existing fast path; also update `validate_single_parent_op` (or
  introduce a separate `validate_non_merge_op`) if the refactored helper from
  T178 makes the split guard awkward; add a test in
  `tests/drop_commit/root_commit.rs` that verifies the root commit is dropped
  and the history is correctly rewritten.

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
- [ ] T209 P2 feat - Add `Space` / `b` (less convention) and `Ctrl-F` / `Ctrl-B`
  (vi convention) page-scroll keybindings in the commit detail view: `Space` and
  `Ctrl-F` scroll one page down, `b` and `Ctrl-B` scroll one page up; the scroll
  amount should match the existing `PageDown`/`PageUp` behaviour (one
  visible-area height, keeping one line of overlap)
- [ ] T143 P3 feat - Add half-page scrolling to the commit detail view: bind
  `Ctrl-D` / `Ctrl-U` (vim convention) and `Ctrl-PageDown` / `Ctrl-PageUp` to
  scroll approximately half the visible content area at a time; the scroll
  amount should be derived from the current panel height so it stays
  proportional regardless of terminal size
- [ ] T144 P3 feat - Add jump-to-top/bottom keybindings in the commit detail
  view: bind `g` / `G` (less/vi convention) and `Home` / `End` to scroll to the
  very first or very last line of the diff content
- [ ] T145 P3 feat - Add horizontal scroll-to-edge keybindings in the commit
  detail view: bind `0` / `$` (vi/less convention), `Ctrl-A` / `Ctrl-E` (emacs
  convention), and `Ctrl-Home` / `Ctrl-End` to scroll the diff content fully
  left (column 0) or fully right (rightmost position) respectively
- [ ] T166 P3 feat - Increase and decrease diff context lines in commit detail
  view with `+` and `-`: pressing `+` should increase the number of context
  lines shown around each hunk (default 3, matching git's default), and `-`
  should decrease it (minimum 0); store the context line count in `AppState` and
  pass it through to `commit_diff` (or re-render the cached diff with the new
  context); changing the value should trigger a re-fetch or re-render of the
  diff so the change is immediately visible; show the current context line count
  in the footer or status line so the user knows the active value
- [ ] T165 P3 feat - Navigate between files in commit detail view by pressing
  `f`: pressing `f` should jump the scroll position to the start of the next
  file's diff block in the commit detail view; pressing `F` (shift) should jump
  to the previous file; the file boundary can be detected from the rendered line
  list (each `FileDiff` entry starts with a file header line); wrap around when
  reaching the end/beginning of the file list so the navigation is cyclic
- [ ] T146 P3 feat - Make the help overlay context-sensitive: pressing `?` (or
  `h`) in the commit detail view should show only the keybindings relevant to
  that view (scrolling, search, navigation back), while pressing it in the
  commit list shows only commit-list bindings; the current single monolithic
  help window is becoming too long as new keybindings are added; implement by
  passing the current `AppMode` to the help renderer and selecting the
  appropriate subset of bindings to display

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

## Startup & Performance
- [ ] T211 P2 feat - Start the TUI immediately and load commits in a background
  thread, showing a loading screen while work is in progress: add a
  `Loading { count: usize }` variant to `AppMode`; in `main.rs`, initialise the
  terminal and enter the event loop before fetching any commits; spawn a
  background thread that opens its own `Git2Repo` handle (since
  `git2::Repository` is not `Send`) and sends commits one-by-one via
  `std::sync::mpsc`; the event loop polls the channel each tick and increments
  the displayed counter until all commits arrive, then transitions to
  `CommitList`; add a `views::loading` module that renders the counter and a
  brief status message; Ctrl-C exits cleanly via the normal event loop quit
  path, so no separate signal handler is needed

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
