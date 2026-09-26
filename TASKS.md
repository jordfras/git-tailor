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
- [ ] T242 P1 fix - Make the journal durable: `write_doc`
  (`src/repo/git2_impl/journal.rs`) renames a temp file into place, which is
  atomic, but nothing is fsynced — not the temp file before the rename, not the
  containing directory after it. A power cut or kernel panic between the journal
  write and the ref move can leave the two disagreeing: a journal naming an
  operation the refs do not reflect, or refs that moved with no record saying
  what to undo. **The one open item that can still lose work** — everything the
  journal protects (undo/redo, the in-progress record that recovers a paused
  conflict, the auto-stash record naming a stash) depends on it being on disk
  when the process dies.
  Note which failure is worse. A journal that is **corrupt but present** comes
  back as `JournalStatus::Corrupt` and tells the user. A journal that is
  **missing** comes back from `load_doc` as `JournalDoc::default()` — "nothing
  was in progress" — so the refs have moved and nothing records what to undo.
  The silent one is the one to design for.
  Scope, in portability order:
  * `sync_all()` on the temp file before the rename. Portable — `fsync` on Unix,
    `FlushFileBuffers` on Windows — and it is what stops a half-written journal
    becoming visible.
  * fsync the containing directory after the rename, under `#[cfg(unix)]`. This
    is what makes the rename itself durable, and it has no portable form: a
    directory cannot be opened by `std::fs::File::open` on Windows at all
    (`CreateFile` needs `FILE_FLAG_BACKUP_SEMANTICS`, which std does not set).
  * Decide `F_FULLFSYNC` on macOS deliberately rather than by accident: plain
    `fsync` there does not flush the drive's own cache, so `sync_all()` means
    something weaker on macOS than on Linux.
  * Do **not** add a `windows-sys` dependency for `MOVEFILE_WRITE_THROUGH` (the
    Windows equivalent of the directory fsync, which `std::fs::rename` does not
    expose) on spec. Record the gap and revisit only with evidence.
  **Handle the Windows rename failure as part of this task**, not as a
  follow-up. `std::fs::rename` over an existing file fails on Windows when
  anything holds a handle to the destination, and antivirus and the search
  indexer take transient handles constantly. This is far more likely in practice
  than power loss, and nobody will report it — it surfaces as an occasional
  "failed to finalize journal" that looks like a fluke. The session lock keeps
  another git-tailor out; it does nothing about a scanner.
  Retry the rename with a short backoff, `#[cfg(windows)]`. Match on
  `raw_os_error()` — `ERROR_ACCESS_DENIED` (5) and `ERROR_SHARING_VIOLATION`
  (32) — rather than on `io::ErrorKind`, which does not distinguish these
  reliably across Rust versions. Cap the total wait low enough that a genuine
  permission error still fails promptly rather than hanging the TUI.
  **Measure the fsync cost first** — it sits on the path of every operation, and
  a sync per write may be noticeable on spinning disks or a network filesystem.
  If so, restrict it to the writes that immediately precede a ref move.
  Testing is the hard part and is honest to state: a crash between two writes is
  not reachable in-process. The `#[cfg(windows)]` retry will at least be
  compiled, linted and run now that CI covers all three targets (T247), which it
  would not have been before — but CI cannot manufacture a scanner holding a
  handle, so the retry's own behavior needs the helper exercised with an
  injected error. The rest is an audit that every write preceding a ref move is
  synced, recorded in the commit message.
- [ ] T243 P2 fix - Decide whether the dirty-state guard should know about
  *parked* work. `check_no_dirty_state` (`src/repo/git2_impl.rs`) refuses a
  rewrite when the tree has staged or unstaged changes. It no longer exempts a
  fold in flight — that was `covers_working_tree`, removed once the fold began
  setting the other row aside in the stash, because the tree a lift leaves is
  then genuinely clean.
  But "clean" has become ambiguous: it can mean nothing is uncommitted, or that
  the uncommitted work is parked in a stash nobody is finishing. Clear the
  journal mid-fold (`--clean-journal`, or startup discarding a stale record) and
  the branch is left on a temporary commit with a stash still recorded, and a
  rewrite proceeds over it. Nothing is lost — `discard_in_flight` deliberately
  spares the auto-stash record — but the rewrite runs on a history containing a
  synthetic commit the user never made.
  The same shape has always been true for `--autostash`.
  The naive fix (refuse whenever an auto-stash record exists) breaks
  `--autostash` outright, because its flow is save-then-operate and the guard
  would fire on its own stash. A correct version needs a notion of *which
  operation owns the parked work* — which is what `covers_working_tree` supplied
  for the fold before it was deleted.
  Decide first whether this deserves a mechanism at all: the parked work is
  recorded and recoverable either way, so this is about not surprising the user
  rather than about losing anything.
- [ ] T244 P2 idea - Decide whether `--autostash` should invert to an opt-out
  `--strict` (Flags: HUMAN INPUT). The question that prompted the whole
  uncommitted-work-safety round, still unanswered. Squash/fixup on the Staged
  and Unstaged rows works on a dirty tree with no flag, while every other
  history rewrite refuses unless `--autostash` is passed.
  It is a cleaner decision than when it was first raised: both paths now park
  work the same way — a stash, tracked changes only, untracked files left where
  the user put them — so this is a choice about one behavior rather than a
  reconciliation of two. A product decision about defaults, not a mechanical
  one, which is why it carries HUMAN INPUT.
