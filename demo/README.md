# demo/

Tooling for producing the git-tailor **README demo GIF** and the **promo
video**. Everything here is meant to be reproducible. The GIF and the video use
**separate tape files** — the README GIF is a short, small loop, while the promo
video uses its own, longer/higher-quality scene tapes — but both are rendered
with the same toolchain image described below.

## Quick start

The only host requirement is **Docker**. From the repo root:

```bash
demo/build.sh gif      # render every demo/tapes/*.tape into demo/out/
```

That builds the toolchain image (first run only), compiles git-tailor, generates
a fresh demo repo, and runs vhs — leaving the rendered GIF(s) in `demo/out/`
(git-ignored, owned by you). Other subcommands:

```bash
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

Shared pieces sit at this level; everything specific to one artifact lives in
its own directory, so a second artifact can be added without disturbing the
first.

- [`Dockerfile`](Dockerfile) — the toolchain image (see below).
- [`build.sh`](build.sh) — host entry point; drives Docker so no other host
  tools are needed.
- [`render.sh`](render.sh) — runs *inside* the image: builds git-tailor,
  generates the fixture, renders the tapes. Shared, because capturing a tape is
  the same job whatever is being captured.
- [`gif/`](gif) — the README demo GIF: its tape, the fixture it drives, and
  [`gif/README.md`](gif/README.md).

git-tailor is compiled into a Docker **cache volume** (not the host's `./target`)
so repeat renders reuse crate builds and the host tree is left untouched.

## Toolchain image

The image ([`Dockerfile`](Dockerfile)) bundles everything a render needs, so vhs,
ffmpeg and a Rust toolchain need not be installed on the host:

- **Rust** (latest stable) — to build and run git-tailor
- **vhs** — deterministic terminal capture, driven by `.tape` scripts
- **ffmpeg** — frame encoding (used by vhs) and later video composition
- **ttyd + headless Chromium** — vhs's capture backend (from the base image)
- **git** — to script the throwaway demo repo that git-tailor operates on
- **vim** — the `$EDITOR` the tapes drive when scripting commit-message edits
