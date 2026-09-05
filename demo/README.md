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
demo/build.sh video    # render a video's scenes and compose it into demo/out/
demo/build.sh cues     # check a video's keystrokes still follow its narration
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
- [`promo/`](promo) — the promo video: scenes, narration, audio, and the
  composition pipeline. See [`promo/README.md`](promo/README.md).

Each artifact brings its own fixture — [`gif/make-repo.sh`](gif/make-repo.sh)
and [`promo/make-repo.sh`](promo/make-repo.sh) — regenerated before *every*
tape, since tapes rewrite history and would otherwise inherit each other's
leftovers. They are separate because the GIF's fixture engineers a rebase
conflict and its tape navigates by row offsets, so sharing one would mean
recalibrating and republishing `doc/demo.gif` every time the video changed.

## Adding another video

A **video** is any directory under `demo/` holding `scenes/` and
`make-repo.sh`. `promo/` is one; a tutorial series would be
`tutorials/01-matrix/` and so on. Both subcommands take it first, defaulting to
the promo:

```bash
demo/build.sh video tutorials/01-matrix          # all its scenes
demo/build.sh video tutorials/01-matrix 02-foo   # one of them
demo/build.sh cues  promo 04-pitch
```

Nothing needs teaching about the new name. `render.sh` derives the fixture from
the tape's own path — a tape at `<video>/scenes/<name>/scene.tape` films on
`<video>/make-repo.sh` — so each video brings its own repository, and a fixture
stays shaped for the story told on it.

What differs between videos goes in `<video>/video.conf`, sourced shell, every
key optional and defaulting to what the promo uses:

| Key | What it does |
|-----|--------------|
| `OUT_BASE` | output filename stem, so videos cannot overwrite each other |
| `PICTURE_FIT` | `scale` fills the frame; `pad` places the capture unresized, keeping the text as sharp as captured and the cell grid whole, which is what marker boxes are placed on |
| `TERMINAL_BG` | the terminal's own background: what a bumper fades to and what `PICTURE_FIT=pad` borders with, so both meet the picture invisibly |
| `TTS_VOICE`, `TTS_SPEED` | the read; `cue-check.sh` uses the same ones, so timings match the render. Both defer to the environment, so a voice can be auditioned without editing the file |
| `VOICE_FILTER` | path to a filter script, or `none` to leave the voice as synthesized |
| `BUMPER`, `BUMPER_HOLD`, `BUMPER_FADE`, `BUMPER_TO` | the opening ident, or `BUMPER=none` |
| `LOGO_SPIN` | how long a scene's logo takes to spin in and land; `0` for one that simply appears, with no impact cue under it |
| `MUSIC`, `MUSIC_LOOP_START`, `MUSIC_LOOP_END` | the bed, or `MUSIC=none` — which also removes the ducking that exists to serve it |

Everything else — frame compositing, matching a scene to its narration,
cross-fades, cards, stamps, cues — is the same for any video and stays in
[`promo/scripts/compose.sh`](promo/scripts/compose.sh). It lives under `promo/`
because that is the video it was written for; if a second series makes that
name misleading, moving it is a rename, not a rewrite.

## Toolchain image

The image ([`Dockerfile`](Dockerfile)) bundles everything a render needs, so vhs,
ffmpeg and a Rust toolchain need not be installed on the host:

- **Rust** (latest stable) — to build and run git-tailor
- **vhs** — deterministic terminal capture, driven by `.tape` scripts
- **ffmpeg** — frame encoding (used by vhs) and later video composition
- **ttyd + headless Chromium** — vhs's capture backend (from the base image)
- **git** — to script the throwaway demo repo that git-tailor operates on
- **vim** — the `$EDITOR` the tapes drive when scripting commit-message edits
