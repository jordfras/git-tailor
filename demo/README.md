# demo/

Tooling for producing the git-tailor **README demo GIF** and the **promo
video**. Everything here is meant to be reproducible. The GIF and the video use
**separate tape files** — the README GIF is a short, small loop, while the promo
video uses its own, longer/higher-quality scene tapes — but both are rendered
with the same toolchain image described below.

## Quick start

The only host requirement is **Docker**. From the repo root:

```bash
demo/build.sh gif      # render every demo/gif/*.tape into demo/out/
```

That builds the toolchain image (first run only), compiles git-tailor, generates
a fresh demo repo, and runs vhs — leaving the rendered GIF(s) in `demo/out/`
(git-ignored, owned by you). Other subcommands:

```bash
demo/build.sh video    # render the promo scenes and compose demo/out/promo.mp4
demo/build.sh cues     # check the promo's keystrokes still follow its narration
demo/build.sh publish  # render the README demo and copy it to doc/demo.gif
demo/build.sh image    # just (re)build the toolchain image
demo/build.sh shell    # interactive shell inside the image
demo/build.sh clean    # remove demo/out/ and the build-cache volume
```

`publish` writes `doc/demo.gif`, which `README.md` references with a relative
path. crates.io resolves that against the repository, so the same committed file
renders on both GitHub and crates.io — run `publish` before a release (see
[`../RELEASE.md`](../RELEASE.md)) and commit the updated GIF.

## How it fits together

Two artifacts, one toolchain. The shared pieces sit at this level; everything
specific to one artifact lives in its own directory.

- [`Dockerfile`](Dockerfile) — the toolchain image (see below).
- [`build.sh`](build.sh) — host entry point; drives Docker so no other host
  tools are needed.
- [`render.sh`](render.sh) — runs *inside* the image: builds git-tailor,
  generates the fixture, renders the tapes. Shared, because capturing a tape is
  the same job either way.
- [`gif/`](gif) — the README demo GIF: one tape and the fixture it drives.
- [`promo/`](promo) — the promo video: narration, audio, and the composition
  pipeline.

Each artifact brings its own fixture — [`gif/make-repo.sh`](gif/make-repo.sh)
and [`promo/make-repo.sh`](promo/make-repo.sh) — regenerated before *every*
tape, since tapes rewrite history and would otherwise inherit each other's
leftovers. They are separate because the GIF's fixture engineers a rebase
conflict and its tape navigates by row offsets, so sharing one would mean
recalibrating and republishing `doc/demo.gif` every time the video changed.

## Toolchain image

The image ([`Dockerfile`](Dockerfile)) bundles everything a render needs, so vhs,
ffmpeg and a Rust toolchain need not be installed on the host:

- **Rust** (latest stable) — to build and run git-tailor
- **vhs** — deterministic terminal capture, driven by `.tape` scripts
- **ffmpeg** — frame encoding (used by vhs) and later video composition
- **ttyd + headless Chromium** — vhs's capture backend (from the base image)
- **git** — to script the throwaway demo repo that git-tailor operates on
- **vim** — the `$EDITOR` the tapes drive when scripting commit-message edits
