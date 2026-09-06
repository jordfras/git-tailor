# Work in progress, and getting back

The third tutorial: the changes you have not committed yet, and every way back
from an operation. Rendered with `demo/build.sh video tutorials/03-in-progress`;
see [`../../README.md`](../../README.md) for the pipeline and
[`video.conf`](video.conf) for what this video sets differently from the promo.

| # | Scene | Shows |
|---|-------|-------|
| 00 | title | the logo and the name |
| 01 | rows | the staged and unstaged rows, and `a` / `A` / `c` |
| 02 | fold | `f` from the unstaged row straight into an older commit |
| 03 | autofixup | `F` folding every `fixup!` commit in one pass |
| 04 | conflict | the resolution dialog, resolved through a real merge tool |
| 05 | guard | the refusal over a dirty tree, then `--autostash` |
| 06 | undo | `u` back through the session, and across a restart |
| 07 | close | one line, closing the series |

## What the fixture has to keep producing

[`make-repo.sh`](make-repo.sh) is the only fixture here that ships a **dirty
working tree**, because the first two scenes are about what a dirty tree puts
below the commits, where they belong in the order:

- a **staged** edit and an **unstaged** edit in *different files*, so the two
  rows separate cleanly and either can be folded on its own;
- the unstaged edit must belong to a commit several rows up — that fold is the
  reason the video exists, and it has to reach *past* something rather than into
  the row above it;
- two `fixup!` commits whose subjects name their targets exactly as
  `git commit --fixup` writes them, or autofixup matches nothing;
- a last commit (`refactor: send warnings through the logger`) that belongs with
  `fix: send warnings to stderr` but has to reach past the `fixup!` between
  them, so folding it back conflicts for real in scene 04. It changes the
  *macro* on that line rather than the prefix on purpose: two commits that both
  end at the same text merge cleanly however far apart they are, and an earlier
  draft of this fixture did exactly that and produced no conflict at all;
- that same last commit is what scene 05 drops, because nothing is replayed
  after it — dropping anything earlier stops on a conflict from a later commit
  depending on it, which is not that scene's subject.

Scenes 03, 04, 06 and 07 reset the tree in their hidden setup: a rewrite refuses
to run over uncommitted changes, and that refusal is scene 05's subject rather
than an accident in the others.

The fixture sets `merge.tool = vimdiff` and nothing else: git-tailor knows that
tool built in, so one config line is the whole setup a viewer would need. It is
picked over kdiff3 or meld because it runs *in the terminal* — a GUI tool opens
a window the recording never sees. `core.editor` is set for the same reason as
tutorial 2: git-tailor has no `vi` fallback.

## `eigh` in the narration is the letter a

`scenes/01-rows/narration.txt` says "eigh stages every tracked change", and that
is not a typo. The reader phonemizes a bare `a` as the article — a schwa — so
the line came out as "*uh* stages every tracked change". Of the spellings that
reach the same reader, `eigh` is the one that gives the letter's own sound;
`ay`, `aye` and `ei` all give "eye" instead, and a capital `A` changes nothing.

The caption on screen still reads `a stages, A unstages, c commits`, which is
what the viewer needs to see. Only the spoken form is spelled for the reader.
Test a candidate with `espeak-ng -x -q "<text>"` inside the toolchain image: it
is the same phonemizer the voice uses, so what it prints is what you will hear.

## Check claims against frames, not against `--static`

The rule from the first two videos holds, and the working-tree rows add a second
trap: `--static` prints them, but nothing about the staged/unstaged split is
visible in its symbols. If the narration says the staged change stayed staged,
pull the frame and read the rows.
