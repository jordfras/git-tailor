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

## Bug Fixes — Windows Compatibility
- [X] T136 P1 bug - Error messages disappear instantly on Windows: on Windows,
  crossterm fires both a key-down and a key-release event for a single
  keystroke; error messages shown after an invalid operation (e.g. attempting a
  move or squash with unstaged changes) are dismissed immediately because the
  key-release event is treated as the user acknowledgement key press, making the
  message unreadable; filter out `KeyEventKind::Release` (and
  `KeyEventKind::Repeat` if appropriate) events in the input handling layer so
  that only `KeyEventKind::Press` events are acted upon, matching the Linux
  behavior where only press events are emitted
- [X] T137 P2 bug - First commit always excluded when browsing complete history:
  when the user passes the very first (root) commit of the repository as the
  positional `base` argument, that commit is never shown in the commit list; the
  root cause is that `main.rs` always filters out the reference-point commit
  (`filter(|c| c.oid != reference_oid)`) because in the normal branch-workflow
  the merge-base is shared history that should not be editable; for
  complete-repository history this invariant does not hold and the root commit
  must be included; the fix should detect the root-commit / no-parent case (or
  add an `--all` flag) to skip the exclusion filter so that all commits from
  HEAD down to and including the first commit are shown and can be reordered,
  squashed, or split; the rebase engine's `reference_oid` concept (the "parent"
  onto which cherry-picks land) also needs to handle the case where there is no
  parent commit — likely by cherry-picking onto an empty tree for the first
  commit in the new sequence

## Bug Fixes — Squash & Fixup
- [X] T131 P1 bug - Fixup conflict resolution incorrectly opens commit message
  editor: when a fixup operation causes a conflict in the squash tree itself and
  the user resolves it, `RebaseContinue` in `main.rs` always opens the editor
  for the commit message (via `squash_finalize`) regardless of whether the
  operation was a squash or a fixup; the `SquashContext` needs an `is_fixup`
  field (or equivalent) so that the editor is skipped and the target message is
  used as-is when finalizing a fixup, mirroring the non-conflict path in
  `PrepareSquash`
- [X] T132 P1 bug - Fixup conflict falsely reported as still unresolved: after
  the user resolves a conflict during a fixup (either manually or via mergetool)
  and presses Enter to continue, `rebase_continue` in `git2_impl.rs` re-reads
  the index with `index.read(true)` and calls `index.has_conflicts()`, which
  returns true even though the working-tree file has been correctly resolved and
  staged; investigate whether libgit2's in-memory index is not being refreshed
  from disk before the `has_conflicts()` check, or whether deleted-file
  conflicts leave behind phantom stage entries, and fix so that a genuinely
  resolved index is not incorrectly treated as unresolved
- [X] T133 P1 bug - Aborting a fixup after a conflict leaves dirty working tree:
  `rebase_abort` in `git2_impl.rs` resets the branch ref and calls
  `checkout_head()`, but this does not clean up untracked files or staged
  deletions that were left behind by the failed cherry-pick (e.g. a file that
  was deleted in the conflict appears as a staged deletion and also as an
  untracked file after the abort); the abort should additionally clean untracked
  files and reset the index so the working tree matches HEAD, similar to what
  `git checkout -f HEAD` followed by `git clean -fd` would do (fixed by T130:
  libgit2's `checkout_head(force)` already resets both the index and workdir to
  HEAD, including files absent from HEAD's tree; the dirty-workdir symptom was a
  consequence of T130's `stage_file` bug leaving the index in a corrupt state;
  integration test added to confirm)
- [X] T134 P1 bug - External editor conflict resolution not detected during
  squash/fixup: when a conflict occurs during squash or fixup and the user
  resolves it by editing the conflicted file in an external editor (e.g. VS
  Code) and saving, git-tailor does not detect the resolution; opening the
  built-in mergetool afterward still shows the original conflict markers as if
  the external edits were ignored; resolving via the built-in mergetool works
  correctly; the likely cause is that git-tailor reads the file content from
  git2's in-memory state or a cached copy rather than re-reading from the
  working tree on disk when checking conflict status or launching the mergetool

