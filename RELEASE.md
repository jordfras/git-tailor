# Release Checklist

Steps to follow when cutting a new release of git-tailor.

> **Tip:** `./release.sh <version>` automates the mechanical parts below — it
> sets the `Cargo.toml` version, stamps the changelog with the version and date,
> and regenerates the screenshot (§2) and demo GIF, reporting whether each
> changed. It does not commit, tag or publish. Requires Docker (for the GIF).

## 1. Changelog

- All user-visible changes since the last release are documented in
  `CHANGELOG.md` under `[Unreleased]`.
- Change `[Unreleased]` to the new version and today's date.

## 2. README

- Update `README.md` if any new CLI flags, key bindings, or significant features
  were added.
- Regenerate the TUI screenshot so it reflects the current rendering:
  `cargo run --example gen_screenshot` (writes `doc/tui_example.png`). Commit the
  result if it changed.
- Regenerate the demo GIF so it reflects the current UI and key bindings:
  `demo/build.sh publish` (renders the tape in Docker and writes `doc/demo.gif`).
  Commit the result if it changed. Requires Docker.

## 3. Cargo.toml metadata

- Bump `version` according to semantic versioning.
- Review `description`, `keywords`, and `categories` — adjust if the tool's
  scope has changed.
- Confirm the `exclude` list still covers all dev/internal files (AGENTS.md,
  TASKS.md, examples, etc.) and nothing important is missing from the published
  crate.

## 4. Dependency hygiene

- Run `cargo deny check` — no license violations, no known CVEs.
- Run `cargo update` and review the diff; update `Cargo.lock` if sensible.

## 5. CI

- Merge the release branch/PR to `main`.
- Confirm all CI jobs pass on `main` (format, clippy, licenses, build, tests).

## 6. Manual testing — Linux / WSL

- Launch `gt` on a real repo; browse commits, scroll, search with `/`.
- Reorder two commits.
- Squash two commits; verify combined message.
- Fixup a commit (silent squash).
- Drop a commit.
- Reword a commit message.
- Split a commit (per-file and per-hunk).
- Trigger a rebase conflict; resolve via mergetool; verify result.
- Test `gt --all` on a repo with no upstream.
- Test `gt <branch>` with an explicit base.

## 7. Manual testing — Windows

- Repeat the smoke tests above in a native Windows terminal (PowerShell / cmd)
  and in WSL2.
- Confirm error messages stay visible long enough to read (key-release
  regression check).

## 8. Dry-run publish

- `cargo publish --dry-run` — confirm packaging succeeds and the file list looks
  correct.

## 9. Tag and publish

- `git tag v<version>` on the release commit.
- `git push origin main --tags`
- `cargo publish`
