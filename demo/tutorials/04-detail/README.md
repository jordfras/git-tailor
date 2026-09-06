# Reading a commit

The fourth tutorial: the commit detail view — opening it, moving around a diff,
searching it, and the one thing that surprises people, which is that no
operation works while it is open. Rendered with
`demo/build.sh video tutorials/04-detail`; see [`../../README.md`](../../README.md)
for the pipeline and [`video.conf`](video.conf) for what this video sets
differently from the promo.

| # | Scene | Shows |
|---|-------|-------|
| 00 | title | the logo and the name |
| 01 | open | `Enter` opens it; the divider, the metadata, the diff, and the keys that scroll |
| 02 | files | `f` / `F` jump file to file — the useful part |
| 03 | search | `/` as a regex, `n` / `N` across files, `Esc` to dismiss |
| 04 | context | `+` / `-` change how much is shown around each change |
| 05 | close | operation keys do nothing here, and `h` shows why; `q` first, then they work |

It is the short one: reading is easier to explain than rewriting, and the
material does not need the matrix.

## Chapter 1 widens the view; the rest arrive widened

The default split leaves the diff too narrow to read at this font size, so every
chapter is captured with the divider eight steps left. Chapter 1 does it on
camera and the narration accounts for it -- `Ctrl-Left` and `Ctrl-Right` are
worth knowing, and a width the viewer never saw arrive is a width they cannot
reproduce. Every later chapter moves the divider before `Show`, because a
tutorial that rearranges itself at the top of all six chapters is a tutorial
about its own layout.

## What the fixture has to keep producing

[`make-repo.sh`](make-repo.sh) is shaped for *reading* rather than for
operations, so the commit the video opens has to keep three properties:

- **four files in one commit**, or `f`/`F` have nowhere to jump;
- **a diff longer than the screen**, so scrolling is a real problem rather than
  a demonstration of a scrollbar — it is currently 68 lines against a viewport
  of about 28;
- **two changes in `client.rs` with a gap between them** — two hunks at three
  lines of context, one at five. Narrow that gap, or widen it much, and the
  context scene shows a diff that does not visibly change: with the first draft
  of this fixture the whole file fit in one hunk, so `+` and `-` did nothing at
  all on screen. Check with
  `git show -U3 HEAD~1 -- src/client.rs | grep -c '^@@'` against `-U5`;
- **a word in several files** (`timeout`, 11 occurrences), so stepping through
  matches crosses file boundaries and shows that search is not per file.

Every file must actually change in that commit. An earlier draft left `log.rs`
identical to its previous state, and the commit quietly became a three-file one.

## Check claims against frames, not against `--static`

Same rule as the other three, with one addition for this video: `--static` says
nothing about the detail view at all, so every claim here — what `f` jumps to,
what a search highlights, what `d` does while the view is open — can only be
checked by pulling the frame.
