# Developing git-tailor with Claude Code

Install these before starting a Claude Code session in this repository. The
hooks in `settings.json` enforce the Code Quality rules in `CLAUDE.md`, and they
need these tools to work.

| Tool | Why | Install |
|------|-----|---------|
| Rust stable, with `rustfmt` and `clippy` | Build, format and lint. The Stop and commit hooks run both. | [rustup](https://rustup.rs), then `rustup component add rustfmt clippy` |
| [cargo-nextest](https://nexte.st) | The test runner. The commit hook runs `cargo nextest run`, and so should you. | `cargo install cargo-nextest --locked`, or a [prebuilt binary](https://nexte.st/docs/installation/pre-built-binaries/) |
| [jq](https://jqlang.org) | The commit hook reads the Bash command from JSON on stdin. Without jq, commits go unchecked and every Bash call reports a hook error. | `apt install jq` / `brew install jq` / `winget install jqlang.jq` |
| bash and git | The hook script is bash and calls `git status` and `git diff`. On Windows, use Git Bash or WSL. | Usually already present |
| tmux | Only for the `run-git-tailor` skill, which drives the TUI. | `apt install tmux` / `brew install tmux` |

Needed for some tasks:

| Tool | Why | Install |
|------|-----|---------|
| [cargo-deny](https://embarkstudios.github.io/cargo-deny/) | License and advisory checks against `deny.toml`. CI runs `check licenses`, and the release checklist runs `cargo deny check`. Needed when adding or bumping dependencies. | `cargo install cargo-deny --locked` |
| The MSRV toolchain | `rust-version` in `Cargo.toml` (currently 1.89). Check the library and binary with `cargo +1.89 check`; tests may need a newer toolchain. | `rustup toolchain install 1.89` |
| [gh](https://cli.github.com) | Pull requests, CI status, and triggering `release.yml`. | `apt install gh` / `brew install gh` |
| docker | Only for rendering the demo GIFs (`demo/build.sh`). The image bundles vhs, ffmpeg and the rest. | See `demo/README.md` |

## The hooks

`hooks/rust-gate.sh` runs in two places:

- **Stop:** when Claude ends a turn with uncommitted Rust changes, it runs
  `cargo fmt` and `cargo clippy --all-targets -- -D warnings`. It won't let
  Claude stop until clippy is clean.
- **Before `git commit`:** fmt, clippy and `cargo nextest run`. A `test:` commit
  (a TDD red step) must have failing tests; every other commit must pass.

Review or disable them with `/hooks` in Claude Code.
