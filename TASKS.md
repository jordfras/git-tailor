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
- [X] T215 P1 bug - Fix spurious conflict when squashing across a rename: when
  squashing commit B into an earlier commit A where a file touched by both was
  renamed in a commit between them, the tool incorrectly reports a conflict and
  leaves both the old and new filename to resolve — even though `git rebase
  -i` completes cleanly; investigate how `squash_op.rs` builds the cherry-pick
  chain across renames (the intermediate rename commit changes the path, so the
  cherry-pick of A's diff onto the post-rename tree likely applies to the wrong
  path); compare with how `move_op.rs` handles rename tracking; the fix should
  make the squash cherry-pick chain path-aware — either by detecting the rename
  and rewriting the diff path before applying, or by using the post-rename path
  consistently throughout the chain; add a regression test in
  `tests/squash_commit/` with a rename between the squash source and target.
- [X] T214 P2 feat - Allow squash/fixup into the root commit: currently
  `squash_commits` (and fixup) bail when the target commit has no parent because
  the cherry-pick chain requires a base tree; handle the root case by squashing
  the source commit's diff directly onto the root's tree, then creating a new
  root commit (no parents) with the combined tree and message; the source commit
  should then be removed from the chain using the existing rebase logic; add
  tests in `tests/squash_commit/` covering squash-into-root and fixup-into-root.
- [X] T190 P2 feat - Support dropping the root commit: currently `drop_commit`
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
- [X] T217 P1 bug - Fix wrong highlight row in hunk group matrix during move
  commit (`m`): when the move-select dialog is open, the highlighted row in the
  fragmap / hunk group matrix is always two rows below the empty placeholder
  line that marks the insertion point; investigate how `move_select.rs` (or
  `main_view.rs`) computes the highlighted matrix row from `insert_before` and
  trace back to where the off-by-two offset originates; fix the index
  calculation so the highlighted row tracks the insertion-point placeholder
  exactly; add or update the `tui_move_select` snapshot tests to cover the
  highlighted-row position.

