# TASKS Checklist

Guidelines:
- Each task line: `- [ ] T### P? category - Title (Flags: ...)`
- Priorities: P0 (urgent) → P3 (low).
- Categories: bug | feat | fix | idea | human.
- Flags (optional): CLARIFICATION, HUMAN INPUT, HUMAN TASK, DUPLICATE.
- Mark completion by [ ] → [X]. Keep changes atomic (one commit per task).
- Mark won't-do tasks by [ ] → [-] and add `WONT DO` to Flags.
- Completed tasks are archived in TASKS-COMPLETED.md.

## UNCATEGORIZED

## Architecture & Robustness
- [ ] T222 P3 feat - Support gix (gitoxide) as an alternative git backend behind
  a lower-level raw-git trait: investigate and lay the groundwork for building
  git-tailor against `gix` (pure-Rust, no libgit2/C dependency — simpler static
  builds, no C/OpenSSL build deps) instead of `git2`. The current `GitRepo`
  trait is high-level (it exposes whole operations like `drop_commit`,
  `squash_commits`, the cherry-pick chain, journal, and undo), so implementing it
  directly for a second backend would duplicate all the backend-agnostic
  orchestration (planning, cherry-pick replay, journal, undo). Instead introduce
  a *lower* trait — e.g. `GitBackend` / `RawGit` — capturing only the raw
  primitives the orchestration needs: open repo and expose `.git`/workdir paths;
  read HEAD and resolve refs; walk commits and read commit metadata; read trees
  and blobs; diff two trees; three-way merge / cherry-pick a commit in memory;
  apply a diff to a tree; create commits; create/update/delete refs (with reflog
  messages); read/write the index and conflict stages; checkout (incl. writing
  conflict markers); stash save/apply; read git config. Refactor today's
  `Git2Repo` so the orchestration in `git2_impl/*` (cherry-pick chain,
  drop/move/squash/split planning, journal, undo) depends only on this lower
  trait, and the two backends implement just the raw operations.
  Phased:
  * Spike: audit which git2 calls the orchestration relies on and whether gix has
    equivalents. The known risk is gix's maturity for tree merges / cherry-pick,
    `apply_to_tree`, the index/conflict-stage model, checkout-with-conflicts, and
    stash — if those are missing we may have to reimplement them in Rust or
    conclude gix is not yet viable (a valid outcome to document). Choose the
    trait-seam granularity (too low ⇒ we reimplement merge logic ourselves; too
    high ⇒ duplication).
  * Refactor: extract the `GitBackend` trait and move the raw git2 calls behind
    it, leaving the orchestration backend-agnostic. This has standalone value and
    de-risks the rest even before gix lands.
  * gix backend: implement `GitBackend` for gix behind a Cargo feature
    (`backend-git2` default vs `backend-gix`, likely mutually exclusive so a
    build pulls only one backend's deps; gix is MIT/Apache so `cargo deny` stays
    green, but it adds many crates — keep it behind the feature).
  * Parity tests: run the existing `tests/` integration suite against both
    backends (e.g. rstest `#[case]` per backend, or a feature-gated CI matrix) to
    prove identical end-state behaviour.
  Keep the `GitRepo` trait interface unchanged so the TUI, journal, and undo are
  unaffected regardless of backend.

## Build & CI
- [X] T118 P2 feat - Set up GitHub Releases with pre-built binaries: create
  `.github/workflows/release.yml` that triggers on version tags (`v*`), builds
  the `gt` binary for `x86_64-unknown-linux-musl` (fully static, covers WSL2 and
  all Linux distros), `x86_64-pc-windows-msvc` (Windows native), and optionally
  `aarch64-unknown-linux-gnu` and `aarch64-apple-darwin`; use
  `taiki-e/upload-rust-binary-action` to strip, archive, and attach binaries to
  the GitHub Release automatically; the musl target should produce a zero
  shared-library binary (add `RUSTFLAGS=-C target-feature=+crt-static` if
  needed) so no system libs beyond the kernel are required
