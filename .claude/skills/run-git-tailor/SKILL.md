---
name: run-git-tailor
description: Build, run and drive the gt TUI (git-tailor) against a scratch repo. Use when asked to run, start or try gt, confirm a change works in the real app, drive a split/squash/reword/move by keys, or capture what the screen shows.
---

gt is a full-screen TUI, so an agent drives it inside tmux with
`.claude/skills/run-git-tailor/driver.sh` (start / keys / wait / screen /
stop) against a throwaway repo built by `.claude/skills/run-git-tailor/fixture.sh`.
Paths below are relative to the repository root.

## Prerequisites

`cargo` and `tmux` on `PATH`. Nothing else: gt uses libgit2, not the git CLI
(the fixture script does use `git` to build its repo).

## Build

```bash
cargo build
```

The driver runs `target/debug/gt`; set `GT=/path/to/gt` to use another binary.

## Run (agent path)

Build a scratch repo, launch gt on it, drive it, read the screen, then check
the result with plain `git`:

```bash
D=.claude/skills/run-git-tailor
R=/tmp/gt-scratch            # anywhere; the fixture deletes it first
$D/fixture.sh $R
$D/driver.sh start $R main   # gt's BASE argument: list feature's commits above main
$D/driver.sh keys g p j j Enter   # top commit, split, third strategy (per hunk)
$D/driver.sh wait "Confirm Split"
$D/driver.sh keys Enter
$D/driver.sh wait "Commit split"
$D/driver.sh screen
$D/driver.sh stop
git -C $R log --oneline main..
```

| command | what it does |
|---|---|
| `driver.sh start REPO [GT_ARGS...]` | launch gt in REPO in tmux session `gt` (140x30), wait until the list is drawn |
| `driver.sh keys KEY...` | send tmux keys one at a time (`Enter`, `Space`, `Esc`, `Up`, or a character) |
| `driver.sh wait TEXT [SECS]` | wait for TEXT on screen; on timeout prints the screen and fails |
| `driver.sh screen` | print the screen, with the empty rows of dialog borders dropped |
| `driver.sh stop` | kill the session |

`fixture.sh DIR` makes `main` (base) and `feature` with two commits:
**change** edits text in `a.txt` (two hunks), `b.txt` and `both.sh`, changes
the binary `bin.dat`, adds an empty file, and makes `run.sh` (mode only) and
`both.sh` executable; **later** touches `b.txt` again, giving two hunk groups.
`fixture.sh DIR single` makes one commit: a single text hunk beside a binary
file.

Operations that open an editor (reword `r`, squash `s`) take a scripted one:

```bash
GIT_EDITOR="sed -i 1s/.*/reworded/" $D/driver.sh start $R main
$D/driver.sh keys g r
$D/driver.sh wait reworded
```

### Keys

Commit list: `j`/`k` move, `g`/`G` top/bottom, `p` split, `s` squash,
`f` fixup, `m` move, `r` reword, `d` drop, `u` undo, `i` detail view,
`h` help, `q` quit. The split dialog lists Per file, Split out file(s),
Per hunk, Per hunk group, Split out hunk(s), in that order; `j` then
`Enter`. The hunk and file pickers take `Space` to toggle, `Enter` to split.

## Static output (no TUI)

```bash
cd $R && /path/to/git-tailor/target/debug/gt --static --no-color main
```

Prints the hunk-group matrix and exits: `#` touch, `.` empty.

## Run (human path)

```bash
cd some/repo && /path/to/git-tailor/target/debug/gt main   # q to quit
```

## Test

```bash
cargo fmt && cargo clippy --all-targets && cargo test
```

## Gotchas

- **The agent shell exports `GIT_EDITOR=true`.** gt checks `GIT_EDITOR` before
  `core.editor`, so a reword "edits" with `true`, gets the message back
  unchanged and reports "No changes made". Setting `core.editor` in the
  fixture does nothing; pass `GIT_EDITOR=...` to `driver.sh start`, which
  forwards it into the tmux session.
- **The list is oldest first and the cursor starts on HEAD, at the bottom.**
  Press `g` before acting on the first commit.
- **Pass the base.** Without a BASE argument gt resolves `origin/HEAD` and
  falls back to `main`; the fixture has no remote, so name `main` explicitly
  and the list holds only the feature commits.
- **Splits into more than 5 commits stop at a "Confirm Split" dialog.** Wait
  for it and send `Enter`; a split that stays at 5 or fewer runs at once.
- **Check results with `git`, not the screen.** The screen shows summaries;
  `git log --stat --summary main..` shows which piece holds which file and
  mode change, and `git diff <original> <last piece>` must be empty for a
  split.

## Troubleshooting

- **`driver: 'X' did not appear within 10s`**: the screen it printed says
  why — usually an error in the status line, or a dialog still open.
- **"No changes made" after a reword or squash**: the editor was `true`; see
  the first gotcha.
