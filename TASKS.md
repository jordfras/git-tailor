# TASKS Checklist

Guidelines:
- Each task line: `- [ ] T### P? category - Title (Flags: ...)`
- Priorities: P0 (urgent) → P3 (low).
- Categories: bug | feat | fix | idea | human.
- Flags (optional): CLARIFICATION, HUMAN INPUT, HUMAN TASK, DUPLICATE.
- Version flags (optional): V1, V2 etc. (used to group versions/releases).
- Mark completion by [ ] → [X]. Keep changes atomic (one commit per task).
- Completed tasks are archived in TASKS-COMPLETED.md.


## UNCATEGORIZED

## Core Behavior & Constraints (V4)
- [X] T081 P0 feat - Exclude the reference point (merge-base) commit from the
  commit list and all operations — it is shared with the target branch and must
  not be squashed, moved, or split (Flags: V4)

## Interactivity — Basic UI (V4)
- [X] T061 P0 feat - Change exit key from 'q' to Esc (Flags: V4)
- [X] T062 P1 feat - Add vertical separator line between title column and hunk
  groups column (Flags: V4)
- [X] T063 P1 feat - Add help dialog on 'h' key showing all interactive
  keybindings (q=quit, i=info, s=split, m=move, h=help) (Flags: V4)
- [X] T085 P2 feat - Add 'r' key to reload: re-read the commit list from HEAD
  down to the originally calculated reference point (merge-base), refreshing
  after external git operations without restarting the tool (Flags: V4)
- [X] T086 P2 feat - Show staged and unstaged working-tree changes as synthetic
  rows at the top of the commit list (above HEAD), displayed with distinct
  labels ("staged" / "unstaged") and included in the fragmap matrix so their
  hunk overlap with commits is visible (Flags: V4)

## Interactivity — Fragmap View (V4)
- [X] T082 P1 feat - Improve selected row highlighting in the hunk group matrix;
  the current inverse-color style is hard to read — use a subtler approach such
  as a bold/bright foreground, a dim background tint, or a side marker (Flags:
  V4)
- [X] T083 P2 feat - Add CLI flag `--no-dedup-columns` (or similar) to disable
  deduplication of identical hunk-group columns in the fragmap view, useful for
  debugging and understanding the raw cluster layout (Flags: V4)

## Interactivity — Commit Detail View (V4)
- [X] T064a P0 feat - Add DetailView app mode and 'i' key toggle, create basic
  commit_detail view module with placeholder rendering (Flags: V4)
- [X] T064b P0 feat - Display commit metadata in detail view: full message,
  author name, author date, commit date (Flags: V4)
- [X] T064c P0 feat - Add file list showing changed/added/removed files with
  status indicators (Flags: V4)
- [X] T064d P0 feat - Add complete diff rendering with +/- lines (plain text, no
  colors) (Flags: V4)
- [X] T065 P1 feat - Color diff output in commit detail view similar to tig:
  green for additions, red for deletions, cyan for hunk headers (Flags: V4)
- [X] T066 P1 feat - Support scrolling in commit detail view for long diffs
  (Flags: V4)
- [X] T067 P1 feat - Pressing 'i' again or Esc in detail view returns to the
  commit list with hunk groups (Flags: V4)

## Interactivity — Split Commit (V4)
- [X] T068 P0 feat - Add split mode on 's' key: prompt user to choose split
  strategy — one commit per file, per hunk, or per hunk cluster (Flags: V4)
- [X] T069 P0 feat - Implement per-file split: create N commits each applying
  one file's changes, using git2 cherry-pick/tree manipulation; refuse if
  staged/unstaged changes overlap (share file paths) with the commit being
  split, and report the conflicting file(s) to the user (Flags: V4)
- [X] T070 P1 feat - Implement per-hunk split: create one commit per hunk using
  git2 diff apply with filtered patches (Flags: V4)
- [X] T071 P1 feat - Implement per-hunk-cluster split: create one commit per
  fragmap cluster column (Flags: V4)
- [X] T072 P1 feat - Add numbering n/total to split commit messages in the
  subject line (Flags: V4)
- [X] T087 P2 feat - Before executing a split that would produce more than 5 new
  commits, show a yes/no confirmation dialog displaying the count and asking the
  user to confirm before proceeding (Flags: V4)

## Interactivity — Drop Commit (V4)
- [X] T084a P1 feat - Implement `drop_commit` on `GitRepo` trait: remove the
  selected commit by cherry-picking its descendants onto its parent. Return a
  `RebaseOutcome` that is either `Complete` on success or `Conflict` with enough
  state to resume or abort. Each cherry-pick step can conflict, so conflicts
  must be detected at every stage of the rebase. (Flags: V4)
- [X] T084b P1 feat - Implement `drop_commit_continue` and `drop_commit_abort`
  on `GitRepo` trait: after the user resolves conflicts in the working tree,
  `continue` stages the resolution and resumes cherry-picking the remaining
  descendants; `abort` restores the branch to its original state. (Flags: V4)
