# git-tailor
![git-tailor logo](doc/git-tailor_banner.png)

An interactive terminal tool for tidying up Git commits on a branch — squash,
reorder, split, drop, reword, and bulk-autofixup commits before submitting a
pull request or pushing to a shared branch.

The left panel shows your commits. The right panel shows a **hunk group matrix**
— a visual aid that instantly reveals which commits touch the same lines of
code, and whether combining them would be safe or risky.

![git-tailor demo](doc/demo.gif)


## Installation

```sh
cargo install --locked git-tailor
```

Requires Rust 1.85 or later.

Or download a pre-built `gt` binary for Linux (x86_64), Windows (x86_64), or
macOS (Apple Silicon or Intel) from the
[latest release](https://github.com/jordfras/git-tailor/releases/latest) — no
Rust toolchain needed. The Linux build is statically linked and runs on any
distribution (and WSL2).


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

`gt` also has options for things like reversing the commit order, browsing the
complete repository history, choosing a hunk group matrix theme, and printing
the matrix to stdout without launching the TUI (handy in scripts). Run
`gt --help` for the full list of options, along with the environment variables
that can set their defaults.


## Operations

Press **Space** on any row to open a menu of the operations available for it —
handy while you are still learning the individual shortcut keys. With a commit
selected you can:

- **Squash** — merge it into an earlier commit (with an editable combined
  message)
- **Fixup** — like squash, but discard the selected commit's message
- **Move** — reorder it to a new position
- **Split** — divide it into smaller commits, by file, by hunk, by hunk group,
  or by picking one or more files/hunks to peel out into their own commit
- **Reword** — edit its message
- **Drop** — delete it entirely
- **Edit** — check out the commit and drop into a shell to rewrite it by hand
  (fix a few lines and `git commit --amend`, or `git reset HEAD~` and re-commit
  in pieces to split it); on exit the following commits are replayed onto your
  result
- **Autofixup** — not tied to the selected commit: scans the whole branch for
  `fixup!`/`squash!`-prefixed commits and squashes each into the commit its
  message names, in one bulk pass, after showing a confirmation of what will
  happen
- **Undo / redo** — every operation can be undone and redone, and the undo
  history is kept even after you quit and reopen `gt`

Pressing `Enter` (or `i`) opens the **commit detail view** with the full diff
and incremental regex search. If an operation hits a merge conflict, git-tailor
opens a resolution dialog where you can fix it up in your editor or merge tool
and then continue or abort.

By default, history-rewriting operations refuse to run when the working tree has
uncommitted changes, so your work is never discarded. Pass `--autostash` (or set
`GT_AUTOSTASH=true`) to let git-tailor stash those changes first, run the operation,
and reapply them afterwards — preserving your exact staged/unstaged split —
instead of refusing.

Press `h` in the TUI for the complete key-binding reference — including all
navigation, scrolling, and search keys.


## The interface

![tui](doc/tui_example.png)

Each **column** in the hunk group matrix represents a group of related hunks (a
contiguous set of lines in a file that is touched by one or more commits). A
square `█` in a column means that the commit modifies lines in that hunk group. A
vertical line (`│` or `┃`) between two squares is a **connector** — it means the
two commits touch the same hunk group but are separated by other commits.

The default **highlight** theme focuses on the selected commit: the hunk groups
it touches are drawn at full brightness (with heavy `┃` connectors) while every
other column is dimmed. Pass `--matrix-theme plain` for a flat look with no
dimming, or `--matrix-theme classic` for the background-color style matching
`--static` output. The
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

With `--matrix-theme plain` these same relationships are shown in yellow (squashable),
red (conflicting), and gray (fully squashable) instead.

### Colors and palettes

The screenshot above uses git-tailor's built-in **Dark+** palette. By default
(`--palette terminal`) git-tailor adopts your terminal's own colors, which works
best on a dark background — the matrix, diff, and bars are designed for one. On a
light or pastel theme the UI can wash out; pass `--palette campbell` or
`--palette dark+` to render a fixed dark scheme on any terminal.

You can also point `--palette` at a
[Windows Terminal color-scheme](https://learn.microsoft.com/windows/terminal/customize-settings/color-schemes)
JSON file to use any custom palette:

```sh
gt --palette ~/my-scheme.json
```

Ready-made schemes in that format are available from
[windowsterminalthemes.dev](https://windowsterminalthemes.dev/) and, in the
`windowsterminal/` folder of the
[iTerm2-Color-Schemes](https://github.com/mbadolato/iTerm2-Color-Schemes)
collection (whose native `.itermcolors` format is *not* accepted directly).


## Notes

### Rewriting history safely

git-tailor rewrites branch history, so the usual rewrite caveats apply. To keep
that safe, every operation is journalled before it runs: anything can be undone
and redone, and an interrupted run is recovered the next time you start `gt`.
Operations also refuse to run on a dirty working tree unless you ask for
`--autostash`. As with any history rewriting, having the branch pushed to a
remote is still a good extra safety net.

The tool is developed through AI-assisted ("vibe coded") sessions, with a large
automated test suite, and is used daily for real work. It comes with no warranty
of any kind — see [LICENSE](LICENSE) for the full disclaimer.

### A single hunk can span more than one hunk group

A hunk group isn't the same thing as a hunk — it's a chunk of code *and* which
commits touched it. If your hunk overlaps only part of an earlier commit's
change, the overlapping lines and the rest of the hunk belong to different
commits, so they land in different hunk groups. That's why one hunk in your
diff can show up as squares in more than one column of the same row.

This matters when splitting a commit:

- **Per hunk group** keeps hunks whole wherever possible, aiming for one
  result commit per column. If a hunk truly needs to be divided to make the
  split possible at all, git-tailor divides just that one hunk — and no
  further, since slicing every hunk along column lines would make the
  resulting commits look related to each other again, defeating the point of
  splitting. So you can end up with fewer commits than columns, and the
  columns shown after a split may differ from before — both are expected.
- **Per hunk** splits along the commit's actual diff hunks, not by column. A
  hunk spanning two columns still becomes one commit here — use per hunk
  group if you want it divided by column.

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
