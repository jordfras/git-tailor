# Visualization of commit relations

The first tutorial: what the hunk group matrix shows, and how to read a commit's
relationships off it. Rendered with `demo/build.sh video tutorials/01-matrix`;
see [`../../README.md`](../../README.md) for the pipeline and
[`video.conf`](video.conf) for what this video sets differently from the promo.

| # | Scene | Teaches |
|---|-------|---------|
| 00 | title | the logo and the name |
| 01 | columns | reading order (`--reverse`), rows line up with the list, columns are groups of hunks |
| 02 | colors | dimming, and the green clean fold |
| 03 | connectors | the same relationship across a gap, then red |
| 04 | context | zero context here, three lines when git squashes |
| 05 | list | the two list colors that follow the selection, symmetric in either direction |
| 06 | dim | dim as one idea across both panes, then the payoff |

## What the fixture has to keep producing

[`make-repo.sh`](make-repo.sh) is built so every state the narration names is on
screen with a reason. Change it and these have to survive, or scenes start
describing something that is not there:

- a **white** square — a region's first toucher, with nothing to fold into;
- a **green** square and connector — a commit whose regions all lead back to one
  earlier commit, with nobody in between;
- a **spanning** connector — that relationship drawn across commits that touched
  something else;
- a **red** square and connector — a commit whose regions lead back to two
  different commits, so it can fold into neither;
- a **dim green** commit — one that can fold somewhere, seen from a selection it
  is unrelated to. The documentation commit is the only row in this fixture that
  is related to nothing, so it is the only vantage point that shows this.

The green and the red both hinge on the same rule, which is worth knowing before
editing the history: a commit is green when *every* region it touches leads back
to the same earlier commit. A commit that changes two unrelated things at once is
red however clean each change looks on its own.

## Check claims against frames, not against `--static`

Three narration claims in this video were wrong when first written, and every one
was caught by extracting a frame and looking at it:

- the cursor sat on the wrong commit while a color was being named;
- the connector on screen was red while the narration called it clean;
- an earlier draft explained context using the diff view, whose `+`/`-` setting
  has nothing to do with how the matrix groups hunks.

`gt --static --no-color` is not a substitute. Its symbols are easy to misread by
a column, and it says nothing about the commit list's colors. If the narration
asserts something visual, pull the frame at that timestamp and check it.