## Bug Fixes — Split
- [X] T147 P1 bug - Segfault when splitting a submodule-change commit per file:
  calling "split per file" on a commit that updates a submodule revision causes
  a segfault; the split-per-file path in `git2_impl.rs` iterates over the
  commit's diff entries and builds per-file patches using `Diff::apply_to_tree`,
  but a submodule change produces a delta whose old/new objects are commit OIDs
  rather than blob OIDs; attempting to treat a submodule entry as a regular blob
  (e.g. passing it to `Blob::lookup` or building a patch from it) likely
  triggers a null dereference or invalid memory access inside libgit2; the fix
  should detect submodule deltas (delta kind `GIT_DELTA_*` where the object mode
  is `GIT_FILEMODE_COMMIT`, i.e. `0o160000`) and handle them explicitly — either
  by applying the submodule pointer update as a tree-level operation instead of
  a blob diff, or by grouping all submodule deltas into a single synthesised
  commit so the split result is well-formed; add an integration test using a
  `TempDir` repo with a real submodule to reproduce the crash and verify the fix
- [X] T148 P2 bug - Split commits lose the original commit message body: all
  three split strategies (per-file, per-hunk, per-hunk-group) construct the
  message for each new commit using only `commit.summary()` (the first line),
  appending a `(n/total)` counter; a commit whose message has a multi-line body
  or a detailed description will have that body silently discarded; the fix
  should use `commit.message()` instead, replacing just the first line with the
  summary + counter so the full body is retained in all split commits (or at
  least in the last one, mirroring what `git commit --amend` and `git rebase` do
  by default); all three `format!` message expressions in `git2_impl.rs` need
  updating
- [X] T150 P2 bug - Splitting the root commit in `--all` mode fails with "Can
  only split a commit with exactly one parent": `split_commit_per_file`,
  `split_commit_per_hunk`, and `split_commit_per_hunk_group` in `git2_impl.rs`
  all reject commits with `parent_count != 1`; the fix should apply the same
  pattern used for `move_commit` — build the first split-piece commit as a new
  orphan root (applying its diff onto an empty tree with no parents), then
  cherry-pick the remaining split pieces and any later commits on top

## Bug Fixes — Move Commit
- [X] T149 P2 bug - Moving a commit to the earliest position places it second
  instead of first: when using `gt --all` (or any case where the oldest visible
  commit is also the root commit), selecting a commit and choosing to move it
  before the first commit in the list results in the commit being placed
  immediately after the root commit rather than before it; the status message
  reports success; the root cause is likely that `move_commit` in `git2_impl.rs`
  resolves the "insert before first commit" target as "insert after merge-base /
  root", but for `--all` the root commit is included in the editable list which
  makes this the wrong reference point; the fix should ensure that when the
  target position is before the first commit, the entire cherry-pick chain is
  rebuilt with the root commit cherry-picked onto an empty tree first, the same
  way T137 handled the no-parent case for the initial rebase

## Interactivity — Conflict Resolution
- [X] T135 P2 feat - Add option to open the configured editor when resolving a
  conflict: the conflict view currently offers a key binding to launch the
  mergetool (`core.mergetool` / `merge.tool`); add a second key binding (e.g.
  `e`) that instead opens the conflicted file in the user's configured editor
  (`core.editor`, falling back to `$VISUAL`, then `$EDITOR`, then a sensible
  default such as `vi`); after the editor exits, re-check the file for conflict
  markers and update the conflict view state accordingly, the same way the
  mergetool path does

## Interactivity — Fragmap View
- [X] T105 P2 feat - Add glyph-weight focus highlighting to the fragmap matrix:
  clusters related to the focus commit (selected commit in CommitList, source
  commit in SquashSelect/MoveSelect) use heavy glyphs — `█` for touched squares
  and `┃` for connectors — while unrelated clusters use light glyphs — `▪` for
  touched squares and `┆` for connectors. Colors stay unchanged (white for
  conflicting squares, grey for squashable squares, red/yellow for connectors).
  This makes it immediately scannable which hunk groups the focus commit
  participates in without introducing new colors. "Related" means the cluster
  column contains a touch from the focus commit. Implement as a `FocusTheme`
  behind the `FragmapTheme` trait from T106.
