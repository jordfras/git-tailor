# demo/

Tooling for producing the git-tailor **README demo GIF** and the **promo
video**. Everything here is meant to be reproducible. The GIF and the video use
**separate tape files** — the README GIF is a short, small loop, while the promo
video uses its own, longer/higher-quality scene tapes — but both are rendered
with the same toolchain image described below.

## Toolchain image

Rather than requiring vhs, ffmpeg and a Rust toolchain to be installed on the
host, everything runs inside a Docker image defined by [`Dockerfile`](Dockerfile):

- **Rust** (latest stable) — to build and run git-tailor
- **vhs** — deterministic terminal capture, driven by `.tape` scripts
- **ffmpeg** — frame encoding (used by vhs) and later video composition
- **ttyd + headless Chromium** — vhs's capture backend (from the base image)
- **git** — to script the throwaway demo repo that git-tailor operates on
- **vim** — the `$EDITOR` the tapes drive when scripting commit-message edits

### Build the image

```bash
docker build -t git-tailor-demo demo/
```

### Use it

Mount the repository into the container and run whatever you need. For example,
an interactive shell:

```bash
docker run --rm -it -v "$PWD:/work" git-tailor-demo
```

Inside the container the repo is at `/work`, so you can `cargo build --release`
and then render a tape with `vhs some.tape`.
