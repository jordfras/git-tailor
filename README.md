# git-tailor
![git-tailor logo](doc/git-tailor_banner.png)

An interactive terminal tool for tidying up Git commits on a branch — squash,
reorder, split, drop, and reword commits before submitting a pull request or
pushing to a shared branch.

The left panel shows your commits. The right panel shows a **hunk group matrix**
— a visual aid that instantly reveals which commits touch the same lines of
code, and whether combining them would be safe or risky.

![git-tailor demo](doc/demo.gif)

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
  or by peeling a single file out into its own commit
- **Reword** — edit its message
- **Drop** — delete it entirely
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


## Notes

### A single hunk can span more than one hunk group

The matrix columns are not simply "one per hunk." A hunk group is a range of
lines that one or more commits touch, and its boundaries fall wherever *any*
commit's hunk starts or ends. So when two commits change overlapping but not
identical line ranges, the shared region and the non-overlapping remainders each
become a separate group — and a single hunk from one commit can then cover more
than one group, showing up as squares in two (or more) adjacent columns on the
same row.

This is worth keeping in mind when splitting a commit:

- **Per hunk group** cannot slice a hunk along a group boundary. A hunk is the
  smallest unit Git can apply on its own, so when one hunk spans several groups
  those groups stay together in the same resulting commit. The split can
  therefore produce *fewer* commits than there are columns.
- **Per hunk** splits by the commit's actual diff hunks, not by columns. A hunk
  drawn across two columns looks like two changes in the matrix, but it is a
  single hunk and yields a single commit — it cannot be divided further.

In short, the matrix shows how changes *relate* across commits; it does not
promise that every column is independently splittable.

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