- [X] T106 P2 feat - Refactor fragmap cell rendering into a `FragmapTheme` trait
  with four methods keyed by two enums: `SquareRole` (`Current` = the focus
  commit's own square, `Related` = another commit's square in a focus-cluster
  column, `Unrelated` = any square in a non-focus-cluster column),
  `ConnectorRole` (`Related` = the column is a focus cluster, `Unrelated` =
  otherwise), and `RelationType` (`Conflict` | `Squashable`); the trait methods
  are `square_symbol(SquareRole, RelationType) -> char`,
  `square_style(SquareRole, RelationType) -> Style`,
  `connector_symbol(ConnectorRole, RelationType) -> char`, and
  `connector_style(ConnectorRole, RelationType) -> Style`; implement
  `PlainTheme` reproducing the current uniform heavy-glyph behavior (no focus
  distinction); replace the inline constant lookups in `fragmap_cell_content`,
  `fragmap_connector_content`, and `build_fragmap_cell` with calls through the
  trait so that adding new themes (T105, T107) doesn't require scattering
  conditionals throughout the rendering functions
- [X] T107 P3 feat - Add `--theme <THEME>` CLI option to select the fragmap
  rendering theme; three themes are supported: `plain` (the current uniform
  heavy-glyph rendering with no focus-related highlighting, equivalent to
  DefaultTheme from T106), `highlight` (glyph-weight focus highlighting from
  T105 where clusters related to the selected commit use heavy glyphs and
  unrelated clusters use light glyphs), and `classic` (identical rendering to
  `--static`, reproducing the traditional fragmap tool appearance); store the
  selected theme in `AppState` and select the appropriate `FragmapTheme`
  implementation at startup; `plain` should be the default
