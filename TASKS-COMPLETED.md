# Completed Tasks

## CLI Reference Point
- [X] T002 P0 feat - Add git2 dependency to Cargo.toml
- [X] T003 P0 feat - Parse single CLI argument (commit-ish string)
- [X] T004 P0 feat - Open repo from current directory with git2
- [X] T005 P0 feat - Resolve CLI arg to Oid using revparse_single
- [X] T006 P0 feat - Get HEAD as Oid
- [X] T007 P0 feat - Call merge_base to find common ancestor
- [X] T008 P0 feat - Print reference commit hash to stdout
- [X] T009 P1 feat - Add integration test with TempDir fixture repo
- [X] T010 P1 feat - Test resolving branch name to ref point
- [X] T011 P1 feat - Test resolving tag to ref point
- [X] T012 P1 feat - Test resolving short hash to ref point
- [X] T013 P1 feat - Test resolving long hash to ref point

## TUI Commit List View
- [X] T014 P0 feat - Add ratatui and crossterm dependencies to Cargo.toml
 
- [X] T015 P0 feat - Create CommitInfo domain type (oid, summary, author, date)
  in lib.rs
- [X] T016 P0 feat - Implement list_commits(from_oid, to_oid) in library to get
  commits in range
- [X] T017 P0 feat - Create app module (src/app.rs) with AppState struct (Flags:

- [X] T018 P0 feat - Add commit list and selection index to AppState
- [X] T019 P0 feat - Implement methods for moving selection up/down in AppState
 
- [X] T020 P0 feat - Create event module (src/event.rs) for input handling
 
- [X] T021 P0 feat - Parse arrow keys and 'q' key in event module
- [X] T022 P0 feat - Create views module (src/views.rs) declaring commit_list
  submodule
- [X] T023 P0 feat - Create commit_list view (src/views/commit_list.rs) with
  render function
- [X] T024 P0 feat - Render table with "SHA" and "Title" column headers (Flags:

- [X] T025 P0 feat - Render commits oldest-to-newest with short SHA (7 chars)
  and summary
- [X] T026 P0 feat - Highlight selected row with different color/style (Flags:

- [X] T027 P0 feat - Update main.rs to initialize terminal with crossterm
  backend
- [X] T028 P0 feat - Implement main event loop: draw, handle input, update state
 
- [X] T029 P0 feat - Call list_commits with HEAD and reference point from CLI
  arg
- [X] T030 P0 feat - Handle 'q' key to exit and restore terminal
- [X] T031 P1 feat - Add integration test for list_commits returning correct
  order
- [X] T032 P1 feat - Add unit test for AppState selection movement
- [X] T033 P2 feat - Add TUI snapshot test with TestBackend for commit_list view
 

## TUI Enhancements
- [X] T035 P1 feat - Start application with HEAD commit selected instead of
  first commit
- [X] T036 P2 feat - Highlight table column headers with background color or
  style
- [X] T037 P1 feat - Make commit list scrollable when commits exceed screen
  height
- [X] T038 P1 feat - Render scrollbar for commit list when content exceeds
  visible area
- [X] T039 P1 feat - Add footer showing selected commit info (long SHA, commit
  position)
- [X] T040 P1 feat - Add clap dependency for CLI argument parsing
- [X] T041 P1 feat - Add --reverse flag to display commits in reverse order
 
- [X] T043 P2 feat - Remove Commits border from commit list table

## Fragmap — Diff Extraction
- [X] T044 P0 feat - Add diff domain types: FileDiff, Hunk, DiffLine, CommitDiff
 
- [X] T045 P0 feat - Add commit_diff(oid) function in repo.rs using git2 to
  extract CommitDiff for a single commit
- [X] T046 P1 feat - Add integration tests for commit_diff using fixture repos
 

## Fragmap — Span Extraction
- [X] T047 P0 feat - Add FileSpan type and extract_spans function in fragmap
  module
- [X] T048 P1 feat - Add unit tests for span extraction

## Fragmap — Matrix Generation
- [X] T049 P0 feat - Build fragmap matrix: commits x chunks with TouchKind
  cells, one column per hunk
- [X] T050 P1 feat - Add unit tests for matrix generation with fabricated
  CommitDiff data

## Fragmap — Conflict & Squashability Analysis
- [X] T051 P0 feat - Determine squashability between commit pairs sharing a
  column: yellow if trivial, red if conflicting
- [X] T052 P1 feat - Add unit tests for squashability logic

## Fragmap — TUI Rendering
- [X] T053 P0 feat - Compute fragmap data in main.rs and store in AppState
 
- [X] T054 P0 feat - Render fragmap grid right of commit title: white squares
  for touched chunks, colored lines between related commits
- [X] T055 P1 feat - Add snapshot tests for fragmap grid rendering
- [X] T056 P2 feat - Horizontal scrolling for fragmap columns exceeding
  available width
- [X] T057 P2 feat - Add horizontal scrollbar indicator for fragmap matrix
 
- [X] T058 P1 feat - Align fragmap matrix to the left, adjacent to title column
 
- [X] T059 P1 feat - Colorize SHA and title of commits where all touched
  clusters are squashable into the same single other commit
- [X] T060 P1 feat - Highlight related commits when a commit is selected: color
  SHA and title of squashable targets in yellow (COLOR_SQUASHABLE) and
  conflicting commits in red (COLOR_CONFLICTING), matching the vertical
  connector line colors

## Bugs
- [X] T042 P0 bug - Commit list shows commits from repo start to reference point
  instead of from HEAD to reference point

## Code Organization & Refactoring
- [X] T034 P2 feat - Move find_reference_point and list_commits from lib.rs to
  repo module

## Interactivity — Basic UI
- [X] T120 P2 fix - "Hunk groups" header label is truncated when the fragmap
  matrix has fewer columns than the label is wide: in `build_constraints` the
  third column uses `Constraint::Length(layout.fragmap_col_width)`, which clips
  the 11-character label to as few characters as there are cluster columns; fix
  by using `Constraint::Min(layout.fragmap_col_width)` (or
  `Constraint::Length(layout.fragmap_col_width.max(MIN_HEADER_WIDTH))`) for the
  fragmap column so the header always has enough room to display the full label
 
- [X] T121 P2 fix - Help dialog wraps long key-binding lines mid-text, splitting
  a single entry across two rows without indentation — making it hard to read;
  find the help text rendering in `views/help.rs` and ensure each entry either
  fits on one line or wraps with a hanging indent (e.g. align continuation lines
  under the description column) so no entry appears to be two separate bindings
 
- [X] T122 P2 fix - Dialogs that show multi-line body text (e.g. the "some
  conflicts are still unresolved" conflict dialog and similar) wrap long lines
  without preserving indentation: continuation lines start at column 0 inside
  the dialog instead of aligning with the start of the text on the first line;
  update `render_centered_dialog` (or the individual dialog callers) to apply a
  hanging indent when wrapping body lines, so wrapped text is visually grouped
  under its first line
- [X] T119 P1 fix - Handle Ctrl+C gracefully: always quit the application
  immediately regardless of the current mode; if the app is in `RebaseConflict`
  mode (i.e. a rebase is in progress with a half-applied working tree), call
  `rebase_abort` first to restore the branch to its original state before
  exiting, so the repo is never left in a broken state; for all other modes
  (including selection overlays like SquashSelect, MoveSelect, SplitSelect —
  none of which have touched the repo yet) simply quit directly; parse
  `KeyCode::Char('c')` with `KeyModifiers::CONTROL` in `AppMode::parse_key`, map
  it to a new `KeyCommand::ForceQuit`, and handle it in `main.rs` outside the
  per-mode dispatch so it cannot be shadowed; ensure raw mode and the alternate
  screen are properly restored before exit
- [X] T117 P2 feat - Allow the user to move the vertical separator bar between
  the commit list and the right panel (fragmap / commit detail) using Ctrl+Left
  and Ctrl+Right arrow keys; store the offset as a signed integer in `AppState`
  (e.g. `split_offset: i16`) defaulting to 0, clamp it so both sides keep a
  minimum width, parse `Ctrl+Left` / `Ctrl+Right` in `AppMode::parse_key` as new
  `KeyCommand` variants (`SplitLeft` / `SplitRight`), and apply the offset to
  the `split_x` constant in `render_main_view`


## Core Behavior & Constraints
- [X] T081 P0 feat - Exclude the reference point (merge-base) commit from the
  commit list and all operations — it is shared with the target branch and must
  not be squashed, moved, or split


## Interactivity — Basic UI
- [X] T061 P0 feat - Change exit key from 'q' to Esc
- [X] T062 P1 feat - Add vertical separator line between title column and hunk
  groups column
- [X] T063 P1 feat - Add help dialog on 'h' key showing all interactive
  keybindings (q=quit, i=info, s=split, m=move, h=help)
- [X] T085 P2 feat - Add 'r' key to reload: re-read the commit list from HEAD
  down to the originally calculated reference point (merge-base), refreshing
  after external git operations without restarting the tool
- [X] T086 P2 feat - Show staged and unstaged working-tree changes as synthetic
  rows at the top of the commit list (above HEAD), displayed with distinct
  labels ("staged" / "unstaged") and included in the fragmap matrix so their
  hunk overlap with commits is visible


## Interactivity — Fragmap View
- [X] T082 P1 feat - Improve selected row highlighting in the hunk group matrix;
  the current inverse-color style is hard to read — use a subtler approach such
  as a bold/bright foreground, a dim background tint, or a side marker (Flags:

- [X] T083 P2 feat - Add CLI flag `--no-dedup-columns` (or similar) to disable
  deduplication of identical hunk-group columns in the fragmap view, useful for
  debugging and understanding the raw cluster layout


## Interactivity — Commit Detail View
- [X] T064a P0 feat - Add DetailView app mode and 'i' key toggle, create basic
  commit_detail view module with placeholder rendering
- [X] T064b P0 feat - Display commit metadata in detail view: full message,
  author name, author date, commit date
- [X] T064c P0 feat - Add file list showing changed/added/removed files with
  status indicators
- [X] T064d P0 feat - Add complete diff rendering with +/- lines (plain text, no
  colors)
- [X] T065 P1 feat - Color diff output in commit detail view similar to tig:
  green for additions, red for deletions, cyan for hunk headers
- [X] T066 P1 feat - Support scrolling in commit detail view for long diffs
 
- [X] T067 P1 feat - Pressing 'i' again or Esc in detail view returns to the
  commit list with hunk groups


## Interactivity — Split Commit
- [X] T068 P0 feat - Add split mode on 's' key: prompt user to choose split
  strategy — one commit per file, per hunk, or per hunk cluster
- [X] T069 P0 feat - Implement per-file split: create N commits each applying
  one file's changes, using git2 cherry-pick/tree manipulation; refuse if
  staged/unstaged changes overlap (share file paths) with the commit being
  split, and report the conflicting file(s) to the user
- [X] T070 P1 feat - Implement per-hunk split: create one commit per hunk using
  git2 diff apply with filtered patches
- [X] T071 P1 feat - Implement per-hunk-cluster split: create one commit per
  fragmap cluster column
- [X] T072 P1 feat - Add numbering n/total to split commit messages in the
  subject line
- [X] T087 P2 feat - Before executing a split that would produce more than 5 new
  commits, show a yes/no confirmation dialog displaying the count and asking the
  user to confirm before proceeding


## Interactivity — Drop Commit
- [X] T084a P1 feat - Implement `drop_commit` on `GitRepo` trait: remove the
  selected commit by cherry-picking its descendants onto its parent. Return a
  `RebaseOutcome` that is either `Complete` on success or `Conflict` with enough
  state to resume or abort. Each cherry-pick step can conflict, so conflicts
  must be detected at every stage of the rebase.
- [X] T084b P1 feat - Implement `drop_commit_continue` and `drop_commit_abort`
  on `GitRepo` trait: after the user resolves conflicts in the working tree,
  `continue` stages the resolution and resumes cherry-picking the remaining
  descendants; `abort` restores the branch to its original state.
- [X] T084c P1 feat - Wire drop to 'd' key in the TUI: always prompt the user
  for confirmation before executing (Enter to confirm, Esc to cancel). (Flags:

- [X] T084d P1 feat - Handle conflict during drop: when `drop_commit` returns a
  conflict, prompt the user to resolve it in their working tree (Enter to
  continue as resolved, Esc to abort the drop).
- [X] T092 P2 fix - Wrap long commit summaries in the drop confirm and drop
  conflict dialogs so the title is never truncated when it exceeds the dialog
  width
- [X] T093 P2 feat - Show conflicting file paths in the drop conflict dialog:
  query the index for entries with conflict stage > 0 and list them inside the
  dialog so the user can see which files need to be resolved
- [X] T094 P1 fix - When `drop_commit_continue` is called with partially
  unresolved conflicts (some files still have conflict markers), detect the
  remaining conflicts, show them to the user inside the dialog, and keep the
  `DropConflict` mode active instead of returning an error and leaving the repo
  in a broken state
- [X] T095 P2 feat - When a merge conflict occurs during drop, offer to launch
  the user's configured merge tool (from `merge.tool` / `mergetool.<name>.cmd`
  git config) on each conflicted file. Suspend the TUI (disable raw mode, leave
  alternate screen), write the three index stages (base/ours/theirs) to temp
  files, invoke the tool and wait for it to exit (same contract as the commit
  message editor), then restore the TUI and re-read the index to refresh
  `conflicting_files`. If no merge tool is configured, leave the current
  behaviour unchanged.


## Interactivity — Move Commit
- [X] T073 P0 feat - Add move mode on 'm' key: highlight selected commit and
  show a "move <short sha> here" insertion row navigable with arrow keys.
  Design: move `KeyCommand` enum and key parsing into `app.rs`, implement
  `AppMode::parse_key(event: Event) -> KeyCommand` so each mode resolves
  ambiguous keys ('m' → `MoveCommit` in `CommitList`, `Mergetool` in
  `RebaseConflict`), and delete the `event` module. UI: add
  `AppMode::MoveCommit { source_index: usize, insert_before: usize }`;
  `build_rows` injects a styled separator row (e.g. `▶ move here`) at the
  insertion point — same pattern as the existing squash source highlight. A thin
  line between rows is not feasible with ratatui's Table widget without
  reimplementing layout.
- [-] T074 P1 feat - Color the insertion row red with "move <short sha> here -
  likely conflict" when moving to a position that would cause a conflict (Flags:
  WONT DO)
- [X] T075 P0 feat - Execute the move via git2 cherry-pick rebase onto the new
  position, abort and notify user on conflict
- [X] T076 P2 feat - On conflict, tell the user whether the conflict is in the
  moved commit or in a commit rebased on top of it


## Interactivity — Squash Commit
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

- [X] T101 P1 feat - Remap split key from 's' to 'p' (sPlit) in the commit list
  view and help dialog, freeing 's' for squash which matches git's interactive
  rebase keybindings
- [X] T077 P0 feat - Add squash mode on 's' key: enter a `SquashSelect` app mode
  where the selected commit is the "source" and the user navigates with arrow
  keys to pick a squash target; the source is squashed *into* the target (target
  keeps its position, source is removed, their changes are combined); pressing
  Enter confirms the target, Esc cancels back to CommitList; block the key when
  the selected row is a staged/unstaged synthetic entry
- [X] T078 P1 feat - Color squash target candidates in SquashSelect mode: yellow
  if squashable without conflict, red if the squash would likely conflict
  (overlapping fragmap clusters), white/dim if unrelated (no shared hunks and no
  conflict)
- [X] T079 P0 feat - Implement `squash_commits` on the `GitRepo` trait: given
  source and target OIDs plus `head_oid`, create a combined tree by
  cherry-picking the target then the source onto the target's parent, then
  cherry-pick all remaining descendants (commits between target and source
  exclusive, plus commits after source) onto the result using
  `cherry_pick_chain` — return `RebaseOutcome` so conflicts during the
  descendant rebase are handled by the generalized conflict infrastructure
 
- [X] T100 P0 feat - Wire squash execution in the TUI: after the user picks a
  target in SquashSelect, open the editor (reuse `edit_message_in_editor`) with
  both commit messages concatenated — target message first, then a blank line,
  then source message, matching git's interactive-rebase squash format; if the
  user saves an unchanged or non-empty message, call `squash_commits`; on
  `RebaseOutcome::Conflict` enter `RebaseConflict` mode (reusing the generalized
  conflict dialog, continue, abort, and mergetool flows from T099); on success
  reload commits and show a confirmation message
- [x] T080 P2 feat - Handle squash-time conflict (source changes conflict with
  target changes): when creating the combined tree itself fails due to
  overlapping edits in the source and target commits, write the conflict to the
  working tree and enter `RebaseConflict` mode so the user can resolve,
  continue, abort, or launch the mergetool — same flow as descendant rebase
  conflicts
- [X] T102 P1 feat - Replace the SquashSelect overlay dialog with a footer-based
  context line: remove `squash_select::render()` and its centered dialog, and
  instead show a footer message in `render_footer` when in SquashSelect mode —
  e.g. `Squash: select target for <short_oid> "<summary>" · Enter confirm · Esc
  cancel` — so the commit list is never obscured while picking a squash target;
  the source commit's magenta highlight and candidate coloring already provide
  sufficient visual context
- [X] T103 P1 feat - Restrict SquashSelect cursor to earlier commits only: in
  `squash_select::handle_key`, clamp navigation so the cursor cannot move to
  commits later than (above) the source commit — squashing into a later commit
  is not supported; also dim the rows above the source in the commit list when
  in SquashSelect mode to visually indicate they are unreachable targets (Flags:

- [X] T104 P1 feat - Add fixup mode on 'f' key: works identically to squash
  ('s') — enters `SquashSelect`, uses the same target-picking UI, candidate
  coloring, and conflict handling — but instead of opening the editor with both
  messages concatenated, it silently keeps the target commit's message as-is
  (the source commit's message is discarded); reuse `squash_try_combine`,
  `squash_commits`, and `squash_finalize` with the target's message passed
  directly, skipping `edit_message_in_editor`; update the footer context line to
  say "Fixup" instead of "Squash" and add 'f' to the help dialog


## Interactivity — Reword Commit
- [X] T088 P1 feat - Implement `resolve_editor()` helper: walk GIT_EDITOR env
  var → core.editor git config → VISUAL env var → EDITOR env var → "vi"
  fallback, matching git's own editor resolution order
- [X] T089 P1 feat - Implement general `edit_message_in_editor(repo, message)`
  utility: write message to a tempfile, suspend TUI (disable raw mode, leave
  alternate screen), spawn the resolved editor with inherited stdio and the
  tempfile as argument, wait for exit, restore TUI (enable raw mode, re-enter
  alternate screen), read and return the edited message; works for both
  terminal-UI editors (e.g. `vim`, `emacs -nw`) and GUI editors that open their
  own window (e.g. `code --wait`) — this function is intentionally general so it
  can be reused when editing commit messages during squash
- [X] T090 P1 feat - Change reload key from 'r' to 'u' (update) in commit list
  view and help dialog, to free 'r' for reword
- [X] T091 P1 feat - Add 'r' reword key in commit list view: invoke
  `edit_message_in_editor` with the selected commit's message, then use git2 to
  recreate the commit with the same tree and parents but the new message; if the
  commit is not HEAD, cherry-pick all descendants onto the new commit chain
  (same approach as split) — no conflict risk since only the message changes and
  the tree content is identical at every step, so staged/unstaged working-tree
  changes are unaffected and do not need to block this operation; block the key
  (show an error) only when the selected row is a staged or unstaged synthetic
  entry


## CLI Output & Compatibility
- [X] T109 P2 feat - Add `--static` CLI flag to output the commit SHA/title list
  and fragmap matrix to stdout without launching the interactive TUI, mimicking
  the behavior of the original fragmap tool; format each row as: short SHA in
  cyan, commit title truncated to 26 chars (grey if the commit is fully
  squashable, normal otherwise), then one character per cluster column — `.` for
  empty, a white-background space (`\x1b[47m \x1b[0m`) for a direct hunk-group
  touch (regardless of squash status), a yellow-background space
  (`\x1b[43m \x1b[0m`) for a squashable connector between two touching commits,
  and a red-background space (`\x1b[41m \x1b[0m`) for a conflicting connector;
  skip staged/unstaged synthetic rows (not present in original fragmap output);
  then exit
- [X] T110 P3 feat - Add `--no-color` CLI flag to disable all color output when
  used with `--static` from T109, producing plain text output suitable for
  piping or automated processing; ensure this works correctly with the fragmap
  symbols and commit list formatting

## Refactoring — TUI Architecture
- [X] T096 P1 feat - Refactor event loop to mode-first dispatch: flip the main
  match from action-first to mode-first so there is one small match on `AppMode`
  delegating to a `handle_action(action, app)` function in each view module
  (co-located with `render()`). Each handler returns an `ActionResult` enum
  (Handled, ExecuteSplit, ExecuteDrop, Quit, etc.) so view modules stay free of
  git/terminal dependencies and `main.rs` only interprets the result
- [X] T097 P2 feat - Extract shared dialog rendering helper: create
  `views/dialog.rs` with a `render_centered_dialog(frame, config)` utility that
  handles centering, clearing, bordering and wrapping — then refactor drop
  confirm, drop conflict, split select, split confirm and help dialogs to use
  it, eliminating the duplicated layout/clear/border code
- [X] T098 P2 feat - Formalize the overlay concept: add an
  `AppMode::background()` method that returns the underlying mode to render
  first for overlay modes (SplitSelect, SplitConfirm, DropConfirm, DropConflict,
  Help), then simplify the render dispatch in `main.rs` to call
  `render_mode(background)` then `render_mode(foreground)` instead of
  hand-coding the layering for each overlay variant
- [X] T123 P2 feat - Extract render_main_view from main.rs into
  views/main_view.rs: move the split-panel orchestrator (separator clamping,
  left/right area computation, fragmap hide/restore, commit_list + commit_detail
  coordination) out of main.rs into a proper view module
- [X] T124 P2 feat - Extract fragmap rendering helpers into
  views/hunk_groups.rs: move build_fragmap_cell, fragmap_cell_content,
  fragmap_connector_content, cluster_relation, commit_text_style, fragmap color
  constants, and render_horizontal_scrollbar out of commit_list.rs into a
  dedicated module. commit_list.rs calls into hunk_groups for the third table
  column
- [X] T125 P3 feat - Move SeparatorLeft/Right handling out of main event loop:
  instead of the event loop doing
  `if action == SeparatorLeft { ... continue; }`, handle separator_offset
  mutation inside the view handle_key (main_view or commit_list), returning
  AppAction::Handled

## CLI Output & Compatibility — continued
- [X] T128 P2 feat - Adapt title column width to terminal width in `--static`
  output: the original fragmap tool sets the title column width dynamically so
  that the SHA + title + hunk-group matrix fills the available terminal width;
  investigate the original Python implementation
  (https://github.com/amollberg/fragmap) to understand the exact layout
  algorithm (how many columns it reserves for SHA, separators, and the matrix,
  and how it clamps the title width), then implement the same or equivalent
  logic in `static_views::fragmap::render` — the title currently uses a fixed
  26-character truncation; instead, detect the terminal width (via
  `crossterm::terminal::size()` or a passed-in width, falling back to 80),
  compute `title_width = terminal_width − sha_width − separators − matrix_width`
  clamped to a sensible minimum, and truncate/pad the title to that width
 
- [X] T126 P2 feat - Add `--squashable-scope <commit|group>` CLI argument
  controlling what the squashable connector color/symbol means: `group` (default
  in TUI) — a connector in a column is squashable when *that hunk-group pair
  alone* has no intervening touches (current per-group behavior); `commit`
  (default in `--static`) — a connector is squashable only when the *entire
  lower commit* is fully squashable into the same single upper commit (i.e.
  `fragmap.is_fully_squashable()` is true and `squash_target()` points to that
  upper commit), matching the original fragmap tool's stricter rule; the
  argument must be valid in both TUI and `--static` modes; store the choice in
  `AppState` and thread it through the fragmap connector rendering logic in both
  `static_views::fragmap::render` and the TUI fragmap widget
- [X] T127 P2 fix - Respect the `-r` / `--reverse` flag when `--static` is used:
  currently `--static` always outputs commits in the order returned by
  `list_commits` (newest-first); when `--reverse` is also passed the rows should
  be printed oldest-first, matching the interactive TUI behavior
- [x] T111 P3 feat - Replace the current example application in `examples/` with
  a compatibility tool that takes a commit-ish as its argument, uses it to find
  the merge-base (same as `--static`), then builds a `Fragmap` object in the
  normal way and also runs the original `fragmap` binary (if installed) on the
  same repository/ref; the tool renders git-tailor's result through the static
  view and compares the two outputs column-by-column (columns may be in any
  order); if the same commit-cluster relationships are present in both it prints
  "OK"; otherwise it prints the `fragmap` output, then git-tailor's static
  output, plus a short summary explaining what differs

## Build & CI — continued
- [X] T114 P2 feat - Write comprehensive README.md documentation: describe what
  the tool does (interactive git commit browser with fragmap visualization and
  rebase operations), installation instructions, basic usage guide with key
  bindings, attribution to original fragmap tool (reference NOTICE file), note
  that the entire tool is AI-generated, and include a prominent data safety
  disclaimer warning users to push their changes before using the tool since any
  bugs may cause permanent data loss — author takes no responsibility for data
  loss under any circumstances, see Apache 2.0 license text
- [X] T115 P2 feat - Add CHANGELOG.md following keepachangelog.com format:
  create initial changelog with sections for Unreleased, version entries (Added,
  Changed, Deprecated, Removed, Fixed, Security), and update AGENTS.md to
  instruct AI agents to ask users whether changes should be noted in the
  changelog when completing tasks that add user-visible features or fix bugs
 

## Bug Fixes — continued
- [X] T129 P1 bug - Fix move/drop/fixup/squash/split losing working-tree and
  index changes: currently these rebase operations discard any uncommitted
  changes (both staged and unstaged) that exist in the working tree when the
  operation is applied; `reword` already preserves them correctly, so audit how
  `reword` saves and restores the working-tree and index state and apply the
  same stash-and-restore (or equivalent) pattern to `move_commit`,
  `drop_commit`, `squash_commit`, `fixup_commit`, and `split_commit` in the
  rebase engine; add integration tests in the `tests/` directory covering all
  five operations with both staged changes (files added to the index but not
  committed) and unstaged changes (modified tracked files not yet staged),
  asserting that after the operation completes the working tree and index
  reflect the same content that was present before the operation started (Flags:


## Interactivity — Auto-detection
- [X] T130 P2 feat - Auto-detect the repository default branch when no `<BASE>`
  is provided on the command line: resolve `origin/HEAD` via
  `git rev-parse --abbrev-ref origin/HEAD` (libgit2: look up the symbolic target
  of `refs/remotes/origin/HEAD`) and use the resulting branch as the base; fall
  back to the current hard-coded default if `origin/HEAD` is not set.
