# Changelog

All notable changes to this project will be documented in this file.

The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).


## [Unreleased]

### Added

- Ctrl-Z suspends the TUI and returns to the shell on Unix (SIGTSTP). Pressing
  `fg` in the shell resumes the TUI exactly where it was left.
- The footer now shows a right-aligned `Press 'h' for help` hint so new users
  can discover key bindings without prior knowledge. The hint is shown only when
  there is enough space and is suppressed whenever a status or error message
  occupies the footer.

### Internals

- Extensive internal refactoring of the TUI rendering layer: extracted shared
  layout helpers, decomposed large rendering functions into smaller focused
  units, and consolidated duplicated style logic. No intentional behaviour
  changes, but these structural changes touch most view modules and may
  introduce subtle regressions.


## [0.4.0] - 2026-04-22

### Added

- Theming support for the hunk group matrix via `--theme`. Three built-in
  themes: `plain` (default), `highlight` (dims unrelated columns to emphasize
  the selected commit's clusters), and `classic` (background-color style
  matching `--static` output).
- Environment variable defaults for CLI flags: `GT_THEME`, `GT_REVERSE`,
  `GT_FULL`, and `GT_SQUASHABLE_SCOPE`. Set these in your shell profile to avoid
  repeating flags on every invocation. CLI flags take precedence.

### Fixed

- "Split per file" no longer crashes (segfault) when the commit being split
  contains a submodule pointer update.
- All three split strategies (per-file, per-hunk, per-hunk-group) now preserve
  the full commit message body in every split commit. Previously only the first
  line (summary) was kept; the body was silently discarded.
- Editor commands with spaces in the path (e.g. `'C:/Program Files/…/emacs.exe'
  --no-splash`) are now parsed correctly as shell words. Previously, naive
  whitespace splitting broke the executable path, silently preventing the editor
  from launching on Windows.
- Merge tool commands are now launched directly without a shell, with
  `$LOCAL`/`$BASE`/`$REMOTE`/`$MERGED` substituted in Rust. This fixes launch
  failures on Windows (no `sh`) and correctly handles tool paths that contain
  spaces.
- After returning from an external editor or merge tool, Enter, Esc, and arrow
  keys no longer stop working. The TUI suspend/restore now correctly re-pushes
  keyboard-enhancement flags and uses the right terminal handle on all
  platforms.
- Moving a commit to the first position in `--all` mode now correctly makes it
  the new root commit. Previously the commit landed second because the original
  root was used as the fixed rebase anchor.
- All three split strategies (per-file, per-hunk, per-hunk-group) now work on
  the root commit in `--all` mode. Previously they failed with "Can only split a
  commit with exactly one parent".


## [0.3.0] - 2026-04-03

### Added

- Press `/` in the commit detail view to search the diff with a regex pattern.
  `n` / `N` jump to the next / previous match, `Esc` dismisses the search.
- `--all` flag: pass `gt --all` to browse and edit the complete repository
  history from HEAD down to the first (root) commit. Mutually exclusive with the
  positional `BASE` argument. Useful for projects with no upstream branch or
  single-branch workflows.

### Fixed

- Error messages (e.g. "Cannot move staged/unstaged changes") no longer
  disappear instantly on Windows. Crossterm emits both a key-press and a
  key-release event per keystroke on Windows; the release event was clearing the
  message before it could be read. Release events are now discarded.
- Squash/fixup with a conflict no longer corrupts the resulting tree. Stale
  index entries from HEAD leaked into the squash commit, causing spurious
  conflicts (with empty merge bases) in later commits during the rebase.


## [0.2.0] - 2026-03-21

### Added

- `<BASE>` argument is now optional. When omitted, `gt` resolves `origin/HEAD`
  to determine the repository's default upstream branch (e.g. `origin/main`).
  Falls back to `main` if `origin/HEAD` is not configured.
- Press `e` in the conflict dialog to open each conflicting file directly in the
  configured editor (`GIT_EDITOR` → `core.editor` → `$VISUAL` → `$EDITOR` →
  `vi`). After the editor exits the conflict state is refreshed, the same way
  the mergetool (`m`) path works.

### Fixed

- Fixup operations that hit a squash-tree conflict no longer open the commit
  message editor after the conflict is resolved — the target commit's message is
  used as-is, matching the behavior of a conflict-free fixup.
- Resolving a conflict that involves a deleted or renamed file (modify/delete
  conflict) is no longer falsely reported as still unresolved: `stage_file` now
  correctly stages the deletion when the file is absent from the working tree
  instead of failing with "file not found".
- Aborting a squash or fixup after a conflict now correctly leaves a clean
  working tree: `checkout_head` (force) already resets both the index and
  workdir to HEAD, including removing any files written during conflict checkout
  that are absent from HEAD's tree.
- Conflicts resolved in an external editor (e.g. VS Code) are now detected when
  pressing Enter to continue: the app auto-stages working-tree files whose
  conflict markers have been removed, so the index reflects the actual
  resolution state. Previously only the built-in mergetool path worked.
- Hunk group matrix now tracks file renames across commits: when a file is
  renamed, overlapping spans in the old and new paths are correctly clustered
  together instead of being treated as unrelated files.
- Added possibility to perform drop, move and squash operations when there are
  unstaged/staged changes in a submodule.


## [0.1.0] - 2026-03-15

### Added

- Interactive TUI commit browser showing all commits between HEAD and the
  merge-base with a configured base branch (e.g. `main`).
- Hunk group matrix panel: a fragmap-style visualization showing which commits
  touch the same lines of code, with white/grey squares and colored connectors
  indicating conflicts and squashability.
- Commit detail view (toggle with `Enter`/`i`) showing the full diff for the
  selected commit with syntax-highlighted output.
- **Squash** (`s`) — merge the selected commit into an earlier one, with an
  editable combined commit message.
- **Fixup** (`f`) — like squash but discards the selected commit's message.
- **Move** (`m`) — reorder the selected commit to a new position in the history.
- **Split** (`p`) — divide the selected commit into smaller commits, per file or
  per hunk.
- **Reword** (`r`) — edit the commit message of any commit in the range.
- **Drop** (`d`) — delete the selected commit entirely.
- Conflict and squashability highlighting in the commit list: selected commit's
  partners are colored yellow (squashable), red (conflicting), or grey (fully
  squashable).
- Adjustable panel separator (`Ctrl ←`/`Ctrl →`) between the commit list and the
  hunk group matrix.
- Scrollable hunk group matrix with left/right navigation (`←`/`→`).
- `--reverse` / `-r` flag to show oldest commits at the top.
- `--full` / `-f` flag to show every raw hunk group column without
  deduplication.
- `--static` / `-s` flag to print the hunk group matrix to stdout and exit
  without launching the TUI; title column width adapts to terminal width.
- `--no-color` flag to disable colors in `--static` output.
- `--squashable-scope <commit|group>` flag controlling whether yellow connectors
  indicate per-hunk-group or per-commit squashability.
- Help dialog (`h`) listing all key bindings.
- Drop, move, squash, and fixup refuse to run when staged or unstaged changes
  are present, preventing accidental data loss.
