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

## 3. Videos

The promo and the four tutorials live in
[one YouTube playlist](https://www.youtube.com/playlist?list=PLbaTpyhikKGE).
They are *not* part of every release: re-record only when a release changes what
is on screen or what the narration claims — key bindings, dialog text, the
matrix rendering, the operations themselves. The tutorials are concept-led and
outlive most releases.

When a re-record is needed:

- Render one video at a time, unpiped:
  `demo/build.sh video promo`, `demo/build.sh video tutorials/01-matrix`, and so
  on. They share `demo/out/narration/`, so two concurrent renders clobber each
  other's audio. A single chapter can be re-rendered on its own with
  `demo/build.sh video tutorials/01-matrix 07-close`, which writes a
  `*-preview.mp4` rather than the full video.
- Confirm the render actually finished: the `>> done: ...` line and the output
  file's timestamp. A piped build reports the pipe's exit status, not the
  build's, so a failure can look like a success.
- Check claims against extracted frames, not against the tape and not against
  `--static`. Every fault worth catching here has been one the tape described
  correctly and the picture did not.
- Watch the per-scene table the render prints. `tape` far under `voice` is a
  frozen frame at the end of a chapter; far over it is dead air.
- Each tutorial's `README.md` in `demo/tutorials/` records what its fixture has
  to keep producing. Those invariants are the reason the scenes demonstrate
  anything; changing `make-repo.sh` without reading them is how a chapter ends
  up showing nothing.

Publishing an updated video:

- YouTube cannot replace the file behind an existing video, so a re-record is
  always a new upload with a new address.
- Add the new upload to the playlist in the old one's position, then remove the
  old one from the playlist and set it **unlisted** rather than deleting it —
  links in old issues, release notes and posts keep working.
- Pin a comment on the retired video pointing at the replacement.
- Keep version numbers out of titles and narration; put "recorded against
  git-tailor <version>" in the description instead, so a video only needs
  re-recording when the interface actually changes.
- Chapter timestamps for the description come from the render's own scene table
  (`07-recover ... @ 215.524s`).
- Link the playlist, never an individual video: `README.md` ships inside the
  published crate, and every past version of it stays on crates.io forever.

## 4. Cargo.toml metadata

- Bump `version` according to semantic versioning.
- Review `description`, `keywords`, and `categories` — adjust if the tool's
  scope has changed.
- Confirm the `exclude` list still covers all dev/internal files (AGENTS.md,
  TASKS.md, examples, etc.) and nothing important is missing from the published
  crate.

## 5. Dependency hygiene

- Run `cargo deny check` — no license violations, no known CVEs.
- Run `cargo update` and review the diff; update `Cargo.lock` if sensible.

## 6. CI

- Merge the release branch/PR to `main`.
- Confirm all CI jobs pass on `main` (format, clippy, licenses, build, tests).

## 7. Manual testing — Linux / WSL

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

## 8. Manual testing — Windows

- Repeat the smoke tests above in a native Windows terminal (PowerShell / cmd)
  and in WSL2.
- Confirm error messages stay visible long enough to read (key-release
  regression check).

## 9. Dry-run publish

- `cargo publish --dry-run` — confirm packaging succeeds and the file list looks
  correct.

## 10. Tag and publish

- `git tag v<version>` on the release commit.
- `git push origin main --tags`
- `cargo publish`

Pushing the `v<version>` tag triggers the **Release** workflow
(`.github/workflows/release.yml`), which creates the GitHub Release (notes from
`CHANGELOG.md`) and attaches pre-built `gt` binaries + SHA-256 checksums for
Linux (x86_64 musl, static), Windows (x86_64), and macOS (arm64 and x86_64).

To (re)build binaries for a tag that already exists — without moving the tag —
run the workflow manually:

```sh
gh workflow run release.yml -f tag=v<version>
```