- [X] T108 P1 fix - Fix fragmap relations not following file renames: when a
  file is renamed across commits, spans should cluster together if they overlap
  the same logical content, but currently they are treated as separate files and
  don't form clusters. Investigate the original fragmap Python implementation
  (https://github.com/amollberg/fragmap) to see how rename detection is handled
  in span clustering, and adapt the SPG logic in `src/fragmap/spg.rs` to
  properly track renamed files so that overlapping spans across renames are
  correctly clustered together

## Interactivity — Commit Detail View
- [X] T139 P3 feat - Add text search in commit detail view: add an incremental
  search mode activated by `/` (vim convention) that opens a search input bar at
  the bottom of the commit detail view; as the user types, highlight all matches
  in the visible diff content and scroll to the first match; support `n` / `N`
  to jump to next / previous match; `Escape` dismisses the search bar; the
  search should operate over the rendered diff text (file paths, hunk headers,
  and diff lines) and wrap around at the end of the content

## Build & CI
- [X] T112 P3 feat - Set up cargo-deny with configuration to check dependency
  licenses are compatible with Apache 2.0: install cargo-deny, create
  `deny.toml` config allowing Apache-compatible licenses (Apache-2.0, MIT,
  BSD-2-Clause, BSD-3-Clause, ISC, etc.), deny copyleft licenses (GPL, LGPL,
  AGPL), and add `cargo deny check` command to verify no license violations in
  the dependency tree
- [X] T113 P3 feat - Add cargo-deny to GitHub Actions CI: create or update
  `.github/workflows/ci.yml` to run `cargo deny check licenses` alongside
  existing format/clippy/test checks, failing the build if any dependency
  license conflicts are detected; ensure this runs on pull requests and main
  branch pushes

## Refactoring — TUI Architecture
- [X] T151 P3 fix - Eliminate duplication between `AppState::new()` and
  `AppState::with_commits()`: both functions repeat the same ~30 field
  initializations verbatim; implement `Default` for `AppState` containing all
  the zero-values, then have `with_commits` construct via `AppState { commits,
  selection_index, ..Default::default() }` and `new` delegate to `Default`;
  remove the duplicate field lists entirely
- [X] T152 P3 fix - Extract repeated `head_oid` fetch pattern in `main.rs`: the
  block `match git_repo.head_oid() { Ok(oid) => oid, Err(e) => {
  app.set_error_message(...); continue; } }` appears five times in the
  `AppAction` dispatch arms (PrepareSplit, PrepareDropConfirm, PrepareReword,
  PrepareSquash, ExecuteMove); extract a local macro or inline helper
  `get_head_oid!(git_repo, app)` that encapsulates the error path so each call
  site is a single expression
- [X] T153 P3 fix - Add `CommitInfo::is_synthetic()` helper to replace scattered
  inline checks: the expression `commit.oid == "staged" || commit.oid ==
  "unstaged"` is repeated in five or more places across `app.rs` and
  `commit_list.rs`; add a `pub fn is_synthetic(&self) -> bool` method to
  `CommitInfo` in `lib.rs` and replace every inline occurrence with a call to it
- [X] T154 P3 fix - Introduce `Oid` / `VirtualOid` types: replace raw `String`
  OIDs throughout the codebase with a newtype `Oid(String)` (short()/long()
  accessors, Display, From<String>, From<&str>, From<git2::Oid>) and a
  `VirtualOid` enum (`Real(Oid)`, `Staged`, `Unstaged`) for commit-list entries
  that may be synthetic working-tree pseudo-commits; add
  `CommitInfo::is_synthetic()`, `From<&Oid> for git2::Oid`, and update all
  call sites, tests, and snapshots
- [X] T155 P3 fix - Extract common split-commit preamble into a shared helper:
  `split_commit_per_file`, `split_commit_per_hunk`, and
  `split_commit_per_hunk_group` in `git2_impl.rs` each begin with ~20 identical
  lines (parse OID, find commit, bail on merge commit, compute `parent_tree`
  handling the root-commit case, get `commit_tree`); extract a private helper
  `fn load_split_commit(repo, oid) -> Result<SplitCommitParts>` returning the
  shared values, and apply the same extraction to the three `count_split_*`
  methods which duplicate the same setup
- [X] T156 P3 fix - Remove redundant `visible_clusters` double-iteration in
  `compute_layout`: `commit_list.rs::compute_layout` iterates the fragmap matrix
  twice with identical predicate logic — once to compute `visible_cluster_count`
  for the scrollbar decision, then again to build
  `visible_clusters: Vec<usize>`; compute the `Vec` first and derive the count
  from `visible_clusters.len()` to eliminate the duplicate pass
- [X] T157 P3 fix - Split `src/repo/git2_impl.rs` (2238 lines) into focused
  sub-modules under `src/repo/git2_impl/`: `reads.rs` (head_oid, list_commits,
  commit_diff, staged/unstaged_diff, default_branch, root_commit_oid,
  get_config_string), `split.rs` (the three split_commit_per_* methods +
  count_split_per_* + the load_split_commit helper from T155), `squash.rs`
  (squash_try_combine, squash_finalize, squash_commits), `move_drop.rs`
  (move_commit, drop_commit, reword_commit), `conflict.rs` (rebase_continue,
  rebase_abort, collect_conflict_files, write_conflicts_to_workdir,
  auto_stage_resolved_conflicts, read_conflicting_files), and `hunks.rs` (the
  pure free-function helpers: apply_single_hunk_to_tree, apply_hunk_to_content,
  apply_multiple_hunks_to_content, apply_selected_hunks_to_tree,
  apply_gitlink_delta_to_tree, split_lines_keep_eol); the `Git2Repo` struct
  stays in `git2_impl.rs` and each sub-module adds its `impl Git2Repo` /
  `impl GitRepo for Git2Repo` block; preserves the existing public API
- [X] T158 P3 fix - Move the inline `#[cfg(test)] mod tests` block (~1600 lines)
  out of `src/fragmap.rs` (2387 lines) into a separate `src/fragmap/tests.rs`
  file gated by `#[cfg(test)] mod tests;` in `fragmap.rs`; production code drops
  to ~700 lines and the file becomes navigable; no behavioural change
- [X] T159 P2 fix - Extract `AppState::reload_preserving_selection(&impl
  GitRepo)` to replace the five-times-repeated pattern `let saved_index =
  app.selection_index; reload_commits(&git_repo, &mut app); app.selection_index
  = saved_index.min(app.commits.len().saturating_sub(1));` in `main.rs` (drop,
  move, squash, squash_finalize, rebase_continue); each call site becomes a
  single line
- [X] T160 P2 fix - Extract a `handle_rebase_outcome` helper (free fn or
  AppState method) in `main.rs` to consolidate the repeated `match outcome
  { Ok(RebaseOutcome::Complete) => { reload + preserve_selection +
  set_success_message }, Ok(RebaseOutcome::Conflict(state)) =>
  app.enter_rebase_conflict(*state), Err(e) => app.set_error_message(format!
  ("{label} failed: {e}")) }` block; called by ExecuteDrop, ExecuteMove,
  PrepareSquash, RebaseContinue, and the squash finalize path; reduces ~80
  lines of boilerplate
- [X] T161 P3 fix - Extract a `run_external_tool<T>(terminal, kb_enhanced, f)`
  helper in `main.rs` that wraps the `with_external_process(kb_enhanced, f) +
  terminal.clear()?` pattern; used by editor invocations (PrepareReword,
  squash-message editor, conflict-finalize editor) and the mergetool/editor
  conflict-resolution paths; the four current call sites collapse from 4 lines
  each to 1
- [X] T162 P2 fix - Decompose `main.rs::main()` (~530 lines) into focused
  helpers: `load_initial_commits(&git_repo, &cli)` returning `(Vec<CommitInfo>,
  String, bool)` (extracts lines 222–254), `setup_terminal()` returning a RAII
  `TerminalGuard` that owns raw mode + alternate screen + keyboard enhancement
  and restores them on Drop (extracts lines 285–303 and 743–748),
  `init_app_state(commits, &cli, &git_repo)` returning the configured `AppState`
  with synthetic rows (extracts lines 305–320), and split the giant `AppAction`
  dispatch `match` into `dispatch_action(action, &mut app, &git_repo, terminal,
  kb_enhanced) -> Result<()>` so `main` reads as a clear setup → loop → teardown
  flow under 50 lines
- [X] T163 P3 fix - Decompose `views/commit_detail.rs::render` (~290 lines) into
  focused helpers: `build_metadata_lines(commit) -> Vec<Line>` (oid, message,
  author, dates), `build_file_list_lines(diff) -> Vec<Line>` (the "Changed
  Files:" section with status indicators), `build_diff_lines(diff) -> Vec<Line>`
  (file headers + hunk headers + colored +/- lines), and
  `compute_scroll_layout(content_area, content) -> ScrollLayout` (returns
  text_area, scrollbar areas, max_scroll, max_h_scroll); render becomes a
  composition of these helpers + the search-highlight pass + widget calls
- [X] T164 P3 fix - Decompose `views/commit_list.rs::build_rows` (~190 lines)
  by extracting `fn row_text_style(app, focus_ctx: FocusContext, commit_idx,
  is_selected, is_synthetic) -> Style` to replace the 60-line nested
  if/else-if chain that picks the foreground style based on
  squash/move/normal mode; introduce a small `FocusContext` enum (`Squash {
  source_idx }`, `Move { source_idx }`, `Normal`) to make the dispatch
  explicit; also add `AppState::fragmap_index(visual_idx) -> usize` to remove
  the three repeated `if app.reverse { len-1-idx } else { idx }`
  expressions in build_rows

## Refactoring — Integration Tests
- [X] T190 P1 feat - Move duplicated `file_content_at` and `commits_from_head`
  helpers into `tests/common.rs`: identical 8-line and 13-line definitions
  appear at `tests/split_commit.rs:20`, `tests/squash_commit.rs:23`,
  `tests/drop_commit.rs:23`, `tests/move_commit.rs:23` (and the matching
  `commits_from_head` at `:31`/`:34`/`:34`/`:34`). Move both to
  `tests/common.rs` as `pub fn file_content_at(...)` /
  `pub fn commits_from_head(...)` and remove the four local copies; ~80 LOC of
  duplication eliminated and ~80 call sites stay readable via
  `common::file_content_at(...)` / `common::commits_from_head(...)`.
- [X] T191 P1 feat - Move duplicated `NoOpRepo` GitRepo stub into
  `tests/common.rs`: the `struct NoOpRepo` plus its ~120-line `GitRepo` impl
  (every method `unimplemented!()`/panics) is defined identically at
  `tests/tui_main_view.rs:30` and `tests/tui_commit_detail.rs:33`. Promote to
  `pub struct NoOpRepo;` in `tests/common.rs` and import from both files;
  eliminates ~120 LOC of risky copy-paste that has to stay in sync with the
  `GitRepo` trait.
- [X] T192 P1 feat - Add `assert_complete!` / `assert_conflict!` macros for
  `RebaseOutcome`: the patterns
  `assert!(matches!(result, RebaseOutcome::Complete), …)` and `match outcome {
  RebaseOutcome::Complete => panic!("expected conflict"),
  RebaseOutcome::Conflict(state) => *state }` recur 40+ times across
  `tests/{drop,squash,move}_commit.rs` and `tests/mergetool.rs`. Add two macros
  to `tests/common.rs`: `assert_complete!(outcome)` and
  `expect_conflict!(outcome) -> ConflictState` (returns the boxed state,
  panicking otherwise). Call sites become one line each and read as intent
  rather than as a match-on-an-enum.
- [X] T193 P2 feat - Add `assert_history!(repo, base, &["msg1", "msg2"])`
  helper: the pattern "walk commits from HEAD back to base, assert count, then
  per-commit assert summary contains/equals X" is repeated 15+ times across
  `tests/{split,squash,drop,move}_commit.rs` with bespoke loops. Add a helper in
  `tests/common.rs`: `pub fn assert_history(repo: &git2::Repository, base:
  git2::Oid, expected_summaries: &[&str])` that verifies the count and each
  summary in oldest-to-newest order with descriptive panic messages. Each test
  then asserts the post-rebase commit graph in a single line.
- [X] T194 P2 feat - Add `assert_file_contents!` macro: the pattern
  `assert_eq!(file_content_at(&test.repo, head_oid, "a.txt"), "alpha2\n");`
  appears 30+ times across the rebase-op tests. Add
  `assert_file_contents!(&test.repo, head_oid, "a.txt", "alpha2\n")` in
  `tests/common.rs` so call sites read declaratively and produce better failure
  messages including the file path. Build on T190 so the macro can call
  `common::file_content_at` directly.
- [X] T195 P2 feat - Build a `TuiTestHarness` to consolidate
  backend/terminal/draw/snapshot boilerplate: every TUI test repeats ~6 lines —
  create `TestBackend`, wrap in `Terminal`, call `terminal.draw(|f| ...)`, clone
  the buffer, snapshot. Repeated 20+ times across `tests/tui_*.rs`. Add
  `pub struct TuiTestHarness` to `tests/common.rs` with `new(width, height)`,
  `render(|frame| { ... }) -> Buffer`, and a `snapshot()` convenience that
  delegates to `insta::assert_debug_snapshot!`. Reduces each TUI test to: `let
  mut h = TuiTestHarness::std(); let buf = h.render(|f|
  views::commit_list::render(&mut app, f)); h.snapshot();`.
- [X] T196 P3 feat - Introduce terminal-dimension constants for tests:
  `TestBackend::new(80, 24)` / `(120, 20)` / `(80, 10)` / `(60, 10)` /
  `(80, 12)` are scattered across 25+ TUI test sites
  (`tests/tui_squash_select.rs`, `tui_move_select.rs`, `tui_main_view.rs`,
  `tui_commit_detail.rs`, `tui_theme.rs`, `tui_fragmap.rs`). Define a small set
  of named constants in `tests/common.rs` —
  `TERMINAL_STD: (u16, u16) = (80, 24)`,
  `TERMINAL_WIDE: (u16, u16) = (120, 20)`,
  `TERMINAL_SHORT: (u16, u16) = (80, 10)`,
  `TERMINAL_NARROW: (u16, u16) = (60, 10)`,
  `TERMINAL_PICKER: (u16, u16) = (80, 12)` — and replace the magic numbers.
  Pairs naturally with T195's `TuiTestHarness::std()` / `wide()` / `short()`
  constructors.
- [X] T197 P3 feat - Generalize the 3-commit TUI fixture into
  `common::create_n_commit_app(&[...])`: the helper `make_app_in_squash_select`
  / `make_app_in_move_select` and similar in 6+ TUI test files all build an
  `AppState` whose `commits` field is a hand-rolled
  `vec![common::create_test_commit("aaa111…", "Oldest"), ...]`. Add `pub fn
  create_n_commit_app(summaries: &[&str]) -> AppState` to `tests/common.rs` that
  synthesises deterministic OIDs from the index and populates `commits`.
  Per-file helpers shrink to one or two lines and adding a 4th/5th commit to a
  test no longer requires inventing a fake OID.
- [X] T198 P3 feat - Add `common::create_drop_conflict(&TestRepo) ->
  ConflictState` fixture: the same 3-commit setup that triggers a drop conflict
  (base → adds line → depends on dropped line) appears at
  `tests/mergetool.rs:119` and a couple of places in `tests/drop_commit.rs`
  (e.g. lines 185–210). Extract a helper that returns the resulting
  `ConflictState` so tests focused on conflict resolution start with a one-line
  setup and read more like specifications.
- [X] T199 P3 feat - Centralize stub `GitRepo` variants (NoOpRepo + FakeDiffRepo
  + a builder) in `tests/common.rs`: TUI tests need `GitRepo` instances that
  either panic on every call (`NoOpRepo`, see T191) or return a canned
  `CommitDiff` for one method (`FakeDiffRepo` lives inline in
  `tests/tui_commit_detail.rs`). Once T191 lands, also lift `FakeDiffRepo` and
  add a small builder pattern (e.g.
  `StubRepoBuilder::new().with_commit_diff(diff)
  .build()`) so future TUI tests that need to mock another `GitRepo` method can
  do so without copying the giant impl block.
- [-] T200 P2 feat - Introduce file-path constants for tests: hardcoded
  `"a.txt"`, `"b.txt"`, `"c.txt"`, `"x.txt"`, `"y.txt"`, `"z.txt"`,
  `"root.txt"`, `"unrelated.txt"` appear 50+ times across
  `tests/{split,squash,drop,move}_commit.rs` and `tests/mergetool.rs`. Define
  `pub const FILE_A: &str = "a.txt";` (etc.) in `tests/common.rs` and use them;
  makes test file usage grep-able and lets a future rename touch one place.
  Pairs naturally with T194's `assert_file_contents!` macro. (Flags: WONT DO)
- [X] T201 P2 feat - Add `assert_file_contents_at_head!` macro: the pattern `let
  head_oid = test.repo.head().unwrap().target().unwrap();
  assert_file_contents!(&test.repo, head_oid, path, expected);` recurred 15+
  times at pure-HEAD assertion sites. Added
  `assert_file_contents_at_head!($repo, $path, $expected)` to
  `tests/common/assert.rs` (delegates to `assert_file_contents!`) and migrated
  all pure-HEAD call sites in `drop_commit`, `move_commit`, `split_commit`, and
  `squash_commit`. Sites where the raw `git2::Oid` is also used for
  `find_commit`, `revwalk`, `merge_base`, or `assert_eq` comparisons are left
  using `assert_file_contents!` directly.
- [-] T202 P2 feat - Add `TestRepo::file_at_head(path)` shorthand: the pattern
  of looking up HEAD and reading a file's tree contents appears 50+ times after
  T190 lands as `let head_oid = ...;
  assert_eq!(common::file_content_at(&test.repo, head_oid, "a.txt"),
  ...)`. Add `pub fn file_at_head(&self, path: &str) -> String` on `TestRepo` so
  call sites become `assert_eq!(test.file_at_head("a.txt"), "alpha2\n")`. Halves
  the noise of HEAD lookups in assertions. (Flags: WONT DO)
- [-] T203 P2 feat - Add `TestRepo::commits(&[(path, content, msg), ...])`
  bulk-creation helper: the 3-commit setup `let base = test.commit_file(...);
  let mid = test.commit_file(...); let head = test.commit_file(...);` recurs 20+
  times across `tests/{split,squash,drop,move}_commit.rs`. Add `pub fn
  commits(&self, configs: &[(&str, &str, &str)]) -> Vec<git2::Oid>` on
  `TestRepo` so tests can write `let [base, mid, head]: [git2::Oid; 3] =
  test.commits(&[(...), (...), (...)]).try_into().unwrap();` (or destructure
  however ergonomic). Reduces ~80 LOC of noisy commit setup. (Flags: WONT DO)
- [-] T204 P2 feat - Add `oid()` / `TestRepo::oid_of()` conversion helpers: the
  conversion `&Oid::from(commit_oid)` (where `commit_oid: git2::Oid`) appears
  30+ times across the rebase-op and mergetool tests, often clustered in the
  same call expression (e.g.
  `git_repo.drop_commit(&Oid::from(to_drop), &Oid::from(head))`). Add either a
  free `pub fn oid(v: git2::Oid) -> Oid` in `tests/common.rs` or a
  `TestRepo::oid_of(git2::Oid) -> Oid` method so call sites simplify to
  `.drop_commit(&oid(to_drop), &oid(head))`. Trivial wrapper but removes a lot
  of visual repetition. (Flags: WONT DO)
- [X] T205 P3 feat - Move `create_fragmap` and `simple_cluster` helpers into
  `tests/common.rs`: `tests/tui_fragmap.rs:19-45` defines `create_fragmap(...)`
  and a `simple_cluster(...)` helper used 10+ times in that file, and
  `tests/tui_squash_select.rs:255` re-defines its own near-identical
  `simple_cluster`. Promote both to `pub fn` in `tests/common.rs` (parameterised
  over path / line range / commit OIDs) and import from both files; future TUI
  tests that need synthetic fragmap state get the helpers for free.
- [X] T206 P3 feat - Split large test files into sub-modules for navigability:
  `tests/split_commit.rs` (1177 LOC), `tests/squash_commit.rs` (1046 LOC),
  `tests/drop_commit.rs` (737 LOC), and `tests/move_commit.rs` (476 LOC)
  currently use comment banners (`// --- Conflict tests ---`) to group related
  tests. Replace each with a thin entry-point that just declares sub-modules,
  e.g. `tests/squash_commit.rs` becomes `mod happy_path; mod conflict; mod
  dirty_state;` with the actual tests in `tests/squash_commit/happy_path.rs`,
  `tests/squash_commit/conflict.rs`, etc. Each sub-module declares `mod common;`
  (or uses a shared path attr). Improves IDE file-tree navigation, surfaces the
  test taxonomy in `cargo test` output, and creates natural homes for per-group
  fixtures. No logic changes.
- [X] T207 P3 feat - Add a `common::prelude` module re-exporting
  frequently used test imports: every rebase-op test starts with the
  same import block — `use git_tailor::repo::{Git2Repo, GitRepo,
  RebaseOutcome}; use git_tailor::Oid; use anyhow::Result;` plus
  `mod common;`. Add `pub mod prelude { pub use crate::*; pub use
  git_tailor::repo::{Git2Repo, GitRepo, RebaseOutcome}; pub use
  git_tailor::Oid; }` inside `tests/common.rs` (or as
  `tests/common/prelude.rs`) so each test file can write
  `use common::prelude::*;` and drop ~5 lines of repeated imports.
- [X] T208 P2 feat - Add `TestRepo::write_file`, `stage_file`, and `commit`
  helpers and rename `commit_file` to reflect what it does: `commit_file(path,
  content, message)` actually writes the file to disk, stages it, and creates a
  commit — three distinct operations. (1) Add `pub fn write_file(&self, path:
  &str, content: &str)` that just writes the file to the workdir (replacing the
  repeated `let workdir = test.repo.workdir().unwrap(); std::fs::write(...)` pair
  at ~25 call sites across `drop_commit/dirty_state.rs`,
  `squash_commit/dirty_state.rs`, `split_commit/dirty_state.rs`,
  `move_commit/dirty_state.rs`, `reword_commit.rs`, and others). (2) Add `pub
  fn stage_file(&self, path: &str)` that stages a single file (replacing the
  4-line `index.add_path` + `index.write` block at ~10 call sites in the same
  files, plus `drop_commit/continue_abort.rs`, `drop_commit/error_cases.rs`,
  `split_commit/per_file.rs`, `commit_diff.rs`). (3) Add `pub fn
  commit(&self, message: &str) -> git2::Oid` that commits whatever is currently
  staged (useful in `commit_diff.rs` where files are manually staged before
  committing, and as the building block for `commit_file`). (4) Rename
  `commit_file` → `write_stage_commit` (or a name the implementer prefers) so
  the name accurately describes the three-step operation; refactor its body to
  call `write_file` + `stage_file` + `commit`. Similarly refactor
  `commit_files` to delegate to the new primitives. No test-behaviour changes —
  purely mechanical cleanup.