- [X] T084c P1 feat - Wire drop to 'd' key in the TUI: always prompt the user
  for confirmation before executing (Enter to confirm, Esc to cancel). (Flags:
  V4)
- [X] T084d P1 feat - Handle conflict during drop: when `drop_commit` returns a
  conflict, prompt the user to resolve it in their working tree (Enter to
  continue as resolved, Esc to abort the drop). (Flags: V4)
- [X] T092 P2 fix - Wrap long commit summaries in the drop confirm and drop
  conflict dialogs so the title is never truncated when it exceeds the dialog
  width (Flags: V4)
- [X] T093 P2 feat - Show conflicting file paths in the drop conflict dialog:
  query the index for entries with conflict stage > 0 and list them inside the
  dialog so the user can see which files need to be resolved (Flags: V4)
- [X] T094 P1 fix - When `drop_commit_continue` is called with partially
  unresolved conflicts (some files still have conflict markers), detect the
  remaining conflicts, show them to the user inside the dialog, and keep the
  `DropConflict` mode active instead of returning an error and leaving the repo
  in a broken state (Flags: V4)
- [X] T095 P2 feat - When a merge conflict occurs during drop, offer to launch
  the user's configured merge tool (from `merge.tool` / `mergetool.<name>.cmd`
  git config) on each conflicted file. Suspend the TUI (disable raw mode, leave
  alternate screen), write the three index stages (base/ours/theirs) to temp
  files, invoke the tool and wait for it to exit (same contract as the commit
  message editor), then restore the TUI and re-read the index to refresh
  `conflicting_files`. If no merge tool is configured, leave the current
  behaviour unchanged. (Flags: V4)

## Interactivity — Move Commit (V4)
- [ ] T073 P0 feat - Add move mode on 'm' key: highlight selected commit and
  show a "move <short sha> here" insertion row navigable with arrow keys (Flags:
  V4)
- [ ] T074 P1 feat - Color the insertion row red with "move <short sha> here -
  likely conflict" when moving to a position that would cause a conflict (Flags:
  V4)
- [ ] T075 P0 feat - Execute the move via git2 cherry-pick rebase onto the new
  position, abort and notify user on conflict (Flags: V4)
- [ ] T076 P2 feat - On conflict, tell the user whether the conflict is in the
  moved commit or in a commit rebased on top of it (Flags: V4)

## Interactivity — Squash Commit (V4)
- [X] T099 P1 feat - Generalize conflict handling for reuse by squash and future
  operations: rename `drop_commit_continue`/`drop_commit_abort` →
  `rebase_continue`/`rebase_abort` on the `GitRepo` trait and `Git2Repo` impl,
  rename `AppAction::ContinueDrop`/`AbortDrop` → `RebaseContinue`/`RebaseAbort`,
  rename `AppMode::DropConflict` → `RebaseConflict`, add an `operation_label`
  field to `ConflictState` so the conflict dialog title and success messages
  reflect the originating operation ("Drop Conflict" vs "Squash Conflict"),
  extract conflict dialog code (`handle_conflict_key`, `render_drop_conflict`)
  from `views/drop.rs` into a new `views/conflict.rs`, and update all references
  in `main.rs`, `app.rs`, `AppMode::background()`, tests, and help text (Flags:
  V4)
- [X] T101 P1 feat - Remap split key from 's' to 'p' (sPlit) in the commit list
  view and help dialog, freeing 's' for squash which matches git's interactive
  rebase keybindings (Flags: V4)
- [X] T077 P0 feat - Add squash mode on 's' key: enter a `SquashSelect` app mode
  where the selected commit is the "source" and the user navigates with arrow
  keys to pick a squash target; the source is squashed *into* the target (target
  keeps its position, source is removed, their changes are combined); pressing
  Enter confirms the target, Esc cancels back to CommitList; block the key when
  the selected row is a staged/unstaged synthetic entry (Flags: V4)
- [X] T078 P1 feat - Color squash target candidates in SquashSelect mode: yellow
  if squashable without conflict, red if the squash would likely conflict
  (overlapping fragmap clusters), white/dim if unrelated (no shared hunks and no
  conflict) (Flags: V4)
- [X] T079 P0 feat - Implement `squash_commits` on the `GitRepo` trait: given
  source and target OIDs plus `head_oid`, create a combined tree by
  cherry-picking the target then the source onto the target's parent, then
  cherry-pick all remaining descendants (commits between target and source
  exclusive, plus commits after source) onto the result using
  `cherry_pick_chain` — return `RebaseOutcome` so conflicts during the
  descendant rebase are handled by the generalized conflict infrastructure
  (Flags: V4)
