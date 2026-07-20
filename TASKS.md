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

## Interactivity — Split Commit
- [ ] T227 P2 feat - Add a "split out hunk" split option, mirroring T218's
  "split out file" at hunk granularity: peel one hunk out of a multi-hunk
  commit into its own commit while the rest stay together in the original
  commit's replacement. A bare list of file+line-range labels isn't enough to
  pick a hunk by — the user needs to see the actual code — so instead of a
  selection dialog, make the commit detail view's diff scrolling hunk-aware:
  add `hunk_start_lines: Vec<usize>` alongside the existing
  `file_start_lines` (`src/app/state.rs`, populated in
  `src/views/commit_detail.rs`'s `build_diff_lines`) and
  `jump_to_next_hunk`/`jump_to_prev_hunk` alongside
  `jump_to_next_file`/`jump_to_prev_file`, reusing the same rendered diff's
  scrolling, colors, and search. Add a key binding to split out the hunk
  currently in view directly from the detail view. Add the backend operation
  (`src/repo/git2_impl/split_op.rs`, reusing the existing hunk-application
  helpers in `hunks.rs`) executing as a two-commit split (chosen hunk first
  or otherwise consistently ordered, matching `split_commit_out_file`'s
  approach), and cover the new navigation + split flow with TUI tests plus
  repository tests in `tests/split_commit/`.

## Interactivity — Edit Commit
- [ ] T228 P2 feat - Add an "Edit" operation (interactive-rebase's `edit`
  verb): pause on the selected commit with its tree checked out — as if
  `git rebase -i` had stopped there — and drop the user into a shell to
  freely edit files, `git add`, and `git commit` (including splitting into
  an arbitrary number of commits with custom boundaries, e.g. via
  `git add -p`); when the shell exits, continue. Reuse
  `src/external_tool.rs::with_tui_suspended` (today used for `$EDITOR` and
  the mergetool) to suspend/restore the TUI, spawning `$SHELL` (falling back
  to a sensible default, e.g. `/bin/sh`, if unset) instead; show an on-screen
  message before suspending explaining what to do and that exiting the shell
  continues. On resume, detect the resulting commit chain from the original
  parent to the new HEAD and splice it in place of the original commit,
  replaying descendants — reuse the exact `finalize_split` /
  `rebase_descendants` machinery `split_commit_per_*` already uses in
  `src/repo/git2_impl/split_op.rs` (Edit is architecturally a Split whose
  pieces are user-authored rather than computed). Needs a validation step
  before splicing — confirm the resulting HEAD still descends from the
  expected parent commit — and a clear, safe abort path if the user leaves
  the repo in an unexpected state (checked out elsewhere, a merge commit,
  etc.), in the spirit of the existing interrupted-operation journal/recovery
  system; a no-op (shell exited with no changes) should behave as a
  cancelled operation, not a rewrite. Make the operation undo/redo-able like
  every other history-rewriting operation. Cover with repository tests
  (multi-commit output, no-op case, unexpected-state abort) and TUI tests for
  the suspend/resume flow.

## Interactivity — Squash Commit
- [ ] T229 P2 feat - Add bulk autosquash (mirrors `git rebase --autosquash`):
  a new action (not tied to a single selected commit) that scans the branch
  for `fixup!`/`squash!`-prefixed commits, matches each to the earlier commit
  whose summary line follows the prefix, and squashes/fixups each into its
  target in one bulk pass — bottom-up, respecting each target's position, so
  multiple fixups for the same target stack correctly. Reuse the existing
  squash/fixup backend (`src/repo/git2_impl/squash_op.rs`) as the primitive,
  looping it over the computed target pairing; show one confirmation dialog
  up front listing what will happen before running (the whole batch is a
  single undoable operation via the existing journal, like every other
  rewrite). Cover with repository tests (multiple fixups targeting the same
  commit, a fixup with no matching target, mixed fixup!/squash! prefixes)
  and a TUI test for the confirmation dialog.

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
