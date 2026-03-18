# TASKS Checklist

Guidelines:
- Each task line: `- [ ] T### P? category - Title (Flags: ...)`
- Priorities: P0 (urgent) → P3 (low).
- Categories: bug | feat | fix | idea | human.
- Flags (optional): CLARIFICATION, HUMAN INPUT, HUMAN TASK, DUPLICATE.
- Version flags (optional): V1, V2 etc. (used to group versions/releases).
- Mark completion by [ ] → [X]. Keep changes atomic (one commit per task).
- Mark won't-do tasks by [ ] → [-] and add `WONT DO` to Flags.
- Completed tasks are archived in TASKS-COMPLETED.md.


## UNCATEGORIZED

## Bug Fixes — Squash & Fixup
- [X] T131 P1 bug - Fixup conflict resolution incorrectly opens commit message
  editor: when a fixup operation causes a conflict in the squash tree itself and
  the user resolves it, `RebaseContinue` in `main.rs` always opens the editor
  for the commit message (via `squash_finalize`) regardless of whether the
  operation was a squash or a fixup; the `SquashContext` needs an `is_fixup`
  field (or equivalent) so that the editor is skipped and the target message is
  used as-is when finalizing a fixup, mirroring the non-conflict path in
  `PrepareSquash`
- [ ] T132 P1 bug - Fixup conflict falsely reported as still unresolved: after
  the user resolves a conflict during a fixup (either manually or via mergetool)
  and presses Enter to continue, `rebase_continue` in `git2_impl.rs` re-reads
  the index with `index.read(true)` and calls `index.has_conflicts()`, which
  returns true even though the working-tree file has been correctly resolved and
  staged; investigate whether libgit2's in-memory index is not being refreshed
  from disk before the `has_conflicts()` check, or whether deleted-file
  conflicts leave behind phantom stage entries, and fix so that a genuinely
  resolved index is not incorrectly treated as unresolved
- [ ] T133 P1 bug - Aborting a fixup after a conflict leaves dirty working tree:
  `rebase_abort` in `git2_impl.rs` resets the branch ref and calls
  `checkout_head()`, but this does not clean up untracked files or staged
  deletions that were left behind by the failed cherry-pick (e.g. a file that
  was deleted in the conflict appears as a staged deletion and also as an
  untracked file after the abort); the abort should additionally clean untracked
  files and reset the index so the working tree matches HEAD, similar to what
  `git checkout -f HEAD` followed by `git clean -fd` would do

## Interactivity — Fragmap View (V5)
- [X] T108 P1 fix - Fix fragmap relations not following file renames: when a
  file is renamed across commits, spans should cluster together if they overlap
  the same logical content, but currently they are treated as separate files and
  don't form clusters. Investigate the original fragmap Python implementation
  (https://github.com/amollberg/fragmap) to see how rename detection is handled
  in span clustering, and adapt the SPG logic in `src/fragmap/spg.rs` to
  properly track renamed files so that overlapping spans across renames are
  correctly clustered together (Flags: V5)
- [ ] T106 P2 feat - Refactor fragmap cell rendering into a `FragmapTheme` trait
  with methods like `touched_symbol()`, `connector_symbol()`, `touched_style()`,
  `connector_style()` that accept context (relation type, whether the cluster is
  focus-related, whether the row is selected) and return the glyph and `Style`;
  implement `DefaultTheme` reproducing the current behavior; replace the inline
  constant lookups in `fragmap_cell_content`, `fragmap_connector_content`, and
  `build_fragmap_cell` with calls through the trait so that adding new rendering
  modes (T105) doesn't require scattering conditionals throughout the rendering
  functions (Flags: V5)
- [ ] T105 P2 feat - Add glyph-weight focus highlighting to the fragmap matrix:
  clusters related to the focus commit (selected commit in CommitList, source
  commit in SquashSelect/MoveSelect) use heavy glyphs — `█` for touched squares
  and `┃` for connectors — while unrelated clusters use light glyphs — `▪` for
  touched squares and `┆` for connectors. Colors stay unchanged (white for
  conflicting squares, grey for squashable squares, red/yellow for connectors).
  This makes it immediately scannable which hunk groups the focus commit
  participates in without introducing new colors. "Related" means the cluster
  column contains a touch from the focus commit. Implement as a `FocusTheme`
  behind the `FragmapTheme` trait from T106. (Flags: V5)
- [ ] T107 P3 feat - Add CLI flag `--no-focus-glyphs` (or similar) to disable
  the glyph-weight focus highlighting from T105 and fall back to the uniform
  heavy-glyph rendering (DefaultTheme from T106); store the choice in `AppState`
  and select the appropriate `FragmapTheme` implementation at startup (Flags:
  V5)

## CLI Output & Compatibility (V5)

## Build & CI (V5)
- [ ] T112 P3 feat - Set up cargo-deny with configuration to check dependency
  licenses are compatible with Apache 2.0: install cargo-deny, create
  `deny.toml` config allowing Apache-compatible licenses (Apache-2.0, MIT,
  BSD-2-Clause, BSD-3-Clause, ISC, etc.), deny copyleft licenses (GPL, LGPL,
  AGPL), and add `cargo deny check` command to verify no license violations in
  the dependency tree (Flags: V5)
- [ ] T113 P3 feat - Add cargo-deny to GitHub Actions CI: create or update
  `.github/workflows/ci.yml` to run `cargo deny check licenses` alongside
  existing format/clippy/test checks, failing the build if any dependency
  license conflicts are detected; ensure this runs on pull requests and main
  branch pushes (Flags: V5)
- [ ] T118 P2 feat - Set up GitHub Releases with pre-built binaries: create
  `.github/workflows/release.yml` that triggers on version tags (`v*`), builds
  the `gt` binary for `x86_64-unknown-linux-musl` (fully static, covers WSL2 and
  all Linux distros), `x86_64-pc-windows-msvc` (Windows native), and optionally
  `aarch64-unknown-linux-gnu` and `aarch64-apple-darwin`; use
  `taiki-e/upload-rust-binary-action` to strip, archive, and attach binaries to
  the GitHub Release automatically; the musl target should produce a zero
  shared-library binary (add `RUSTFLAGS=-C target-feature=+crt-static` if
  needed) so no system libs beyond the kernel are required (Flags: V5)

## Refactoring — TUI Architecture (V5)
- [ ] T116 P3 feat - Review codebase for refactoring opportunities: audit
  existing code for duplication, overly complex functions, inconsistent
  patterns, and areas where abstractions could simplify implementation; identify
  specific refactoring targets like extracting common dialog patterns,
  consolidating similar error handling, reducing parameter passing, and
  improving module boundaries; create follow-up tasks for the most impactful
  improvements (Flags: V5)

## Notes
