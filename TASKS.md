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
