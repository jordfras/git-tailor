# Changelog

All notable changes to this project will be documented in this file.

The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).


## [Unreleased]

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
- Fragmap now tracks file renames across commits: when a file is renamed,
  overlapping spans in the old and new paths are correctly clustered together
  instead of being treated as unrelated files.
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
