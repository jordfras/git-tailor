# demo/gif/

The README demo GIF: one tape and the fixture it drives, rendered with
`demo/build.sh gif` and published to `doc/demo.gif` with `demo/build.sh publish`.

[`demo.tape`](demo.tape) is a single follow-along flow — view a stray fix, fold
it in as a fixup, drop a debug commit, resolve the conflict that surfaces in the
editor, land a clean history. It is one continuous story rather than a feature
tour, because a loop that has to be understood in eight seconds cannot afford to
change subject.

[`make-repo.sh`](make-repo.sh) builds the throwaway `calc` repo it runs on:
`main` holds only a README (the fork point), and `feature/calc` stacks nine
commits engineered to exercise every operation — a rich hunk group matrix, two
clean fixup partners, a `DROPME` debug commit, a rebase conflict that surfaces
only when `DROPME` is dropped, and a commit bundling two concerns. Identity and
timestamps are fixed, so the log reads the same on every render.

Keystroke counts in the tape were verified against the real binary before it was
written. There is no way to assert them from the tape itself, so a change to
git-tailor's navigation can silently desynchronise it — watch the result, don't
just check that it rendered.

## Why the file is as small as it is

GIF size is driven by frame count and per-frame entropy, not by pixel
dimensions, which is why this GIF got **bigger on screen and smaller on disk**
at the same time. Three things do the work, and they compound:

- `Set FontSize 18` at 1240×500 — legible without a large canvas.
- `Set Framerate 24`, against vhs's default of 50. Terminal captures are mostly
  static; the dropped frames are duplicates.
- `gifsicle -O3 --lossy=30 --colors 128` after the render (see
  [`../render.sh`](../render.sh)).

`gifsicle` alone takes off about a third — the last render went 1.56 MiB to
1.02 MiB, which is roughly what `doc/demo.gif` weighs. Re-measure rather than
trusting that figure if the tape grows: it scales with how many frames the walk
takes, not with anything about the settings.

## Colors come from the image, not the terminal

The tape launches a bare `gt`. The toolchain image sets `GT_PALETTE=dark+`, so
the capture does not depend on whatever theme the recording terminal happens to
use. The tape needs no `Set Theme` of its own — adding one would only create a
second place to keep in step.

## Publishing

`demo/build.sh publish` writes `doc/demo.gif`, which the project `README.md`
references with a **relative** path. crates.io rewrites relative README URLs
against the repository and serves the image from there, so one committed file
renders on both GitHub and crates.io — absolute raw URLs are not needed. The
corollary is that the media never ships inside the crate: `doc/` and `demo/` are
excluded from the package, and the file only has to be committed on the default
branch. Re-render before a release; see [`../../RELEASE.md`](../../RELEASE.md).
