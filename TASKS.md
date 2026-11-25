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

## Conflict Resolution
- [ ] T284 P2 bug - The built-in kdiff3 merge command lacks `--auto`, so kdiff3
  opens its window even for conflicts it resolves on its own. Git's wrapper
  (`mergetools/kdiff3`) passes it: kdiff3 then resolves what it can, saves and
  exits without a window. Checked with `kdiff3 --auto` run headless: it resolves
  edits to adjacent lines, and the same edit on both sides where one side also
  changed the next line, without a window; git and libgit2 conflict on both.
  Fix in `builtin_cmd` (`src/mergetool.rs`).
  Related: `run_tool_for_file` stages the file whenever the tool exits 0, without
  checking for leftover conflict markers. Git distrusts the exit code of tools
  without `trustExitCode` and asks instead. Check `<<<<<<<`/`>>>>>>>` lines in
  `$MERGED` before staging, and leave the file conflicted if any remain.
- [ ] T285 P2 feat - Offer to resolve trivial conflicts in the conflict dialog,
  one proposal at a time, before (or instead of) the merge tool. Git and libgit2
  conflict on changes that merely touch; in git-tailor that is the common case:
  a fold into a commit whose neighbor edited the next line.
  **Finding the conflicts:** run `merge_file` on stages 1/2/3 with diff3 style
  and a long `marker_size` (so file contents cannot be mistaken for markers),
  then parse the output into clean and conflicting regions. Only libgit2's
  conflicts are candidates, so this can only ever narrow them.
  **Refining each conflict** (a pure function over the three line lists, unit
  tested with no git), by line-diffing base→ours and base→theirs inside the
  region:
  * No base line is changed differently by both sides, and the two sides do not
    insert at the same point: apply both. This covers adjacent edits, and the
    same edit on both sides where one side also changed the next line. It is
    what a 3-way merge already does when one unchanged line separates the edits.
  * Both sides insert at the same point and change nothing else (imports,
    changelog bullets, enum variants): propose ours-then-theirs or
    theirs-then-ours, with the user picking. Never automatic.
  * Anything else stays a conflict. A whitespace-only change on one side is a
    candidate for later, not for this task.
  **UI:** from `RebaseConflict` (and `StashConflict`), a key such as `a` steps
  through the proposals. Each shows base, ours and theirs, and the proposed
  result; keys accept, skip, or pick the order for an insert-insert. A file is
  written and staged only when every conflict in it was accepted. A file with
  anything skipped or unresolvable is left exactly as it was for the merge tool,
  because 3-way tools read the index stages, not `$MERGED`, and would drop a
  partial resolution. Afterwards the dialog shows what is left, and the merge
  tool is the next step as now.
  **Open questions:**
  * The line diff: `git2::Patch::from_buffers` (no new dependency) or a small
    LCS over the region, which is short.
  * Whether "apply both" for clean adjacent edits should also need a key, or
    only be shown. Start with a key; relax it if it proves tedious.
  Alternatives for users meanwhile: git-mediate and mergiraf (`mergiraf solve`)
  resolve trivial conflicts from the terminal, without asking. A built-in 3-way
  editor is out of scope: `nvimdiff3`/`vimdiff3` already cover the terminal.
