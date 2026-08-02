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
- [X] T230 P2 refactor - Interface-segregate the `GitRepo` god trait (54 methods,
  `src/repo.rs`). Split it into focused traits: `RepoRead` (the 17 read/query
  methods) plus mutation traits (`SplitOps`, `SquashOps`, `RewriteOps` =
  drop/move/reword/edit, `RebaseOps`, `JournalOps`, `UndoOps`, `StagingOps`,
  `StashOps`), keeping a bundle `trait GitRepo: RepoRead + SplitOps + … {}` with a
  blanket impl so existing `&impl GitRepo` bounds keep compiling. `Git2Repo`'s impl
  is already a thin delegation layer, so the impl regroups rather than changes.
  Then narrow the read-only consumers (`loader.rs`, `views/commit_detail.rs`,
  `views/main_view.rs`, `editor.rs`) to `&impl RepoRead`, and shrink the test
  doubles: today 74 `unimplemented!()` stubs across `StubRepo`
  (`tests/common/fake.rs`, 49/54) and `MockRepo` (`src/dispatch/tests.rs`, 25/54)
  — `StubRepo` becomes a `RepoRead`-only stub. Orthogonal to T222 (which adds a
  *lower* `GitBackend` seam below `GitRepo`); this segregates the surface *above*
  it. Pure refactor, behaviour-preserving.
- [X] T231 P2 refactor - Factor repeated dispatch-handler scaffolding
  (`src/dispatch/*`). (a) The `autostash_save()`-guard block is copied verbatim 8×
  (commit_ops.rs, split.rs, edit.rs, autofixup.rs) → one helper. (b) The "suspend
  TUI + `$EDITOR` on a message + empty/unchanged match" appears 5× (commit_ops.rs
  commit-staged/reword/squash, conflict.rs squash-continue, autofixup.rs edit
  message) → a helper returning an `EditedMessage { Text | Empty | Unchanged }`.
  (c) `handle_run_mergetool` / `handle_run_editor` / `handle_run_stash_tool`
  (conflict.rs) are three near-identical "suspend → run tool → refresh
  conflicting-files → rebuild conflict-state → banner" flows (the stash one is
  already the merged `use_mergetool: bool` shape) → one `run_conflict_tool`
  parameterized by the tool closure and target-state builder. (d) drop/move
  handlers are line-for-line identical but the git call + labels → a shared
  wrapper. Pure refactor; MockRepo dispatch tests already cover these paths.
- [X] T232 P2 refactor - Factor the `cherry_pick_chain` "finish" wrappers
  (`src/repo/git2_impl/*`). The Complete/Conflict result match is inlined 6×
  (drop_op.rs:57, move_op.rs:79, cherry_pick.rs:258, squash_op.rs:318,
  conflict.rs:79, edit_op.rs:155); squash already extracted `replay_and_advance` —
  generalize it to `advance_and_finish(repo, chain_result, checkout_target,
  log_msg)` and route the other five through it. Also collapse the 3× `ConflictState`
  construction (cherry_pick.rs:167/225, squash_op.rs:281) into one builder, and the
  3× `revwalk push→collect→reverse` idiom (drop_op.rs:75, move_op.rs:101/155) and
  4× empty-tree build into small helpers. Pure refactor; covered by existing
  integration tests.
- [X] T233 P3 refactor - Replace the `ConflictState` fat union with honest per-op
  state (`src/repo.rs:103`). It carries the common conflict fields plus four
  op-specific optional payloads (`moved_commit_oid`, `squash_context`,
  `autofixup_context`, `edit_context`) + an `is_orphan_root` flag, with consumers
  branching on which is `Some`; it is also abused by `begin_edit` (edit_op.rs) to
  journal an in-progress edit that has *no conflict*. Move toward an
  enum-of-contexts and separate the "in-progress journal record" from "conflict
  awaiting resolution". Touches journal serialization + crash recovery → do TDD
  against `tests/undo.rs` and the edit/recovery tests. Higher risk.
- [X] T234 P3 refactor - Break up the `AppState` god-struct (`src/app/state.rs`, 34
  flat fields). Extract the repeated `(offset, max, visible_height)` scroll state —
  detail vertical, detail horizontal, and every dialog — into a reusable
  `ScrollState`, and group the detail-view, search and status fields into
  sub-structs. The commit-list fields are *not* a third scroll-triple: there is no
  `max` (the bound comes from `commits.len()`), the offset is an `Option` override,
  and the effective offset also needs `commits`/`reverse`/`selection_index`. Group
  those by cohesion instead — all five together in a `CommitListState` that owns
  the navigation, the scroll override and the row queries — so each becomes a real
  method rather than one reaching across four fields. The two row helpers that also
  set an error message keep their signatures on `AppState`, which composes list +
  status. Move `pending_autofixup_selection` off `AppState` entirely (the one
  transient-per-op field that leaks into cross-cutting state). Separately, lift the
  self-contained ~10-function detail search subsystem out of
  `views/commit_detail.rs` (929 lines) into its own module. Pure refactor.
