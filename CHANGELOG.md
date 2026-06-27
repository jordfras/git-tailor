# Changelog

All notable changes to this project will be documented in this file.

The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).


## [Unreleased]

### Added

- Stage and unstage all working-tree changes without leaving git-tailor. With the
  Unstaged row selected, press `a` to stage every change (modifications, new
  files, and deletions, like `git add -A`); with the Staged row selected, press
  `A` to unstage everything back to HEAD. Both are undoable and redoable with
  `u` / `Ctrl-r`.
- Undo and redo of history-rewriting operations. Press `u` to undo the last
  drop, move, squash, fixup, reword, or split, and `Ctrl-r` to redo it; multiple
  levels are supported and the stack persists across restarts. Undo restores the
  branch to the exact state before the operation — no per-operation inverse is
  needed, since the old commits are kept and pinned against `git gc`. It refuses
  when the working tree has uncommitted changes, and discards the stack if the
  branch was changed outside git-tailor.
- Crash safety for rebase operations. While a drop, move, squash, or fixup is
  paused on an unresolved conflict, git-tailor now records the operation to a
  journal under `.git/git-tailor/`. If the tool is killed (or the terminal
  closed) before the conflict is resolved, the next launch detects the
  interrupted operation and offers to resume it or abort back to the original
  branch tip — which is pinned with a ref so its commits survive `git gc`.
  Previously this state lived only in memory and was lost on exit, leaving the
  repository stuck mid-conflict.
- Opt-in auto-stash via the `--autostash` flag (or `GT_AUTOSTASH` env var). When
  enabled, operations that need a clean working tree — move, drop, squash,
  fixup, undo, and redo — automatically stash your staged, unstaged, and
  untracked changes, run the operation, and restore the exact staged/unstaged
  split afterwards, instead of refusing to run. The stash is recorded in the
  operation journal so it survives a crash or a conflict pause. If reapplying the
  changes conflicts with the result, the same resolution dialog used for
  cherry-pick conflicts opens — resolve with the merge tool (`m`) or editor
  (`e`) and continue, or press Esc to abort the whole operation and get your
  changes back unchanged. Disabled by default, matching git's own
  `rebase.autoStash` behaviour.

### Changed

- Refreshing the commit list moved from `u` to `R` / `F5`, freeing `u` for undo
  (the vim convention). "Refresh" replaces the previous "update" wording.


## [1.0.0] - 2026-06-12

### Fixed

- Overlay dialogs now size themselves and scroll by wrapped rows rather than
  logical lines. Previously a line wider than the dialog wrapped onto extra
  rows that were not counted, so the dialog rendered one or more rows too short
  — clipping content against the bottom border and leaving it unreachable by
  scrolling (most visible in narrow terminals).
- Rewording a commit without changing the message now shows a "No changes made"
  status message instead of silently doing nothing. Saving an empty message
  cancels the reword with a status message instead of committing an invalid
  commit message.
- The hunk group matrix no longer highlights the wrong row when the move-commit
  picker (`m`) is open. Previously the fragmap selection highlight appeared two
  rows below the "▶ move here" separator instead of on the separator row itself.

### Added

- git-tailor now checks crates.io for a newer release (at most once a day, on a
  background thread so startup is never delayed) and shows an unobtrusive notice
  in the footer when one is found.
- The split menu (`p`) has a new "Split out file" option for commits that touch
  multiple files. It opens a file picker; choosing a file rewrites the commit
  into two: the first keeps every other file's changes under the original
  message, and the second contains only the chosen file's changes (its summary
  suffixed with the file name).
- In the commit detail view, pressing `Enter` to confirm a search now
  automatically scrolls to the first match at or after the current scroll
  position (wrapping to the first match overall if all matches lie above).
- `Space` / `Ctrl-f` and `b` / `Ctrl-b` page-scroll keybindings (less/vi
  convention) now work in both the commit list and the commit detail view,
  mapping to the same page-down / page-up behaviour as `PageDown` / `PageUp`.