- [X] T100 P0 feat - Wire squash execution in the TUI: after the user picks a
  target in SquashSelect, open the editor (reuse `edit_message_in_editor`) with
  both commit messages concatenated — target message first, then a blank line,
  then source message, matching git's interactive-rebase squash format; if the
  user saves an unchanged or non-empty message, call `squash_commits`; on
  `RebaseOutcome::Conflict` enter `RebaseConflict` mode (reusing the generalized
  conflict dialog, continue, abort, and mergetool flows from T099); on success
  reload commits and show a confirmation message (Flags: V4)
- [x] T080 P2 feat - Handle squash-time conflict (source changes conflict with
  target changes): when creating the combined tree itself fails due to
  overlapping edits in the source and target commits, write the conflict to the
  working tree and enter `RebaseConflict` mode so the user can resolve,
  continue, abort, or launch the mergetool — same flow as descendant rebase
  conflicts (Flags: V4)
- [X] T102 P1 feat - Replace the SquashSelect overlay dialog with a footer-based
  context line: remove `squash_select::render()` and its centered dialog, and
  instead show a footer message in `render_footer` when in SquashSelect mode —
  e.g. `Squash: select target for <short_oid> "<summary>" · Enter confirm · Esc
  cancel` — so the commit list is never obscured while picking a squash target;
  the source commit's magenta highlight and candidate coloring already provide
  sufficient visual context (Flags: V4)
- [X] T103 P1 feat - Restrict SquashSelect cursor to earlier commits only: in
  `squash_select::handle_key`, clamp navigation so the cursor cannot move to
  commits later than (above) the source commit — squashing into a later commit
  is not supported; also dim the rows above the source in the commit list when
  in SquashSelect mode to visually indicate they are unreachable targets
  (Flags: V4)
- [ ] T104 P1 feat - Add fixup mode on 'f' key: works identically to squash
  ('s') — enters `SquashSelect`, uses the same target-picking UI, candidate
  coloring, and conflict handling — but instead of opening the editor with both
  messages concatenated, it silently keeps the target commit's message as-is
  (the source commit's message is discarded); reuse `squash_try_combine`,
  `squash_commits`, and `squash_finalize` with the target's message passed
  directly, skipping `edit_message_in_editor`; update the footer context line
  to say "Fixup" instead of "Squash" and add 'f' to the help dialog
  (Flags: V4)

## Interactivity — Reword Commit (V4)
- [X] T088 P1 feat - Implement `resolve_editor()` helper: walk GIT_EDITOR env
  var → core.editor git config → VISUAL env var → EDITOR env var → "vi"
  fallback, matching git's own editor resolution order (Flags: V4)
- [X] T089 P1 feat - Implement general `edit_message_in_editor(repo, message)`
  utility: write message to a tempfile, suspend TUI (disable raw mode, leave
  alternate screen), spawn the resolved editor with inherited stdio and the
  tempfile as argument, wait for exit, restore TUI (enable raw mode, re-enter
  alternate screen), read and return the edited message; works for both
  terminal-UI editors (e.g. `vim`, `emacs -nw`) and GUI editors that open their
  own window (e.g. `code --wait`) — this function is intentionally general so it
  can be reused when editing commit messages during squash (Flags: V4)
- [X] T090 P1 feat - Change reload key from 'r' to 'u' (update) in commit list
  view and help dialog, to free 'r' for reword (Flags: V4)
- [X] T091 P1 feat - Add 'r' reword key in commit list view: invoke
  `edit_message_in_editor` with the selected commit's message, then use git2 to
  recreate the commit with the same tree and parents but the new message; if the
  commit is not HEAD, cherry-pick all descendants onto the new commit chain
  (same approach as split) — no conflict risk since only the message changes and
  the tree content is identical at every step, so staged/unstaged working-tree
  changes are unaffected and do not need to block this operation; block the key
  (show an error) only when the selected row is a staged or unstaged synthetic
  entry (Flags: V4)

## Refactoring — TUI Architecture (V5)
- [X] T096 P1 feat - Refactor event loop to mode-first dispatch: flip the main
  match from action-first to mode-first so there is one small match on `AppMode`
  delegating to a `handle_action(action, app)` function in each view module
  (co-located with `render()`). Each handler returns an `ActionResult` enum
  (Handled, ExecuteSplit, ExecuteDrop, Quit, etc.) so view modules stay free of
  git/terminal dependencies and `main.rs` only interprets the result (Flags: V5)
- [X] T097 P2 feat - Extract shared dialog rendering helper: create
  `views/dialog.rs` with a `render_centered_dialog(frame, config)` utility that
  handles centering, clearing, bordering and wrapping — then refactor drop
  confirm, drop conflict, split select, split confirm and help dialogs to use
  it, eliminating the duplicated layout/clear/border code (Flags: V5)
- [X] T098 P2 feat - Formalize the overlay concept: add an
  `AppMode::background()` method that returns the underlying mode to render
  first for overlay modes (SplitSelect, SplitConfirm, DropConfirm, DropConflict,
  Help), then simplify the render dispatch in `main.rs` to call
  `render_mode(background)` then `render_mode(foreground)` instead of
  hand-coding the layering for each overlay variant (Flags: V5)

## Notes
