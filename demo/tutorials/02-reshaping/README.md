# Reshaping a branch

The second tutorial: the operations that rewrite commits you already have.
Rendered with `demo/build.sh video tutorials/02-reshaping`; see
[`../../README.md`](../../README.md) for the pipeline and
[`video.conf`](video.conf) for what this video sets differently from the promo.

| # | Scene | Shows |
|---|-------|-------|
| 00 | title | the logo and the name |
| 01 | picker | `Space` lists what a row can do, then `h` lists the view's keys |
| 02 | split | the five strategies named, per hunk group demonstrated |
| 03 | limits | grouping comes from the branch; a commit it refuses to split |
| 04 | fold | the two green pieces: `s` into one target, `f` into the other |
| 05 | reword | the white piece — nowhere to fold, so `r` gives it a message |
| 06 | move-drop | `m` with the matrix's blessing, then `d` with its confirmation |
| 07 | edit | `E` into a shell and back |
| 08 | close | one line, pointing at tutorial 3 |

The spine is one worked example rather than a tour of keys: a commit that did
three things is split into three, and then each piece is dealt with according to
what the matrix says about it — squash, fixup, reword. Move, drop and edit
follow as the operations that idea does not reach.

Autofixup, the working-tree rows and undo are tutorial 3: this video is about
commits that already exist, that one is about work that does not yet.

## What the fixture has to keep producing

[`make-repo.sh`](make-repo.sh) gives every operation one obvious target, and two
of them are load-bearing:

- the **split** commit (`refactor: tidy up config, logging and startup`) must
  touch exactly three regions leading back to three different places, so per
  hunk group yields three pieces, each in one column, two of them green
  afterwards. Check with `count_split_per_hunk_group`, or by eye: three white
  squares in three columns before, three rows with one square each after;
- the **refused** commit (`docs: explain how to run it`) must touch one region
  related to nothing, so per hunk group has nothing to separate and says so.

Two more are quieter but will ruin a take if they drift:

- the **dropped** commit must be the *newest* toucher of its region, or the drop
  conflicts on camera;
- the **moved** commit must share no file with anything it passes, for the same
  reason.

`core.editor` is set in the fixture because reword and squash open an editor and
git-tailor has no `vi` fallback — with nothing configured they fail outright.

Scenes 04 and 05 start where scene 02 leaves off, which no scene can inherit:
every scene is handed a freshly built repository. They rebuild it in the
fixture's **`after-split`** mode instead, where commit 6 is already the three
commits the split produces. Both shapes are built from the same file contents in
`make-repo.sh`, so the pieces stay identical to the ones the real split makes —
change one and change the other.

## Check claims against frames, not against `--static`

The rule from tutorial 1 applies here and bit again while this one was written:
after the split, `gt --static --no-color` draws the connectors above the pieces
as `^`, which the legend calls conflicting. They are not — with color they are
yellow, every one of them squashable. If the narration asserts something
visual, pull the frame at that timestamp and look at it.
