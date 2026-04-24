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

## Bug Fixes — Windows Compatibility
- [X] T137 P2 bug - First commit always excluded when browsing complete history:
  when the user passes the very first (root) commit of the repository as the
  positional `base` argument, that commit is never shown in the commit list; the
  root cause is that `main.rs` always filters out the reference-point commit
  (`filter(|c| c.oid != reference_oid)`) because in the normal branch-workflow
  the merge-base is shared history that should not be editable; for
  complete-repository history this invariant does not hold and the root commit
  must be included; the fix should detect the root-commit / no-parent case (or
  add an `--all` flag) to skip the exclusion filter so that all commits from
  HEAD down to and including the first commit are shown and can be reordered,
  squashed, or split; the rebase engine's `reference_oid` concept (the "parent"
  onto which cherry-picks land) also needs to handle the case where there is no
  parent commit — likely by cherry-picking onto an empty tree for the first
  commit in the new sequence
- [X] T136 P1 bug - Error messages disappear instantly on Windows: on Windows,
  crossterm fires both a key-down and a key-release event for a single
  keystroke; error messages shown after an invalid operation (e.g. attempting a
  move or squash with unstaged changes) are dismissed immediately because the
  key-release event is treated as the user acknowledgement key press, making the
  message unreadable; filter out `KeyEventKind::Release` (and
  `KeyEventKind::Repeat` if appropriate) events in the input handling layer so
  that only `KeyEventKind::Press` events are acted upon, matching the Linux
  behavior where only press events are emitted

## Bug Fixes — Squash & Fixup
- [X] T131 P1 bug - Fixup conflict resolution incorrectly opens commit message
  editor: when a fixup operation causes a conflict in the squash tree itself and
  the user resolves it, `RebaseContinue` in `main.rs` always opens the editor
  for the commit message (via `squash_finalize`) regardless of whether the
  operation was a squash or a fixup; the `SquashContext` needs an `is_fixup`
  field (or equivalent) so that the editor is skipped and the target message is
  used as-is when finalizing a fixup, mirroring the non-conflict path in
  `PrepareSquash`
- [X] T132 P1 bug - Fixup conflict falsely reported as still unresolved: after
  the user resolves a conflict during a fixup (either manually or via mergetool)
  and presses Enter to continue, `rebase_continue` in `git2_impl.rs` re-reads
  the index with `index.read(true)` and calls `index.has_conflicts()`, which
  returns true even though the working-tree file has been correctly resolved and
  staged; investigate whether libgit2's in-memory index is not being refreshed
  from disk before the `has_conflicts()` check, or whether deleted-file
  conflicts leave behind phantom stage entries, and fix so that a genuinely
  resolved index is not incorrectly treated as unresolved
- [X] T133 P1 bug - Aborting a fixup after a conflict leaves dirty working tree:
  `rebase_abort` in `git2_impl.rs` resets the branch ref and calls
  `checkout_head()`, but this does not clean up untracked files or staged
  deletions that were left behind by the failed cherry-pick (e.g. a file that
  was deleted in the conflict appears as a staged deletion and also as an
  untracked file after the abort); the abort should additionally clean untracked
  files and reset the index so the working tree matches HEAD, similar to what
  `git checkout -f HEAD` followed by `git clean -fd` would do (fixed by T130:
  libgit2's `checkout_head(force)` already resets both the index and workdir to
  HEAD, including files absent from HEAD's tree; the dirty-workdir symptom was a
  consequence of T130's `stage_file` bug leaving the index in a corrupt state;
  integration test added to confirm)
- [X] T134 P1 bug - External editor conflict resolution not detected during
  squash/fixup: when a conflict occurs during squash or fixup and the user
  resolves it by editing the conflicted file in an external editor (e.g. VS
  Code) and saving, git-tailor does not detect the resolution; opening the
  built-in mergetool afterward still shows the original conflict markers as if
  the external edits were ignored; resolving via the built-in mergetool works
  correctly; the likely cause is that git-tailor reads the file content from
  git2's in-memory state or a cached copy rather than re-reading from the
  working tree on disk when checking conflict status or launching the mergetool

