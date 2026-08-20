# Changelog

All notable changes to this project will be documented in this file.

The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).


## [Unreleased]

### Added

- Shell completion for `bash`, `zsh`, and `fish`. It covers all flags and value
  options (e.g. `--matrix-theme`), and completes the base argument with branch
  and tag
  names from the current repository. Install it with
  `gt completions --shell <bash|zsh|fish> --install` (or omit `--install` to
  print the script) — no package manager required. Candidates are computed by
  `gt` at completion time, so they always match the installed version and the
  repository's current refs.
- Operation picker: press `Space` on any row to open a menu of the operations
  available for it, so you can run one without memorizing its shortcut key. The
  menu is filtered to the selected row — a real commit offers
  split/squash/fixup/reword/move/drop, the Staged row offers commit/unstage, the
  Unstaged row offers stage, and undo/redo are always available. Each entry
  shows its shortcut key, and pressing that key inside the dialog runs the
  operation directly. In the commit detail view `Space` still pages down.
- Adjust the diff context lines in the commit detail view with `+` and `-`
  (default 3, minimum 0). The diff updates immediately: increasing the context
  merges hunks whose surrounding lines overlap into one section, and decreasing
  it splits them apart again. The current count is shown in the detail header.
- `--palette` (env `GT_PALETTE`) chooses the color palette. The default
  `terminal` uses your terminal's own colors; `campbell` and `dark+` are
  built-in dark schemes rendered on any terminal, keeping the UI readable on
  light or pastel themes where it would otherwise wash out. Any other value is a
  path to a Windows Terminal color-scheme JSON file, so you can apply any custom
  scheme.
- Bulk "Autofixup" (`F`, or "Autofixup" in the operation picker), mirroring
  `git rebase --autosquash`: scans the branch for `fixup!`/`squash!`-prefixed
  commits, matches each to the earlier commit its summary names, and squashes
  them into their targets in one pass — so a stack of small fixup commits
  written while preparing a branch can be cleaned up with one keypress instead
  of squashing them in one at a time. Shows a confirmation dialog listing what
  will happen before running, and the whole batch is a single undoable
  operation.
- "Split out hunk(s)" split strategy: pick one or more hunks — possibly across
  several files — from a commit and peel them into their own commit, leaving
  the rest behind under the original message. Choosing it from the split
  strategy menu opens a picker dialog listing the commit's hunks with a diff
  preview of the highlighted one, so you can see the code before selecting.
- "Edit" operation (`E`, or "Edit" in the operation picker), like
  interactive-rebase's `edit`: checks out the selected commit and drops you into
  a shell to change it by hand — fix a few lines and `git commit --amend`, or
  `git reset HEAD~` and re-commit in pieces to split it into several commits.
  Run `exit` when done (exit without changing the commit to cancel); git-tailor
  then replays the following commits onto your result. If you exit with
  uncommitted changes it re-opens the shell so nothing is ever silently
  discarded. The edit is undoable, and an interrupted edit is recovered on the
  next run. Requires a clean working tree unless `--autostash` is set.

### Changed

- The hunk-group matrix theme option was renamed from `--theme` to
  `--matrix-theme` (env `GT_THEME` → `GT_MATRIX_THEME`), to distinguish it from
  the new `--palette` color option. The old `--theme` name and `GT_THEME` are no
  longer accepted.
- git-tailor no longer falls back to `vi` when no editor is configured. If
  neither `core.editor` nor the `GIT_EDITOR`/`VISUAL`/`EDITOR` environment
  variables are set, it now reports a clear error asking you to configure one,
  instead of failing cryptically on systems without `vi` (such as Windows).
- "Split out file" is now "Split out file(s)": pick one or more changed files
  — not just one — from a commit and peel them into their own commit together.
  Choosing it from the split strategy menu opens a picker dialog listing the
  commit's changed files with a diff preview of the highlighted one, mirroring
  the "Split out hunk(s)" picker.
- The crash-recovery journal format changed (v1 → v2). A journal with no
  operation in flight is migrated automatically on first run and your undo/redo
  history is preserved. If git-tailor was left paused at a merge conflict (or in
  an in-progress "Edit" shell) at the moment you upgraded, the new version cannot
  resume that record: it tells you so and leaves the old journal untouched, so
  you can finish the operation with the previous git-tailor — or run
  `gt --clean-journal` to discard it and start fresh.

### Fixed

- Error messages in the status bar now include the underlying cause, not just
  the outermost summary — so a failed operation says what actually went wrong
  instead of only that it failed.
- Rewording or splitting a commit is now refused, with an explanation, when a
  merge commit sits between it and the branch tip. Previously you got libgit2's
  raw "mainline branch is not specified" error — and because the walk that
  collects the commits to replay is unreliable across a merge, it could also
  stop early and replay the wrong set. Both operations now check up front and
  leave the branch untouched.