## Architecture & Robustness
- [ ] T216 P2 refactor - Replace manual cherry-pick chain engine with
  `git2::Rebase` for move, drop, squash, and reword operations: the current
  `cherry_pick_chain` / `commit_and_replay` helpers in `cherry_pick.rs` and the
  per-operation files (`move_op.rs`, `drop_op.rs`, `squash_op.rs`,
  `reword_op.rs`) re-implement what `git2::Rebase` (libgit2's rebase engine)
  already provides — advantages of switching: (1) **crash/kill recovery** —
  libgit2 writes rebase state to `.git/rebase-merge/` so if git-tailor is
  killed mid-operation the user can run `git rebase --abort` to return to a
  clean state, whereas the current approach leaves the index in an unknown
  state requiring `git reset --hard`; (2) **free `--continue` / `--abort`** —
  conflict resolution could delegate to the existing libgit2 rebase state
  machine instead of the custom `ConflictState` serialisation; non-adjacent
  squash is expressible as reorder-then-squash (two steps in the todo list),
  matching what a user would write in `git rebase -i`; note: rename detection
  (T215) is a libgit2 issue that affects both our current cherry-pick calls and
  `git2::Rebase` equally — it is not fixed by this refactor; split operations
  still require custom tree surgery (`apply_to_tree` per file / per hunk) but
  the *replay* of subsequent commits can use the rebase engine; the refactor
  should be done operation by operation behind the `GitRepo` trait so tests
  stay green throughout; keep the `GitRepo` trait interface unchanged so the
  TUI and tests are unaffected.

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
- [X] T209 P2 feat - Add `Space` / `b` (less convention) and `Ctrl-F` / `Ctrl-B`
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
- [X] T146 P3 feat - Make the help overlay context-sensitive: pressing `?` (or
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
- [x] T213 P2 fix - Replace `FragMapBuilder` step loop with a single-callback
  `build_fragmap()` to fix unresponsiveness when one file's SPG takes too long:
  remove `FragMapBuilder` and its `step()` / `run_dedup()` / `finish_matrix()`
  methods; add a `FragMapProgress` enum with variants `ClusteringFile {
  files_done: usize, files_total: usize }`, `Deduplicating`, and
  `BuildingMatrix`; change `build_fragmap` signature to
  `build_fragmap(commit_diffs: &[CommitDiff], deduplicate: bool, progress: &mut
  impl FnMut(FragMapProgress) -> bool) -> Option<FragMap>` where the callback
  returns `true` to continue and `false` to interrupt (returning `None` from
  `build_fragmap`); thread the callback down through `build_file_clusters` →
  `build_file_clusters_and_assign_hunks` → `build_file_spg` (in `spg.rs`),
  calling it after each commit generation is processed inside `build_file_spg`'s
  main loop to ensure responsiveness even for a single large file; also call it
  at the outer file-loop boundary (updating `files_done`), before deduplication,
  and before matrix construction; update `build_hunk_group_matrix` in
  `loader.rs` to call `build_fragmap` with a closure that renders the loading
  view, polls crossterm for `s`/`S` (skip), and updates `app.mode` with the
  appropriate `AppMode::Loading` variant for each phase — the closure captures
  `terminal_guard` and `app` by mutable reference; `build_hunk_group_matrix`
  stays `Result<Option<FragMap>>` (the `Result` wraps terminal I/O errors from
  rendering); `build_fragmap` itself stays `Option<FragMap>` with no `Result`
  since it has no I/O; update `assign_hunk_groups` (used by split) to keep its
  current internal structure but accept an optional no-op progress callback if
  needed for consistency; add or update any tests that directly used
  `FragMapBuilder`.
- [x] T211 P2 feat - Start the TUI immediately and stream commits one-by-one
  with a live counter dialog: added `commit_walker` to `GitRepo` returning a
  boxed iterator so `Git2Repo` yields one commit at a time from the underlying
  `git2::Revwalk`; added `AppMode::Loading { title, message, count }` rendered
  by a new `views::loading` module as a centred dialog overlay; the loading loop
  in the new `src/loader.rs` module renders at ~60 fps and polls for Ctrl-C with
  `crossterm::event::poll(Duration::ZERO)` between commits — no background
  thread needed; split `load_with_progress` into three private helpers:
  `walk_commits` (iterator loop), `confirm_matrix_build` (Y/N dialog for large
  repos), `build_hunk_group_matrix` (fragmap computation with progress title);
  loading dialog shows `"Loading Commits"` title during the walk and
  `"Hunk Group Matrix"` during matrix computation; dialog border colour changed
  from `DarkGray` to `Cyan` to match other info dialogs; Y/N matrix confirm
  labels changed from `Compute`/`Skip` to `Yes`/`No`.

## UI — Theming & Dialogs
- [x] T212 P3 feat - Introduce semantic dialog kinds and text roles to eliminate
  scattered `Color` literals from dialog call sites: add a `DialogKind` enum
  (`Info`, `Confirm`, `Danger`) whose variants map to a fixed border color
  (`Cyan`, `Yellow`, `Red` respectively — matching the existing conventions);
  change `Dialog::render` to accept `DialogKind` instead of a raw `Color` for
  the border; add a `TextRole` enum (`Normal`, `Highlight`, `Muted`, `Key`,
  `Danger`) and corresponding `Dialog` builder methods (`role_line`,
  `role_wrapped`, etc.) that resolve the role to a `Color` internally; update
  all call sites in `views/` (`drop.rs`, `conflict.rs`, `split_select.rs`,
  `help.rs`, `loading.rs`, `squash_select.rs`, `move_select.rs`) to use the
  new API; the `theme.rs` module (or a new `dialog_theme.rs` sibling) owns the
  `DialogKind → Color` and `TextRole → Color` mappings so a future theme
  switch only needs to touch one place.

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
