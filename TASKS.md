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
  — `StubRepo` becomes a `RepoRead`-only stub. Orthogonal to T240 (the *lower*
  `GitBackend` seam below `GitRepo`, split out of T222); this segregates the
  surface *above* it. Pure refactor, behavior-preserving.
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
- [X] T237 P3 refactor - Reduce view-layer duplication. **Five** near-duplicate
  scroll-into-view helpers (operation_select, split_select, split_files_select,
  split_hunks_select, and autofixup — the last with variable-height items) →
  one `ScrollState::ensure_visible(start, height)`. The `reverse` up/down
  mirroring is duplicated across three modules (commit_list.rs handle_key with
  eight copies, list_nav.rs with four, move_select.rs folding it into
  `up ^ reverse`) → mirror the *key* once via
  `KeyCommand::with_vertical_mirroring`, so handlers reason in one direction and
  the display order is resolved in a single testable place. `ScrollListUp`/
  `ScrollListDown` must stay unmirrored: they move the viewport in display space
  and are already visual. Also collapse move_select's four near-identical
  navigation arms into one, and single-source the paging math on
  `app::scroll::page_size`. None of this had any test coverage — commit_list's
  handler had never been sent a navigation key — so land characterization tests
  first and require them to pass unchanged across every refactor.

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
  canceled operation, not a rewrite. Make the operation undo/redo-able like
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

## Demo & Promo Video
- [X] T238 P2 human - Watch the promo video end to end with fresh eyes and
  tighten whatever grates (Flags: HUMAN TASK). Every scene has been checked
  against its own narration and timings, but the whole thing has never been
  judged as one piece by someone not holding the numbers in their head. Render
  with `demo/build.sh video`; the pacing levers and what each is worth are in
  `demo/promo/README.md`.
- [ ] T239 P2 human - Publish the promo video and link it from `README.md`
  (Flags: HUMAN TASK). Upload to YouTube, then link it as a **clickable
  thumbnail** — an image wrapped in a link. Do not embed `<video>` or an MP4:
  GitHub sanitises the tag out of rendered Markdown and crates.io ignores it, so
  an embed silently degrades to nothing on both.

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