- [ ] T245 P3 bug - Work out what grafts and `refs/replace` do to the
  rewrite engine. 3.1.0 fixed a shallow clone's graft boundary being mistaken
  for a true root — rewriting it built a parentless commit and cut the branch
  off from everything upstream, and pushed, it would truncate shared history.
  Grafts (`.git/info/grafts`) and `refs/replace` have the same shape: a commit
  whose parentage is not what the object says. But `is_shallow()` does not
  report them, and libgit2's replace handling differs from git's own.
  Deliberately not chased at the time, because a guard written without
  understanding that difference would be guessing.
  Scope: first establish what libgit2 actually does — does `parent_ids()` follow
  a replacement? — then decide whether a guard is warranted and what it refuses.
  The answer may be that nothing is needed, which is a fine outcome to record.
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
- [ ] T252 P2 fix - Bound the span-propagation graph's path enumeration.
  `spg_enumerate_paths` (`src/fragmap/spg.rs`) enumerates every path through the
  graph eagerly and recursively with no cap. Measured: 34 commits produce
  149,931 deduped clusters in 4.1s release; a 2,000-commit disjoint file takes
  104s. `assign_hunk_groups` hardwires its `poll` closure to `|| true`, so none
  of it is interruptible — the event loop is simply gone for the duration, with
  no way to cancel and no progress shown.
  Two halves, and the second is worth doing even if the first is hard: cap or
  restructure the enumeration (the consumer only needs cluster membership, not
  the paths themselves, so a reachability computation may replace the
  enumeration outright), and thread a real `poll` through so a user can abort.
  Not a patch — the enumeration is the algorithm, which is why this is filed
  rather than fixed.
- [ ] T253 P2 bug - Two files in one commit can collide onto one fragmap key.
  `collect_file_commits` (`src/fragmap.rs`) keys by canonical path, and merges
  hunks when the last entry for a key is the same commit. That merge exists for
  a file appearing twice in one commit — which only happens when a rename chain
  maps two *different* paths in the same commit to the same canonical name.
  Their hunks are then concatenated into one list whose line numbers refer to
  two different files, so it is out of order and the hunk-group assignments
  indexed off it are wrong.
  The fix needs a semantic decision rather than a patch: either keep the two
  files apart at that commit (losing the rename link there, since the canonical
  key is what carries it), or carry the source path alongside so entries can be
  distinguished without collapsing. Both change what the matrix shows, which is
  why this is not a quiet fix.
- [ ] T255 P2 bug - A pure deletion belongs to no fragmap column.
  `spg::SpgSpan::from_new_hunk` and `attribution::hunk_new_span` both return the
  **empty** interval `[new_start+1, new_start+1)` when `new_lines == 0`, while
  `assign_hunk_groups`'s column probe (`fragmap.rs`, `column_of`) measures the
  same hunk as `[new_start, new_start + max(new_lines, 1))` and requires
  `overlap > 0`. An empty span can never satisfy that, so a deletion-only hunk
  gets `column_of == None`.
  Concrete symptom: split a commit that deletes lines in two unrelated files
  where neither region is touched by a neighbour. Both hunks key on `(None, [])`
  and collapse into one hunk group — the merge the comment above `column_of`
  says must never happen.
  **Do not fix this in `extract_spans`.** That function is `#[cfg(test)]` and
  documented "(legacy) ... Kept for tests"; an earlier attempt changed it, added
  a passing test, and shipped nothing. Its whole test block covers code the
  binary does not run, which is worth cleaning up separately.
  The fix is in the two production span builders or in the probe, and it is a
  change to the span-propagation algorithm's core: an empty interval for a
  deletion may be load-bearing for propagation arithmetic, where a zero-width
  point is not. Establish that before changing it.
- [ ] T257 P3 bug - Autofixup identifies a target by its decoded summary.
  `AutofixupContext::message_overrides` is a `HashMap<String, BString>` keyed by
  the target's summary, and that summary is `CommitInfo::summary` — the lossy
  rendering built in `reads.rs` (`lossy(commit.summary_bytes())`). Two targets
  whose summaries differ only in bytes that do not decode render the same, so
  they collide: an override the user edited for one is written onto the other.
  `plan_autofixup`'s `fixup! <summary>` matching has the same exposure, and git
  itself does not — `--autosquash` matches subjects on their raw bytes.
  Same collision class as T250 and T251, but it did not go with them because
  the fix is not a retyping. **Keying on `Oid` is wrong** and the reason is
  recorded on the field: the OID is not stable across the batch's cascading
  rebases, which is precisely why the summary is the key. And the summary
  cannot simply become bytes — it is display data the whole TUI draws, searches
  and measures, and under the rule T249 settled on (decode at the render
  boundary, never before it) `CommitInfo::summary` staying a lossy `String` is
  correct.
  So this needs an identity for a target that is neither its OID nor its
  rendering: the summary's *bytes*, carried alongside the rendering through
  `AutofixupPair`, `AutofixupGroup` and the journal — a decision about what
  identifies a commit across a rebase, which is why it is filed rather than
  fixed.
  Narrow in practice: it needs two commits in range whose summaries differ only
  in undecodable bytes, and an edited override. The consequence is a message
  written to the wrong commit.
- [ ] T258 P3 bug - Splitting out a gitlink hunk probably fails.
  A submodule pointer change has one hunk ("Subproject commit …"), so the
  split-out-hunks picker offers it. `apply_selected_hunks_to_tree`
  (`src/repo/git2_impl/hunks.rs`) then looks the old side up as a blob, and a
  gitlink's id names a commit, not a blob. Unverified: reproduce first. Per-file
  writes gitlinks whole through `apply_whole_deltas_to_tree` and is not
  affected; per-hunk and per-hunk-group go through the same blob path and need
  checking too.

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
