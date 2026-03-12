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

## Interactivity — Fragmap View (V5)
- [ ] T108 P1 fix - Fix fragmap relations not following file renames: when a
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
- [ ] T126 P2 feat - Add `--squashable-scope <commit|cluster>` CLI argument
  controlling what the squashable connector color/symbol means:
  `cluster` (default in TUI) — a connector in a column is squashable when *that
  cluster pair alone* has no intervening touches (current per-cluster behavior);
  `commit` (default in `--static`) — a connector is squashable only when the
  *entire lower commit* is fully squashable into the same single upper commit
  (i.e. `fragmap.is_fully_squashable()` is true and `squash_target()` points to
  that upper commit), matching the original fragmap tool's stricter rule;
  the argument must be valid in both TUI and `--static` modes; store the choice
  in `AppState` and thread it through the fragmap connector rendering logic in
  both `static_views::fragmap::render` and the TUI fragmap widget (Flags: V5)
- [X] T127 P2 fix - Respect the `-r` / `--reverse` flag when `--static` is used:
  currently `--static` always outputs commits in the order returned by
  `list_commits` (newest-first); when `--reverse` is also passed the rows should
  be printed oldest-first, matching the interactive TUI behavior (Flags: V5)
- [ ] T111 P3 feat - Replace the current example application in `examples/` with
  a compatibility test that runs both the original fragmap tool and git-tailor
  in `--static --no-color` mode on the same repository, then compares
  their outputs; the comparison must account for potentially different column
  ordering (cluster columns may appear in different sequences) while verifying
  that the same commit-cluster relationships are detected; fail with a clear
  diff if the tools disagree on which commits touch which clusters (Flags: V5)

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
- [ ] T114 P2 feat - Write comprehensive README.md documentation: describe what
  the tool does (interactive git commit browser with fragmap visualization and
  rebase operations), installation instructions, basic usage guide with key
  bindings, attribution to original fragmap tool (reference NOTICE file), note
  that the entire tool is AI-generated, and include a prominent data safety
  disclaimer warning users to push their changes before using the tool since any
  bugs may cause permanent data loss — author takes no responsibility for data
  loss under any circumstances, see Apache 2.0 license text (Flags: V5)
- [ ] T115 P2 feat - Add CHANGELOG.md following keepachangelog.com format:
  create initial changelog with sections for Unreleased, version entries (Added,
  Changed, Deprecated, Removed, Fixed, Security), and update AGENTS.md to
  instruct AI agents to ask users whether changes should be noted in the
  changelog when completing tasks that add user-visible features or fix bugs
  (Flags: V5)

## Refactoring — TUI Architecture (V5)
- [ ] T116 P3 feat - Review codebase for refactoring opportunities: audit
  existing code for duplication, overly complex functions, inconsistent
  patterns, and areas where abstractions could simplify implementation; identify
  specific refactoring targets like extracting common dialog patterns,
  consolidating similar error handling, reducing parameter passing, and
  improving module boundaries; create follow-up tasks for the most impactful
  improvements (Flags: V5)

## Notes
