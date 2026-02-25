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
- [ ] T081 P0 feat - Exclude the reference point (merge-base) commit from the
  commit list and all operations — it is shared with the target branch and must
  not be squashed, moved, or split (Flags: V4)

## Interactivity — Basic UI (V4)
- [X] T061 P0 feat - Change exit key from 'q' to Esc (Flags: V4)
- [X] T062 P1 feat - Add vertical separator line between title column and hunk
  groups column (Flags: V4)
- [ ] T063 P1 feat - Add help dialog on 'h' key showing all interactive
  keybindings (q=quit, i=info, s=split, m=move, h=help) (Flags: V4)

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
