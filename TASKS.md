# TASKS Checklist

Guidelines:
- Each task line: `- [ ] T### P? category - Title (Flags: ...)`
- Priorities: P0 (urgent) → P3 (low).
- Categories: bug | feat | fix | idea | human.
- Flags (optional): CLARIFICATION, HUMAN INPUT, HUMAN TASK, DUPLICATE.
- Version flags (optional): V1, V2 etc. (used to group versions/releases).
- Mark completion by [ ] → [X]. Keep changes atomic (one commit per task).


## UNCATEGORIZED

## CLI Reference Point (V1)
- [X] T002 P0 feat - Add git2 dependency to Cargo.toml (Flags: V1)
- [X] T003 P0 feat - Parse single CLI argument (commit-ish string) (Flags: V1)
- [X] T004 P0 feat - Open repo from current directory with git2 (Flags: V1)
- [X] T005 P0 feat - Resolve CLI arg to Oid using revparse_single (Flags: V1)
- [X] T006 P0 feat - Get HEAD as Oid (Flags: V1)
- [X] T007 P0 feat - Call merge_base to find common ancestor (Flags: V1)
- [X] T008 P0 feat - Print reference commit hash to stdout (Flags: V1)
- [X] T009 P1 feat - Add integration test with TempDir fixture repo (Flags: V1)
- [X] T010 P1 feat - Test resolving branch name to ref point (Flags: V1)
- [X] T011 P1 feat - Test resolving tag to ref point (Flags: V1)
- [X] T012 P1 feat - Test resolving short hash to ref point (Flags: V1)
- [X] T013 P1 feat - Test resolving long hash to ref point (Flags: V1)

## TUI Commit List View (V2)
- [X] T014 P0 feat - Add ratatui and crossterm dependencies to Cargo.toml
  (Flags: V2)
- [X] T015 P0 feat - Create CommitInfo domain type (oid, summary, author, date)
  in lib.rs (Flags: V2)
- [X] T016 P0 feat - Implement list_commits(from_oid, to_oid) in library to get
  commits in range (Flags: V2)
- [X] T017 P0 feat - Create app module (src/app.rs) with AppState struct (Flags:
  V2)
- [X] T018 P0 feat - Add commit list and selection index to AppState (Flags: V2)
- [X] T019 P0 feat - Implement methods for moving selection up/down in AppState
  (Flags: V2)
- [X] T020 P0 feat - Create event module (src/event.rs) for input handling
  (Flags: V2)
- [X] T021 P0 feat - Parse arrow keys and 'q' key in event module (Flags: V2)
- [X] T022 P0 feat - Create views module (src/views.rs) declaring commit_list
  submodule (Flags: V2)
- [X] T023 P0 feat - Create commit_list view (src/views/commit_list.rs) with
  render function (Flags: V2)
- [X] T024 P0 feat - Render table with "SHA" and "Title" column headers (Flags:
  V2)
- [X] T025 P0 feat - Render commits oldest-to-newest with short SHA (7 chars)
  and summary (Flags: V2)
- [X] T026 P0 feat - Highlight selected row with different color/style (Flags:
  V2)
- [X] T027 P0 feat - Update main.rs to initialize terminal with crossterm
  backend (Flags: V2)
- [X] T028 P0 feat - Implement main event loop: draw, handle input, update state
  (Flags: V2)
- [X] T029 P0 feat - Call list_commits with HEAD and reference point from CLI
  arg (Flags: V2)
- [X] T030 P0 feat - Handle 'q' key to exit and restore terminal (Flags: V2)
- [X] T031 P1 feat - Add integration test for list_commits returning correct
  order (Flags: V2)
- [X] T032 P1 feat - Add unit test for AppState selection movement (Flags: V2)
- [X] T033 P2 feat - Add TUI snapshot test with TestBackend for commit_list view
  (Flags: V2)

## TUI Enhancements (V2)
- [x] T035 P1 feat - Start application with HEAD commit selected instead of
  first commit (Flags: V2)
- [X] T036 P2 feat - Highlight table column headers with background color or
  style (Flags: V2)
- [X] T037 P1 feat - Make commit list scrollable when commits exceed screen
  height (Flags: V2)
- [X] T038 P1 feat - Render scrollbar for commit list when content exceeds
  visible area (Flags: V2)
- [x] T039 P1 feat - Add footer showing selected commit info (long SHA, commit
  position) (Flags: V2)
- [x] T040 P1 feat - Add clap dependency for CLI argument parsing (Flags: V2)
- [x] T041 P1 feat - Add --reverse flag to display commits in reverse order
  (Flags: V2)
- [x] T043 P2 feat - Remove Commits border from commit list table (Flags: V2)

## Fragmap — Diff Extraction (V3)
- [x] T044 P0 feat - Add diff domain types: FileDiff, Hunk, DiffLine, CommitDiff
  (Flags: V3)
