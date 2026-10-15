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
- [ ] T263 P1 bug - A journal that cannot be read is overwritten by the next
  thing that writes one, instead of being left alone. Eleven mutating helpers in
  `journal.rs` start from `load_doc(repo).unwrap_or_default()`, so a read or
  parse failure is indistinguishable from an empty document: the mutation lands
  on `JournalDoc::default()` and `save` writes that over the file. When the
  mutation is a *clear* — `set_worktree_source(None)`, `set_autostash(None)` —
  the result is empty, and `save` removes the journal outright.
  The loss: a journal holding an `autostash` record and a paused `in_progress`
  becomes unreadable, the next operation clears one field, and the stash naming
  the user's uncommitted changes is no longer referenced by anything. Nothing
  reapplies it and nothing says it is there.
  Reachable without exotic failures, and T242 made it the *remaining* way to
  lose work: startup reports a corrupt journal and carries on
  (`recovery.rs`, "Ignoring unreadable operation journal"), and the first
  operation after that warning is what destroys it. T242's own premise — that a
  corrupt-but-present journal "comes back as `JournalStatus::Corrupt` and tells
  the user" — is only true until something writes. Nor does it take
  corruption: on Windows, a scanner holding the file for longer than the read
  retries' 255 ms makes the read fail just the same.
  The decision this needs, which is why it is filed rather than fixed: a
  rewrite cannot proceed on a journal it cannot read, but refusing every
  operation until `--clean-journal` is run is a hard stop for what may be one
  stale byte. Likely shape: propagate the error from the mutating helpers
  (`load_doc(repo)?`) so the operation refuses and names the journal, and let
  `--clean-journal` remain the way out — but check what refuses *before* a
  rewrite starts, so a refusal lands where the user can still act on it rather
  than half-way through.
- [ ] T264 P2 fix - Decide whether libgit2 should flush what it writes. T242
  made the journal durable, but the objects and ref moves of a rewrite are
  still written without a flush, so a power cut can leave the journal naming a
  tip the refs never reached, or a ref naming an object that never reached the
  disk. The second is the one that loses work. libgit2 has
  `GIT_OPT_ENABLE_FSYNC_GITDIR` for it, which git2 does not wrap: turning it on
  means an unsafe call through `libgit2-sys`. It flushes every object, so
  measure a large split before and after; libgit2 also honors
  `core.fsyncObjectFiles` per repository, which may be enough to document.
- [ ] T222 P3 feat - Spike only (timebox: one day): decide whether `gix`
  (gitoxide) can back the rewrite engine — pure Rust, no libgit2/C dependency,
  simpler static builds — then close this task either way. Narrowed 2026-09-12
  from "build a second backend" after auditing the coupling; the seam refactor it
  used to bundle is now T240, which has to justify itself on its own.
  The one open question: **can gix produce an index carrying stage 1/2/3 conflict
  entries from a tree merge, and can that index be checked out with conflict
  markers?** `ConflictState`, `rebase_continue`/`rebase_abort` and the whole
  conflict dialog are shaped around libgit2 handing us exactly that
  (`cherrypick_commit` ×4 → conflicted `git2::Index` → `checkout_index` ×3 writes
  the markers). gix's crate-status calls tree merge done but describes the stage
  index as "a way to generate an index with stages, mostly conforming with Git" —
  "mostly" is the whole question. If we would have to build the stage index and
  write the markers ourselves, that is us reimplementing the merge surface, and
  the answer is no.
  What the audit already settled, so the spike need not redo it:
  * The two presumed blockers are gone: gix now claims stash (save/apply/pop with
    conflict handling) and `git apply`-compatible patch application, covering
    `stash_save2`/`stash_apply` (×6) and `apply_to_tree` (×6, the split/hunk
    peeling). Verify these, don't re-investigate them.
  * The coupling is API-shaped, not behavior-shaped, and already contained: all
    325 `git2::` references in `src/` (161 of them `git2::Oid`) sit inside
    `src/repo/git2_impl/`, across 14 files, with zero leakage into the rest of
    the crate. Mechanical.
  * The hard-won safety work is ours, not libgit2's, and ports as-is:
    `refuse_*_collisions`, `refuse_if_branch_moved`, the session lock,
    journal/undo/gc-pins and the shallow-graft guard are rules we invented over
    git primitives.
  * The byte-exactness work gets *easier*: gix is bytes-native, so
    `commit_preserving_message`'s hand-spliced `encoding` header, the
    `&str`→`&Path` `read_index_stage` signature and `substitute_vars` exist only
    to route around git2 insisting on `&str`.
  * Genuinely libgit2-shaped, so re-derived rather than ported:
    `remove_dropped_files`/`remove_written_path` (it exists because libgit2's
    checkout leaves now-absent paths behind and `remove_untracked` is unscoped)
    and the marker-writing checkout above.
  * `tests/` holds 173 `git2::` references across 28 files building fixtures, so
    a parity suite keeps a git2 dev-dependency even once the binary sheds it.
  Outcome is a decision recorded here: either a follow-up task to implement the
  gix backend behind a Cargo feature (`backend-git2` default vs `backend-gix`,
  mutually exclusive) with the existing `tests/` suite run against both for
  parity, or `[-]` WON'T DO with the evidence.