## Bug Fixes — Split
- [X] T148 P2 bug - Split commits lose the original commit message body: all
  three split strategies (per-file, per-hunk, per-hunk-group) construct the
  message for each new commit using only `commit.summary()` (the first line),
  appending a `(n/total)` counter; a commit whose message has a multi-line body
  or a detailed description will have that body silently discarded; the fix
  should use `commit.message()` instead, replacing just the first line with the
  summary + counter so the full body is retained in all split commits (or at
  least in the last one, mirroring what `git commit --amend` and `git rebase` do
  by default); all three `format!` message expressions in `git2_impl.rs` need
  updating
- [X] T147 P1 bug - Segfault when splitting a submodule-change commit per file:
  calling "split per file" on a commit that updates a submodule revision causes
  a segfault; the split-per-file path in `git2_impl.rs` iterates over the
  commit's diff entries and builds per-file patches using `Diff::apply_to_tree`,
  but a submodule change produces a delta whose old/new objects are commit OIDs
  rather than blob OIDs; attempting to treat a submodule entry as a regular blob
  (e.g. passing it to `Blob::lookup` or building a patch from it) likely
  triggers a null dereference or invalid memory access inside libgit2; the fix
  should detect submodule deltas (delta kind `GIT_DELTA_*` where the object mode
  is `GIT_FILEMODE_COMMIT`, i.e. `0o160000`) and handle them explicitly — either
  by applying the submodule pointer update as a tree-level operation instead of
  a blob diff, or by grouping all submodule deltas into a single synthesised
  commit so the split result is well-formed; add an integration test using a
  `TempDir` repo with a real submodule to reproduce the crash and verify the fix
- [X] T150 P2 bug - Splitting the root commit in `--all` mode fails with "Can
  only split a commit with exactly one parent": `split_commit_per_file`,
  `split_commit_per_hunk`, and `split_commit_per_hunk_group` in `git2_impl.rs`
  all reject commits with `parent_count != 1`; the fix should apply the same
  pattern used for `move_commit` — build the first split-piece commit as a new
  orphan root (applying its diff onto an empty tree with no parents), then
  cherry-pick the remaining split pieces and any later commits on top

## Interactivity — Conflict Resolution
- [X] T135 P2 feat - Add option to open the configured editor when resolving a
  conflict: the conflict view currently offers a key binding to launch the
  mergetool (`core.mergetool` / `merge.tool`); add a second key binding (e.g.
  `e`) that instead opens the conflicted file in the user's configured editor
  (`core.editor`, falling back to `$VISUAL`, then `$EDITOR`, then a sensible
  default such as `vi`); after the editor exits, re-check the file for conflict
  markers and update the conflict view state accordingly, the same way the
  mergetool path does