- Split per hunk group now separates hunks by the exact set of commits their
  lines relate to, computed directly from the diffs, and not only by the column
  they sit in — so for instance a change that a later commit reworks lands in
  the piece related to that commit. A commit whose hunks all share one column
  and one relation set is no longer refused (e.g. a single hunk overlapping two
  other commits' regions): that one hunk is sliced at the relation boundary to
  make the split possible. Hunks otherwise stay whole, so the resulting commits
  never become related to each other.
- Auto-stash (`--autostash`) no longer silently drops an unstaged edit that keeps
  a file's size unchanged (e.g. flipping a single character). libgit2's stash
  could treat such an edit as unmodified when its timestamp collided with the
  index's cached stat, discarding it; git-tailor now refreshes the index against
  the working tree before stashing so the change is preserved.
- Transient footer messages are no longer wiped before they can be read — for
  example, an error shown when your editor fails to launch during reword,
  commit, or squash. Such messages are now dismissed only by an actual key
  press, not by resize or focus events (which the terminal can emit, most
  notably on Windows, when the alternate screen is restored after an external
  tool).
- The commit detail view no longer jumps to re-centre a search match that is
  already on screen. This showed up when the diff got shorter while a search was
  active — pressing `-` to reduce the context lines, or making the terminal
  taller — and moved the view even though the highlighted match had not gone
  anywhere.
- Scrolling back up in a dialog no longer appears frozen after the dialog grows
  — for example when the terminal is made taller while the dialog is scrolled to
  the bottom. Previously the first several key presses did nothing, because the
  stored scroll position was left past the new end of the content and had to be
  walked back down before the view would move.
- Redrawing is much faster on Windows, where moving the cursor could visibly
  paint a row in pieces.
- Scrolling and searching the commit detail view are much faster in large
  repositories. The diff is now read when the view is opened and when `+` / `-`
  changes the context width, instead of on every redraw.


## [2.0.0] - 2026-07-02

### Added

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
- `--clean-journal` flag that removes all git-tailor recovery state — the journal
  file under `.git/git-tailor/` and every ref under `refs/git-tailor/*` (undo
  pins and the in-progress pin) — then exits without launching the TUI and prints
  a summary. A manual escape hatch for when recovery state gets stuck; refs are
  found by namespace, so stray ones are cleared even if the journal is missing or
  out of sync.
- Commit the staged changes without leaving git-tailor. With the Staged row
  selected, press `c` to open your editor for a commit message and create a
  commit from the index. The commit is undoable: `u` soft-resets it (the changes
  reappear as staged, your working-tree edits untouched) and `Ctrl-r` re-commits.
- Stage and unstage all working-tree changes without leaving git-tailor. With the
  Unstaged row selected, press `a` to stage every change (modifications, new
  files, and deletions, like `git add -A`); with the Staged row selected, press
  `A` to unstage everything back to HEAD. Both are undoable and redoable with
  `u` / `Ctrl-r`.
- Opt-in auto-stash via the `--autostash` flag (or `GT_AUTOSTASH` env var). When
  enabled, operations that need a clean working tree — split, move, drop, squash,
  fixup, undo, and redo — automatically stash your staged, unstaged, and
  untracked changes, run the operation, and restore the exact staged/unstaged
  split afterwards, instead of refusing to run. The stash is recorded in the
  operation journal so it survives a crash or a conflict pause. If reapplying the
  changes conflicts with the result, the same resolution dialog used for
  cherry-pick conflicts opens — resolve with the merge tool (`m`) or editor
  (`e`) and continue, or press Esc to abort the whole operation and get your
  changes back unchanged. Disabled by default, matching git's own
  `rebase.autoStash` behaviour.
- Scroll the commit list without moving the selection. `Ctrl-Up` / `Ctrl-Down`
  scroll the list one row at a time while keeping the selected commit
  highlighted, stopping before it would scroll off screen.

### Changed

- Refreshing the commit list moved from `u` to `R` / `F5`, freeing `u` for undo
  (the vim convention). "Refresh" replaces the previous "update" wording.

### Fixed

- The Staged / Unstaged detail view now shows the same surrounding diff context
  as commits (git's default) instead of only the changed lines, so they are no
  longer harder to read than a commit's diff.
- Dropping, moving, or squashing past a commit that added a file no longer leaves
  that file behind as an untracked leftover in the working tree. git-tailor now
  removes exactly the files the rewrite removed (matching `git rebase`), while
  leaving your own untracked files in place.
- A panic now restores the terminal and prints its message instead of leaving a
  blank screen. Previously the terminal teardown wiped the panic output, so a
  crash appeared to exit with no information at all.
- Moving a commit toward the end of the list (e.g. two or three positions
  "down") no longer crashes. Navigating the insertion cursor past the last
  commit left the selection pointing out of bounds; once the move was confirmed
  or cancelled, the next commit-list render indexed the list out of bounds and
  panicked.


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