- [ ] T240 P3 refactor - Extract a `GitBackend` seam under `GitRepo`, separating
  the backend-agnostic orchestration from the raw git calls. `GitRepo` is
  high-level (whole operations: `drop_commit`, `squash_commits`, the cherry-pick
  chain, journal, undo), so a second backend implementing it directly would
  duplicate all the planning/replay/journal/undo logic. A lower trait would
  capture only the primitives that orchestration needs: open repo and expose
  `.git`/workdir paths; read HEAD and resolve refs; walk commits and read commit
  metadata; read trees and blobs; diff two trees; three-way merge / cherry-pick
  in memory; apply a diff to a tree; create commits; create/update/delete refs
  with reflog messages; read/write the index and its conflict stages; checkout
  (incl. writing conflict markers); stash save/apply; read config. Granularity is
  the whole design problem: too low and we reimplement merge logic ourselves, too
  high and the backends duplicate orchestration.
  Gated, not queued. Split out of T222 so it stops riding on a gix decision it
  does not depend on — but it does not obviously pay for itself either: it is a
  mechanical rewrite of 325 call sites through the most safety-critical code in
  the project, with no user-visible benefit, immediately after that code was
  hardened by a long run of conflict/byte-safety fixes. Its honest standalone
  claim is that it would make the orchestration unit-testable without temp repos.
  Do this only if T222 concludes gix is viable, or if that testability argument
  becomes load-bearing on its own. Otherwise leave it: the seam is cheap to add
  later precisely because the git2 code is already confined to one directory.
- [X] T243 P3 fix - Recognize a leftover fold commit before rewriting over it.
  A fold moves the branch onto a temporary "git-tailor: working-tree changes"
  commit and parks the other row in a stash. If its journal record goes before
  the fold finishes, the branch keeps that commit and the tree is clean, so the
  next rewrite treats the commit as the user's own. Nothing is lost: discarding
  the record first keeps the whole working tree in a rescue ref and drops the
  stash, or leaves the stash if keeping it failed.
  Only two paths get there, both after a crash. Startup unwinds an interrupted
  fold and offers a paused one for resuming; it discards the record only when
  the branch has moved since (`discard_stale`), e.g. a `git commit` on top of
  the temporary commit. Or `--clean-journal` with a fold interrupted or
  paused, which leaves the branch on the temporary commit. Auto-stash cannot
  produce a synthetic commit, and startup restores its leftover record itself.
  Done without an ownership model for parked work: every rewrite, a fold's
  included before it lifts, refuses one whose range holds a commit with exactly
  the temporary message, unless it is the fold in flight's own and the
  operation starts from it, or the rewrite drops that commit. The refusal says
  to drop it and points at `gt --list-rescued`, rather than giving a reset
  recipe. No warning when the list loads.
- [ ] T246 P3 idea - Decide whether git-tailor should say which base it picked.
  The default range is HEAD back to the merge-base with the upstream default
  branch, auto-detected via `origin/HEAD` and falling back to `main` when that
  ref is not set. A missing or wrong `origin/HEAD` makes the visible range
  wrong, so an operation can span more history than the user believes it does.
  Not a mechanical bug — the code does what it documents — but a "what does the
  user think they are operating on" question, which is the kind that makes a
  correct rewrite feel like a destructive one.
  Scope: decide whether the chosen base and how it was found belong on screen,
  and whether an unresolvable `origin/HEAD` should be surfaced rather than
  silently falling back to `main`.