## Interactivity — Fragmap View
- [X] T108 P1 fix - Fix fragmap relations not following file renames: when a
  file is renamed across commits, spans should cluster together if they overlap
  the same logical content, but currently they are treated as separate files and
  don't form clusters. Investigate the original fragmap Python implementation
  (https://github.com/amollberg/fragmap) to see how rename detection is handled
  in span clustering, and adapt the SPG logic in `src/fragmap/spg.rs` to
  properly track renamed files so that overlapping spans across renames are
  correctly clustered together
- [X] T106 P2 feat - Refactor fragmap cell rendering into a `FragmapTheme` trait
  with four methods keyed by two enums: `SquareRole` (`Current` = the focus
  commit's own square, `Related` = another commit's square in a focus-cluster
  column, `Unrelated` = any square in a non-focus-cluster column),
  `ConnectorRole` (`Related` = the column is a focus cluster, `Unrelated` =
  otherwise), and `RelationType` (`Conflict` | `Squashable`); the trait methods
  are `square_symbol(SquareRole, RelationType) -> char`,
  `square_style(SquareRole, RelationType) -> Style`,
  `connector_symbol(ConnectorRole, RelationType) -> char`, and
  `connector_style(ConnectorRole, RelationType) -> Style`; implement
  `PlainTheme` reproducing the current uniform heavy-glyph behavior (no focus
  distinction); replace the inline constant lookups in `fragmap_cell_content`,
  `fragmap_connector_content`, and `build_fragmap_cell` with calls through the
  trait so that adding new themes (T105, T107) doesn't require scattering
  conditionals throughout the rendering functions
- [X] T105 P2 feat - Add glyph-weight focus highlighting to the fragmap matrix:
  clusters related to the focus commit (selected commit in CommitList, source
  commit in SquashSelect/MoveSelect) use heavy glyphs — `█` for touched squares
  and `┃` for connectors — while unrelated clusters use light glyphs — `▪` for
  touched squares and `┆` for connectors. Colors stay unchanged (white for
  conflicting squares, grey for squashable squares, red/yellow for connectors).
  This makes it immediately scannable which hunk groups the focus commit
  participates in without introducing new colors. "Related" means the cluster
  column contains a touch from the focus commit. Implement as a `FocusTheme`
  behind the `FragmapTheme` trait from T106.
- [X] T107 P3 feat - Add `--theme <THEME>` CLI option to select the fragmap
  rendering theme; three themes are supported: `plain` (the current uniform
  heavy-glyph rendering with no focus-related highlighting, equivalent to
  DefaultTheme from T106), `highlight` (glyph-weight focus highlighting from
  T105 where clusters related to the selected commit use heavy glyphs and
  unrelated clusters use light glyphs), and `classic` (identical rendering to
  `--static`, reproducing the traditional fragmap tool appearance); store the
  selected theme in `AppState` and select the appropriate `FragmapTheme`
  implementation at startup; `plain` should be the default

## Bug Fixes — Move Commit
- [X] T149 P2 bug - Moving a commit to the earliest position places it second
  instead of first: when using `gt --all` (or any case where the oldest visible
  commit is also the root commit), selecting a commit and choosing to move it
  before the first commit in the list results in the commit being placed
  immediately after the root commit rather than before it; the status message
  reports success; the root cause is likely that `move_commit` in `git2_impl.rs`
  resolves the "insert before first commit" target as "insert after merge-base /
  root", but for `--all` the root commit is included in the editable list which
  makes this the wrong reference point; the fix should ensure that when the
  target position is before the first commit, the entire cherry-pick chain is
  rebuilt with the root commit cherry-picked onto an empty tree first, the same
  way T137 handled the no-parent case for the initial rebase

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
- [X] T139 P3 feat - Add text search in commit detail view: add an incremental
  search mode activated by `/` (vim convention) that opens a search input bar at
  the bottom of the commit detail view; as the user types, highlight all matches
  in the visible diff content and scroll to the first match; support `n` / `N`
  to jump to next / previous match; `Escape` dismisses the search bar; the
  search should operate over the rendered diff text (file paths, hunk headers,
  and diff lines) and wrap around at the end of the content
- [ ] T143 P3 feat - Add half-page scrolling to the commit detail view: bind
  `Ctrl-D` / `Ctrl-U` (vim convention) and `Ctrl-PageDown` / `Ctrl-PageUp` to
  scroll approximately half the visible content area at a time; the scroll
  amount should be derived from the current panel height so it stays
  proportional regardless of terminal size
- [ ] T144 P3 feat - Add jump-to-top/bottom keybindings in the commit detail
  view: bind `Home` to scroll to the very first line and `End` to scroll to the
  very last line of the diff content
- [ ] T145 P3 feat - Add horizontal scroll-to-edge keybindings in the commit
  detail view: bind `Ctrl-A` / `Ctrl-E` (emacs convention) and `Ctrl-Home` /
  `Ctrl-End` to scroll the diff content fully left (column 0) or fully right
  (rightmost position) respectively
- [ ] T146 P3 feat - Make the help overlay context-sensitive: pressing `?` (or
  `h`) in the commit detail view should show only the keybindings relevant to
  that view (scrolling, search, navigation back), while pressing it in the
  commit list shows only commit-list bindings; the current single monolithic
  help window is becoming too long as new keybindings are added; implement by
  passing the current `AppMode` to the help renderer and selecting the
  appropriate subset of bindings to display

## Interactivity — Terminal Integration
- [ ] T142 P3 feat - Support Ctrl-Z to suspend the TUI and return to the shell
  (Unix only): in raw mode the kernel line discipline no longer converts Ctrl-Z
  into SIGTSTP automatically, so the keystroke arrives as a key event; handle
  `KeyCode::Char('z') + CONTROL` in the event loop by tearing down the TUI
  (disable raw mode, leave alternate screen — the same cleanup already done for
  the external editor/mergetool), then calling `libc::raise(libc::SIGTSTP)` to
  suspend the process; when the user runs `fg` the process receives SIGCONT,
  resumes after `raise` returns, and re-initialises raw mode and redraws; gate
  the entire feature on `#[cfg(unix)]` — on Windows the key event is silently
  ignored; the teardown/restore logic should be extracted into a shared helper
  to avoid duplication with editor.rs and mergetool.rs

## CLI — Shell Completion
- [ ] T140 P3 feat - Add dynamic shell completion for CLI options: use
  `clap_complete_dynamic` (or a `COMPLETE=<shell> gt ...` convention) so the
  binary itself emits completions when invoked by the shell's completion
  machinery — no separate script generation or installation step required; all
  flags and value_enum variants (e.g. `--squashable-scope`) should be covered
  automatically from clap's derived schema
- [ ] T141 P3 feat - Add dynamic branch/tag completion for the BASE argument:
  extend the completion mechanism from T140 so that the positional `base`
  argument offers branch and tag candidates; implement this via
  `clap_complete_dynamic` (or a `COMPLETE=<shell> gt ...` convention) so the
  running binary queries `git2` for local branches, remote-tracking refs, and
  tags at completion time — no pre-generated shell scripts required; the dynamic
  path should degrade gracefully if the current directory is not inside a git
  repository

## CLI Output & Compatibility

## Build & CI
- [X] T112 P3 feat - Set up cargo-deny with configuration to check dependency
  licenses are compatible with Apache 2.0: install cargo-deny, create
  `deny.toml` config allowing Apache-compatible licenses (Apache-2.0, MIT,
  BSD-2-Clause, BSD-3-Clause, ISC, etc.), deny copyleft licenses (GPL, LGPL,
  AGPL), and add `cargo deny check` command to verify no license violations in
  the dependency tree
- [X] T113 P3 feat - Add cargo-deny to GitHub Actions CI: create or update
  `.github/workflows/ci.yml` to run `cargo deny check licenses` alongside
  existing format/clippy/test checks, failing the build if any dependency
  license conflicts are detected; ensure this runs on pull requests and main
  branch pushes
- [ ] T118 P2 feat - Set up GitHub Releases with pre-built binaries: create
  `.github/workflows/release.yml` that triggers on version tags (`v*`), builds
  the `gt` binary for `x86_64-unknown-linux-musl` (fully static, covers WSL2 and
  all Linux distros), `x86_64-pc-windows-msvc` (Windows native), and optionally
  `aarch64-unknown-linux-gnu` and `aarch64-apple-darwin`; use
  `taiki-e/upload-rust-binary-action` to strip, archive, and attach binaries to
  the GitHub Release automatically; the musl target should produce a zero
  shared-library binary (add `RUSTFLAGS=-C target-feature=+crt-static` if
  needed) so no system libs beyond the kernel are required

## Refactoring — TUI Architecture
- [ ] T116 P3 feat - Review codebase for refactoring opportunities: audit
  existing code for duplication, overly complex functions, inconsistent
  patterns, and areas where abstractions could simplify implementation; identify
  specific refactoring targets like extracting common dialog patterns,
  consolidating similar error handling, reducing parameter passing, and
  improving module boundaries; create follow-up tasks for the most impactful
  improvements
- [X] T157 P3 fix - Split `src/repo/git2_impl.rs` (2238 lines) into focused
  sub-modules under `src/repo/git2_impl/`: `reads.rs` (head_oid, list_commits,
  commit_diff, staged/unstaged_diff, default_branch, root_commit_oid,
  get_config_string), `split.rs` (the three split_commit_per_* methods +
  count_split_per_* + the load_split_commit helper from T155), `squash.rs`
  (squash_try_combine, squash_finalize, squash_commits), `move_drop.rs`
  (move_commit, drop_commit, reword_commit), `conflict.rs` (rebase_continue,
  rebase_abort, collect_conflict_files, write_conflicts_to_workdir,
  auto_stage_resolved_conflicts, read_conflicting_files), and `hunks.rs` (the
  pure free-function helpers: apply_single_hunk_to_tree, apply_hunk_to_content,
  apply_multiple_hunks_to_content, apply_selected_hunks_to_tree,
  apply_gitlink_delta_to_tree, split_lines_keep_eol); the `Git2Repo` struct
  stays in `git2_impl.rs` and each sub-module adds its `impl Git2Repo` /
  `impl GitRepo for Git2Repo` block; preserves the existing public API
- [ ] T158 P3 fix - Move the inline `#[cfg(test)] mod tests` block (~1600 lines)
  out of `src/fragmap.rs` (2387 lines) into a separate `src/fragmap/tests.rs`
  file gated by `#[cfg(test)] mod tests;` in `fragmap.rs`; production code drops
  to ~700 lines and the file becomes navigable; no behavioural change
- [X] T159 P2 fix - Extract `AppState::reload_preserving_selection(&impl
  GitRepo)` to replace the five-times-repeated pattern `let saved_index =
  app.selection_index; reload_commits(&git_repo, &mut app); app.selection_index
  = saved_index.min(app.commits.len().saturating_sub(1));` in `main.rs` (drop,
  move, squash, squash_finalize, rebase_continue); each call site becomes a
  single line
- [X] T160 P2 fix - Extract a `handle_rebase_outcome` helper (free fn or
  AppState method) in `main.rs` to consolidate the repeated `match outcome
  { Ok(RebaseOutcome::Complete) => { reload + preserve_selection +
  set_success_message }, Ok(RebaseOutcome::Conflict(state)) =>
  app.enter_rebase_conflict(*state), Err(e) => app.set_error_message(format!
  ("{label} failed: {e}")) }` block; called by ExecuteDrop, ExecuteMove,
  PrepareSquash, RebaseContinue, and the squash finalize path; reduces ~80
  lines of boilerplate
- [X] T161 P3 fix - Extract a `run_external_tool<T>(terminal, kb_enhanced, f)`
  helper in `main.rs` that wraps the `with_external_process(kb_enhanced, f) +
  terminal.clear()?` pattern; used by editor invocations (PrepareReword,
  squash-message editor, conflict-finalize editor) and the mergetool/editor
  conflict-resolution paths; the four current call sites collapse from 4 lines
  each to 1
- [X] T162 P2 fix - Decompose `main.rs::main()` (~530 lines) into focused
  helpers: `load_initial_commits(&git_repo, &cli)` returning `(Vec<CommitInfo>,
  String, bool)` (extracts lines 222–254), `setup_terminal()` returning a RAII
  `TerminalGuard` that owns raw mode + alternate screen + keyboard enhancement
  and restores them on Drop (extracts lines 285–303 and 743–748),
  `init_app_state(commits, &cli, &git_repo)` returning the configured `AppState`
  with synthetic rows (extracts lines 305–320), and split the giant `AppAction`
  dispatch `match` into `dispatch_action(action, &mut app, &git_repo, terminal,
  kb_enhanced) -> Result<()>` so `main` reads as a clear setup → loop → teardown
  flow under 50 lines
- [ ] T163 P3 fix - Decompose `views/commit_detail.rs::render` (~290 lines) into
  focused helpers: `build_metadata_lines(commit) -> Vec<Line>` (oid, message,
  author, dates), `build_file_list_lines(diff) -> Vec<Line>` (the "Changed
  Files:" section with status indicators), `build_diff_lines(diff) -> Vec<Line>`
  (file headers + hunk headers + colored +/- lines), and
  `compute_scroll_layout(content_area, content) -> ScrollLayout` (returns
  text_area, scrollbar areas, max_scroll, max_h_scroll); render becomes a
  composition of these helpers + the search-highlight pass + widget calls
- [ ] T164 P3 fix - Decompose `views/commit_list.rs::build_rows` (~190 lines)
  by extracting `fn row_text_style(app, focus_ctx: FocusContext, commit_idx,
  is_selected, is_synthetic) -> Style` to replace the 60-line nested
  if/else-if chain that picks the foreground style based on
  squash/move/normal mode; introduce a small `FocusContext` enum (`Squash {
  source_idx }`, `Move { source_idx }`, `Normal`) to make the dispatch
  explicit; also add `AppState::fragmap_index(visual_idx) -> usize` to remove
  the three repeated `if app.reverse { len-1-idx } else { idx }`
  expressions in build_rows
- [X] T151 P3 fix - Eliminate duplication between `AppState::new()` and
  `AppState::with_commits()`: both functions repeat the same ~30 field
  initializations verbatim; implement `Default` for `AppState` containing all
  the zero-values, then have `with_commits` construct via `AppState { commits,
  selection_index, ..Default::default() }` and `new` delegate to `Default`;
  remove the duplicate field lists entirely
- [X] T152 P3 fix - Extract repeated `head_oid` fetch pattern in `main.rs`: the
  block `match git_repo.head_oid() { Ok(oid) => oid, Err(e) => {
  app.set_error_message(...); continue; } }` appears five times in the
  `AppAction` dispatch arms (PrepareSplit, PrepareDropConfirm, PrepareReword,
  PrepareSquash, ExecuteMove); extract a local macro or inline helper
  `get_head_oid!(git_repo, app)` that encapsulates the error path so each call
  site is a single expression
- [ ] T153 P3 fix - Add `CommitInfo::is_synthetic()` helper to replace scattered
  inline checks: the expression `commit.oid == "staged" || commit.oid ==
  "unstaged"` is repeated in five or more places across `app.rs` and
  `commit_list.rs`; add a `pub fn is_synthetic(&self) -> bool` method to
  `CommitInfo` in `lib.rs` and replace every inline occurrence with a call to it
- [ ] T154 P3 fix - Deduplicate `short_oid` truncation logic: the snippet `if
  oid.len() >= 10 { &oid[..10] } else { &oid }` (or near-identical variants)
  appears independently in `conflict.rs`, `drop.rs`, `split_select.rs`, and
  `commit_list.rs`; add a free function `short_oid(oid: &str) -> &str` in a
  shared location (e.g. `views/dialog.rs` or a new `views/utils.rs`) and
  replace all call sites
- [X] T155 P3 fix - Extract common split-commit preamble into a shared helper:
  `split_commit_per_file`, `split_commit_per_hunk`, and
  `split_commit_per_hunk_group` in `git2_impl.rs` each begin with ~20 identical
  lines (parse OID, find commit, bail on merge commit, compute `parent_tree`
  handling the root-commit case, get `commit_tree`); extract a private helper
  `fn load_split_commit(repo, oid) -> Result<SplitCommitParts>` returning the
  shared values, and apply the same extraction to the three `count_split_*`
  methods which duplicate the same setup
- [ ] T156 P3 fix - Remove redundant `visible_clusters` double-iteration in
  `compute_layout`: `commit_list.rs::compute_layout` iterates the fragmap matrix
  twice with identical predicate logic — once to compute `visible_cluster_count`
  for the scrollbar decision, then again to build
  `visible_clusters: Vec<usize>`; compute the `Vec` first and derive the count
  from `visible_clusters.len()` to eliminate the duplicate pass

## Notes
