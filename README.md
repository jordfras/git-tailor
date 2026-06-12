# git-tailor
![git-tailor logo](doc/git-tailor_banner.png)

An interactive terminal tool for tidying up Git commits on a branch — squash,
reorder, split, drop, and reword commits before submitting a pull request or
pushing to a shared branch.

The left panel shows your commits. The right panel shows a **hunk group matrix**
— a visual aid that instantly reveals which commits touch the same lines of
code, and whether combining them would be safe or risky.

> **⚠ Data safety — please read before use**
> 
> This tool rewrites history. Always push your branch to a remote before running
> it. The author takes **no responsibility** for data loss of any kind. Use at
> your own risk. See [LICENSE](LICENSE) for the full disclaimer.
> 
> This tool was developed entirely through AI-assisted ("vibe coded") sessions
> using AI agents. It has been reviewed and tested, but may contain bugs.


## Installation

```sh
cargo install --locked git-tailor
```

Requires Rust 1.85 or later.


## Usage

```sh
gt [base]
```

`[base]` identifies the branch or point you forked from — typically the target
branch of your pull request (e.g. `main` or `origin/main`). It does not need to
be a direct ancestor of `HEAD`: the **merge-base** (common ancestor) between
`[base]` and `HEAD` is used as the reference point. All commits between that
merge-base and `HEAD` are shown.

When `[base]` is omitted, `gt` automatically uses the repository's default
upstream branch by resolving `origin/HEAD`. If that is not configured it falls
back to `main`.

```sh
gt main              # commits on top of main
gt origin/main       # commits not yet pushed
gt v1.2.3            # commits since a tag
gt                   # auto-detect default branch (origin/HEAD or main)
```

**Flags:**

| Flag                 | Description                                                                        |
|----------------------|------------------------------------------------------------------------------------|
| `--reverse` / `-r`   | Show oldest commit at the top                                                      |
| `--full` / `-f`      | Show every raw hunk group column without deduplication                             |
| `--all`              | Browse the complete repository history from HEAD down to the root commit           |
| `--static` / `-s`    | Print the hunk group matrix to stdout and exit (no TUI)                            |
| `--no-color`         | Disable colors in `--static` output                                                |
| `--theme <THEME>`    | Hunk group matrix rendering theme: `highlight` (default), `plain`, or `classic`    |

Some flags can be persisted via environment variables so you don't have to pass
them every time:

| Environment variable          | Equivalent flag                 |
|-------------------------------|---------------------------------|
| `GT_REVERSE=1`                | `--reverse`                     |
| `GT_FULL=1`                   | `--full`                        |
| `GT_THEME=plain`              | `--theme plain`                 |


## The interface

![tui](doc/tui_example.png)

Each **column** in the hunk group matrix represents a group of related hunks (a
contiguous set of lines in a file that is touched by one or more commits). A
square `█` in a column means that the commit modifies lines in that hunk group. A
vertical line (`│` or `┃`) between two squares is a **connector** — it means the
two commits touch the same hunk group but are separated by other commits.

The default **highlight** theme focuses on the selected commit: the hunk groups
it touches are drawn at full brightness (with heavy `┃` connectors) while every
other column is dimmed. Pass `--theme plain` for a flat look with no dimming, or
`--theme classic` for the background-color style matching `--static` output. The
`plain` and `classic` themes render the matrix much like the
[fragmap](https://github.com/amollberg/fragmap) tool, so they may feel more
familiar if you are coming from there.

### Color legend — hunk group matrix (highlight theme)

| Color / style    | Meaning                                                                                                                |
|------------------|------------------------------------------------------------------------------------------------------------------------|
| Green square     | This commit can be cleanly squashed with the related, earlier commit in this hunk group                                |
| White square     | The earliest commit in a hunk group, or one that touches it but is not a clean squash                                  |
| Light red square | The selected commit's own square where squashing into the related commit would conflict                                |
| Green connector  | The two commits linked by the connector can be cleanly squashed                                                        |
| Red connector    | The two commits touching this hunk group have **conflicting** changes, reordering or squashing risks a merge conflict  |
| Dimmed column    | A hunk group the selected commit does not touch, shown for context                                                     |

Note that even though the colors indicate cleanly squashable, git may consider
the squash causing conflict since git-tailor considers no extra lines of context
while git does.

### Color legend — commit list (highlight theme)

When you select a commit, the other commits are colored relative to it:

| Color     | Meaning                                                                                                                           |
|-----------|-----------------------------------------------------------------------------------------------------------------------------------|
| Green     | Squashable partner — the currently selected commit can be cleanly squashed into the green commit or vice versa depending on order |
| Red       | Conflicting — the currently selected commit touches the same lines as the red commit; squashing or reordering may cause conflicts |
| Dim green | Fully squashable — every hunk group this commit touches is squashable; a good candidate to merge into another commit             |
| Normal    | No shared hunk groups with the currently selected commit                                                                          |

With `--theme plain` these same relationships are shown in yellow (squashable),
red (conflicting), and gray (fully squashable) instead.



## Key bindings

### Navigation

| Key                  | Action                              |
|----------------------|-------------------------------------|
| `↑` / `↓`, `j` / `k` | Move selection up/down              |
| `PgUp` / `PgDn`      | Move one page up/down               |
| `←` / `→`            | Scroll hunk group matrix left/right |
| `Ctrl ←` / `Ctrl →`  | Move the panel separator left/right |

### Operations

| Key | Action                                                                            |
|-----|-----------------------------------------------------------------------------------|
| `s` | **Squash** — merge the selected commit into an earlier one, message can be edited |
| `f` | **Fixup** — like squash, but discards the selected commit's message               |
| `m` | **Move** — pick a new position for the selected commit                            |
| `p` | **Split** — divide the selected commit into smaller commits                       |
| `r` | **Reword** — edit the commit message                                              |
| `d` | **Drop** — delete the selected commit entirely                                    |

### Views and other

| Key           | Action                           |
|---------------|----------------------------------|
| `Enter` / `i` | Toggle commit detail view (diff) |
| `h`           | Show help dialog                 |
| `u`           | Refresh commit list from HEAD    |
| `Esc` / `q`   | Close dialog / quit              |

### Search (commit detail view)

| Key   | Action                              |
|-------|-------------------------------------|
| `/`   | Open search bar (regex)             |
| `n`   | Jump to next match                  |
| `N`   | Jump to previous match              |
| `Esc` | Dismiss search                      |


## Attribution

Git-tailor is inspired by [tig](https://github.com/jonas/tig) and
[fragmap](https://github.com/amollberg/fragmap).

The source code of **fragmap** has been used by AI agents to produce code for
this tool — it is derived from or inspired by fragmap, which is licensed under
the Apache License, Version 2.0.

See [NOTICE](NOTICE) for full details.


## License

Apache License, Version 2.0 — see [LICENSE](LICENSE).

## Changelog

See [CHANGELOG.md](CHANGELOG.md) for a history of notable changes.