- `Ctrl-d` / `Ctrl-u` and `Ctrl-PageDown` / `Ctrl-PageUp` half-page-scroll
  keybindings (vim convention) work in both the commit list and the commit
  detail view; the scroll amount is half the current panel height.
- `g` / `G` and `Home` / `End` jump to the first/last entry in both the commit
  list and the commit detail view.
- `0` / `$`, `Ctrl-a` / `Ctrl-e`, and `Ctrl-Home` / `Ctrl-End` scroll the
  hunk group matrix or commit detail view fully left or right in both the commit
  list and the commit detail view.
- `f` / `F` in the commit detail view jump to the next / previous file's diff
  block, wrapping cyclically.
- The help overlay (`h`) is now context-sensitive: pressing it in the commit
  list shows commit-list keybindings (navigation, operations, fragmap scroll),
  while pressing it in the commit detail view shows only the relevant subset
  (diff scroll, search, navigation back).

### Changed

- The hunk group matrix now defaults to the `highlight` theme, which dims the
  columns the selected commit does not touch and draws related commits in green
  (squashable) or red (conflicting) to focus attention on its relationships. The
  previous flat coloring remains available with `--theme plain` (or
  `GT_THEME=plain`).

### Removed

- `--squashable-scope` CLI flag and `GT_SQUASHABLE_SCOPE` environment variable.
  The per-hunk-group scope was tautologically always squashable and did not
  provide useful information. The tool now always uses per-commit squashability
  (the stricter rule matching the original fragmap tool).


## [0.5.0] - 2026-05-14

### Fixed

- Adjacent insertions in the same file now correctly cluster in the fragmap when
  non-touching commits create a generation gap between them.
- A chain of commits all touching the same cluster now correctly shows each
  adjacent pair as squashable. Previously the bottom commit was compared against
  the earliest ancestor rather than its direct predecessor, causing the chain to
  appear as conflicting even though no intermediate touches existed between
  adjacent pairs.
- The move-commit picker now scrolls to keep the insertion-point separator
  visible when it moves outside the current viewport.
- Entering commit detail mode (`i`) on a narrow terminal now correctly shows the
  commit detail panel.
- Dropping the root commit in `--all` mode now works. The first descendant
  becomes the new orphan root, with the dropped commit's content removed via a
  three-way merge. If a descendant modified a file created by the root, the
  conflict is surfaced for manual resolution.
- Moving the root commit to a later position in `--all` mode now correctly
  strips the root's content from the new orphan root instead of leaking it into
  the tree.
- Squashing or fixing up a commit into the root commit now works. Previously the
  operation was rejected with an error.
- Squashing or fixing up across a rename no longer produces a spurious conflict.
  Previously, if a file was renamed in a commit between the squash source and
  target, the operation would report a conflict on the old filename even though
  `git rebase -i` would complete cleanly.

### Added

- PageUp/Down now works in the move-commit, squash, and split-strategy pickers.
- All dialogs (drop confirmation, rebase conflict, split confirmation, split
  strategy picker, and help overlay) are now scrollable with Up/Down and
  PageUp/PageDown when the content is taller than the terminal.
- Ctrl-z suspends the TUI and returns to the shell on Unix (SIGTSTP). Pressing
  `fg` in the shell resumes the TUI exactly where it was left.
- The footer now shows a right-aligned `Press 'h' for help` hint so new users
  can discover key bindings without prior knowledge. The hint is shown only when
  there is enough space and is suppressed whenever a status or error message
  occupies the footer.
- The TUI now starts immediately and shows a live commit counter while loading.
  Previously the application blocked until all commits were read before showing
  anything. Ctrl-c during loading exits cleanly.
- Hunk group matrix computation is now interruptible throughout all phases
  (file clustering, deduplication, and matrix building). A progress dialog shows
  how many files have been clustered or how many commits have been processed;
  pressing `s` at any point skips the remaining work and opens the TUI without
  a matrix. The rest of the TUI is fully functional without it.

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
