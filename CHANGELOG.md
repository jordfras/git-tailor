# Changelog

All notable changes to this project will be documented in this file.

The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

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