- [X] T235 P3 refactor - Unify the two descendant-replay engines. `reword_op.rs`
  and `split_op.rs` (`finalize_split`) use their own `rebase_descendants`
  (cherry_pick.rs:28), which duplicates the cherry-pick mechanics of the
  conflict-aware `cherry_pick_chain` (drop/move/squash/edit) and differs only in
  what it does with a conflict. Share the step, but keep the distinction: split
  and reword replay onto a commit whose tree is identical to the original's, so
  the merge takes *theirs* at every path and the result equals the descendant's
  own tree — inductively down the chain, a conflict is impossible. Give that path
  a return type with no conflict variant, so callers are never made to handle an
  impossible case, and have it bail without journaling, writing the working tree
  or moving a ref. Two preconditions: per-file split must pin its last piece to
  the original tree (the one strategy where that invariant is emergent rather
  than structural), and both operations must reject merge commits in the replay
  range, which make the descendant revwalk unreliable. Cover with tree-identity
  assertions — a descendant-conflict test is unconstructible.
- [-] T236 P3 refactor - Split the two grab-bag files in the git2 layer
  (`git2_impl/journal.rs`, `git2_impl/reads.rs`) if they keep growing.
  WON'T DO — the trigger never fired and the "grab bag" premise is wrong.
  Both files are stable: `reads.rs` has been flat for two months (491 → 544 →
  512 — it shrank), and `journal.rs` grew 200 → 811 lines in its first 11 days
  then only +48 in the five weeks since, the last +43 of that being T233 refactor
  churn rather than new responsibility. `journal.rs` is also not a grab bag but a
  single persisted document (`JournalDoc`, one `journal.json`) with accessors:
  11 of its 15 `pub(super)` functions open with `load_doc` and 10 close with
  `save`; the supposed five concerns are four *fields* of that one struct;
  `is_empty` deliberately couples their lifecycles (the file is deleted only when
  all are empty at once); and the gc-pins are not state but a pure function of
  `undo`+`redo`, recomputed on every save. Splitting it would mean exposing
  `JournalDoc` and all its fields plus `load_doc`/`write_doc`/`save`/`UndoRecord`
  — an encapsulated core turned into a module-wide API to make one file shorter.
  (The original inventory also missed the in-progress/crash-record group, the
  most externally called cluster at 14 sites.) `reads.rs` is 25 functions
  averaging 16 lines, cohesive by role and clustered around shared private
  helpers that a split would cut across module boundaries.
  Re-open only if either file gains a genuinely independent concern — one with
  its own lifecycle, not another field of `JournalDoc`. Not on line count.
- [ ] T237 P3 refactor - Reduce view-layer duplication. Four near-duplicate
  `scroll_to_*` viewport helpers (operation_select.rs:90, split_select.rs:80,
  split_files_select.rs:165, split_hunks_select.rs:165) → one shared helper. The
  `reverse` up/down mirroring is duplicated across three modules (commit_list.rs
  handle_key, list_nav.rs, move_select.rs); `move_select.rs` reimplements
  `list_nav` navigation by hand (justified by its insertion-separator semantics, so
  extract the shared paging/mirroring math it can reuse). Pure refactor.

## Interactivity — Split Commit
- [X] T227 P2 feat - Add a "split out hunk(s)" split option, mirroring T218's
  "split out file" at hunk granularity: peel one or more selected hunks
  (possibly across several files) out of a commit into their own commit while
  the rest stay together in the original commit's replacement. Selected from
  the split-strategy picker like every other strategy; since picking hunks
  needs the user to see the code (a bare file+line-range label isn't enough),
  confirming it opens a dedicated wide two-pane dialog
  (`AppMode::SplitHunksSelect`, `src/views/split_hunks_select.rs`) — a
  scrollable list of the commit's hunks (file path + old-side line range) on
  the left, a colored diff preview of the highlighted hunk on the right,
  mirroring how the main window splits the commit list from the detail view.
  `↑`/`↓` move the cursor, `v` toggle-selects the hunk in view, `Enter` splits
  out the marked hunks (falling back to just the hunk under the cursor when
  nothing is explicitly marked), `Esc` cancels. The backend operation
  (`GitRepo::split_commit_out_hunks`, `src/repo/git2_impl/split_op.rs`,
  reusing the existing hunk-application helpers in `hunks.rs`) identifies
  hunks as `(delta_idx, hunk_idx)` against the diff at a fixed context level
  (`repo::DEFAULT_CONTEXT_LINES`) — the same level the picker itself loads the
  commit's diff at, via `HunkPickerEntry` (`src/app.rs`), keeping the two
  consistent without needing a separate zero-context diff. Executes as a
  two-commit split via the existing "two-tree trick" (`split_commit_out_file`'s
  approach). Covered by repository tests in `tests/split_commit/out_hunks.rs`
  and TUI `handle_key`/snapshot tests in `tests/tui_split_hunks_select.rs`.

## Interactivity — Edit Commit
- [X] T228 P2 feat - Add an "Edit" operation (interactive-rebase's `edit`
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
- [X] T229 P2 feat - Add bulk "Autofixup" (mirrors `git rebase --autosquash`):
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
