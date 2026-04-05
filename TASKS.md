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
  `PlainTheme` reproducing the current uniform heavy-glyph behavior (no
  focus distinction); replace the inline constant lookups in
  `fragmap_cell_content`, `fragmap_connector_content`, and
  `build_fragmap_cell` with calls through the trait so that adding new themes
  (T105, T107) doesn't require scattering conditionals throughout the rendering
  functions
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
  tags at completion time — no pre-generated shell scripts required; the
  dynamic path should degrade gracefully if the current directory is not inside
  a git repository

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

## Notes