- [X] T045 P0 feat - Add commit_diff(oid) function in repo.rs using git2 to
  extract CommitDiff for a single commit (Flags: V3)
- [X] T046 P1 feat - Add integration tests for commit_diff using fixture repos
  (Flags: V3)

## Fragmap — Span Extraction (V3)
- [X] T047 P0 feat - Add FileSpan type and extract_spans function in fragmap
  module (Flags: V3)
- [X] T048 P1 feat - Add unit tests for span extraction (Flags: V3)

## Fragmap — Matrix Generation (V3)
- [X] T049 P0 feat - Build fragmap matrix: commits x chunks with TouchKind
  cells, one column per hunk (Flags: V3)
- [X] T050 P1 feat - Add unit tests for matrix generation with fabricated
  CommitDiff data (Flags: V3)

## Fragmap — Conflict & Squashability Analysis (V3)
- [X] T051 P0 feat - Determine squashability between commit pairs sharing a
  column: yellow if trivial, red if conflicting (Flags: V3)
- [X] T052 P1 feat - Add unit tests for squashability logic (Flags: V3)

## Fragmap — TUI Rendering (V3)
- [X] T053 P0 feat - Compute fragmap data in main.rs and store in AppState
  (Flags: V3)
- [X] T054 P0 feat - Render fragmap grid right of commit title: white squares
  for touched chunks, colored lines between related commits (Flags: V3)
- [X] T055 P1 feat - Add snapshot tests for fragmap grid rendering (Flags: V3)
- [X] T056 P2 feat - Horizontal scrolling for fragmap columns exceeding
  available width (Flags: V3)
- [X] T057 P2 feat - Add horizontal scrollbar indicator for fragmap matrix
  (Flags: V3)
- [X] T058 P1 feat - Align fragmap matrix to the left, adjacent to title column
  (Flags: V3)
- [X] T059 P1 feat - Colorize SHA and title of commits where all touched
  clusters are squashable into the same single other commit (Flags: V3)
- [X] T060 P1 feat - Highlight related commits when a commit is selected: color
  SHA and title of squashable targets in yellow (COLOR_SQUASHABLE) and
  conflicting commits in red (COLOR_CONFLICTING), matching the vertical
  connector line colors (Flags: V3)

## Bugs
- [x] T042 P0 bug - Commit list shows commits from repo start to reference point
  instead of from HEAD to reference point (Flags: V2)

## Code Organization & Refactoring
- [X] T034 P2 feat - Move find_reference_point and list_commits from lib.rs to
  repo module (Flags: V2)

## Interactivity — Basic UI (V4)
- [ ] T061 P0 feat - Change exit key from 'q' to Esc (Flags: V4)
- [ ] T062 P1 feat - Add vertical separator line between title column and hunk
  groups column (Flags: V4)
- [ ] T063 P1 feat - Add help dialog on 'h' key showing all interactive
  keybindings (q=quit, i=info, s=split, m=move, h=help) (Flags: V4)

## Interactivity — Commit Detail View (V4)
- [ ] T064 P0 feat - Add commit detail view toggled by 'i' key: show full commit
  message, author, author date, commit date, list of changed/added/removed
  files, and complete diff with +/- lines (Flags: V4)
- [ ] T065 P1 feat - Color diff output in commit detail view similar to tig:
  green for additions, red for deletions, cyan for hunk headers (Flags: V4)
- [ ] T066 P1 feat - Support scrolling in commit detail view for long diffs
  (Flags: V4)
- [ ] T067 P1 feat - Pressing 'i' again or Esc in detail view returns to the
  commit list with hunk groups (Flags: V4)

## Interactivity — Split Commit (V4)
- [ ] T068 P0 feat - Add split mode on 's' key: prompt user to choose split
  strategy — one commit per file, per hunk, or per hunk cluster (Flags: V4)
- [ ] T069 P0 feat - Implement per-file split: create N commits each applying
  one file's changes, using git2 cherry-pick/tree manipulation (Flags: V4)
- [ ] T070 P1 feat - Implement per-hunk split: create one commit per hunk using
  git2 diff apply with filtered patches (Flags: V4)
- [ ] T071 P1 feat - Implement per-hunk-cluster split: create one commit per
  fragmap cluster column (Flags: V4)
- [ ] T072 P1 feat - Add numbering n/total to split commit messages in the
  subject line (Flags: V4)

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
- [ ] T077 P0 feat - Add squash mode on 'q' key: highlight selected commit and
  navigate with arrow keys to pick squash target (Flags: V4)
- [ ] T078 P1 feat - Color squash target candidate yellow if squashable, red if
  conflicting, white if no related hunks and no conflict (Flags: V4)
- [ ] T079 P0 feat - Execute squash via git2 cherry-pick combining commits,
  abort and notify user on conflict (Flags: V4)
- [ ] T080 P2 feat - On conflict, provide the user with details on which commit
  and files are conflicting (Flags: V4)

## Notes