- [ ] T260 P3 idea - Spike only (timebox: one day): can the library back a VS
  Code extension through a napi-rs native module, with the fragmap matrix shown
  next to the commit list in a webview? Close this task either way with a
  decision recorded here.
  Why napi-rs: no server process to manage, and N-API is ABI-stable, so one
  build per platform runs in VS Code's Electron without rebuilding per Node
  version. VS Code supports platform-specific `.vsix` packages, and napi-rs
  ships GitHub Actions templates for the build matrix. A `gt serve` JSON-RPC
  mode was the alternative; WASM was ruled out (libgit2 and filesystem access).
  What a quick look already found, so the spike need not redo it:
  * `repo` and `fragmap` do not depend on ratatui/crossterm. The one leak from
    core into the TUI layer is `app::SquashMode`, used by `squash_op.rs`,
    `autofixup_op.rs` and `autofixup.rs`; it belongs in `domain`.
  * `lib.rs` also exports `app` and `views`, so a napi crate linking the library
    today drags in ratatui. Likely shape: a Cargo workspace with a core crate
    (domain, repo, fragmap, autofixup), the TUI crate, and a napi crate. The
    library is not a public API, so nothing constrains the split.
  * `FragMap` (`commits`, `clusters`, `matrix: Vec<Vec<TouchKind>>`) maps almost
    directly onto a webview grid. Convert to `#[napi(object)]` DTOs in the napi
    crate rather than making core types JS-aware.
  * `build_fragmap`'s interruptible progress callback fits a napi `AsyncTask`,
    so a large branch neither blocks the extension host nor runs past a cancel.
  The real cost sits outside the library: the orchestration in the binary's
  `dispatch/` and `loader.rs` (autostash around operations, conflict
  continue/abort, undo/redo, diff loading) is tied to `AppAction`/`AppState`.
  The spike should judge whether it can lift into a UI-agnostic session layer
  in the library, which the TUI would use too, rather than be rewritten in
  TypeScript. Conflicts could go to VS Code's merge editor instead of mergetool.
  Spike deliverable: `SquashMode` moved, the workspace split, and a napi
  function returning the fragmap for HEAD, rendered as a matrix beside the
  commit list in a minimal webview. Outcome is either follow-up tasks for the
  extension or `[-]` WON'T DO with the evidence.

## Build & CI
- [ ] T241 P3 feat - Publish a Homebrew formula from a custom tap, updated
  automatically on each `v*` tag, so `brew install` works for people without a
  Rust toolchain. Create `jordfras/homebrew-tap` (the `homebrew-` prefix is what
  makes the short form resolve), giving users
  `brew install jordfras/tap/git-tailor`.
  Not homebrew-core, deliberately: core has a notability bar (roughly 75 stars /
  30 forks / 30 watchers, and this repo is at 3 / 0 / 0) and its formulae are
  bumped by PR, so "publish on every tag" is not a thing it does. Revisit only
  if the project ever clears that bar.
  A binary formula, not build-from-source — the release already produces exactly
  the assets it needs, with `.sha256` files alongside them:
  * `on_macos` is a single block: `universal-apple-darwin` covers Apple Silicon
    and Intel in one asset, so there is no `on_arm`/`on_intel` split to write.
  * `on_linux` needs both arches now that the release matrix builds
    `aarch64-unknown-linux-musl` as well as x86_64.
  * `def install` is just `bin.install "gt"`; the `test do` block can assert
    `gt --version`.
  Automation is a third job in `.github/workflows/release.yml`, after
  `upload-assets`: download the `.sha256` files from the release, render
  `git-tailor.rb` from a template, commit it to the tap repo. **The one real
  setup cost**: `GITHUB_TOKEN` is scoped to this repository, so pushing to a
  second repo needs a fine-grained PAT (contents-write on the tap) stored as a
  secret here.
  Considered and rejected: `cargo-dist` does tap publishing too, but adopting it
  means replacing the working `taiki-e` pipeline wholesale to gain one formula
  file. Also considered: self-tapping this repo via the two-argument
  `brew tap <user>/<name> <URL>` form, which works on any repo name and needs no
  second repo and no PAT — but leaves users a permanent two-command install
  instead of a one-liner. Worth falling back to if the PAT is the sticking
  point; if so, put the formula in `Formula/` (searched before
  `HomebrewFormula/`, then the repo root) and add it to `Cargo.toml`'s `exclude`
  so it does not ship inside the published crate.
  The Linux half of the formula can be tested locally — Homebrew is already
  installed on the dev machine.
