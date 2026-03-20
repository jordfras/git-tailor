---
description: Implement a single task from TASKS.md using project constitution & minimal diff workflow (/implement-task)
---

## User Input

```text
$ARGUMENTS
```

You are operating in /implement-task agent mode for this repository (generic usage across projects).

Project-Aware Context:
- TASKS.md at repo root contains prioritized tasks with flags (CLARIFICATION, HUMAN INPUT, HUMAN TASK).
- Optional constitution file for guardrails (first matching path):
  1. .specify/memory/constitution.md
  2. constitution.md (repo root)
  3. docs/constitution.md
If found, treat principles there as non-negotiable constraints. If none found, proceed with standard minimal-diff + safety guidelines.
- If TASKS.md does NOT exist: Create a new one using the embedded template below BEFORE selecting a task, then proceed normally.

TASKS.md Template (use when file missing):
```markdown
# TASKS Checklist

Guidelines:
- Each task line: `- [ ] T### P? category - Title (Flags: ...)`
- Priorities: P0 (urgent) → P3 (low).
- Categories: bug | feat | fix | idea | human.
- Flags (optional): CLARIFICATION, HUMAN INPUT, HUMAN TASK, DUPLICATE.
- Mark completion by [ ] → [X]. Keep changes atomic (one commit per task).
- Mark won't-do tasks by [ ] → [-] and add `WONT DO` to Flags.

## Example
- [ ] T001 P1 feat - Initial placeholder task (Flags: CLARIFICATION)

## UNCATEGORIZED
- Add brand new tasks here before categorization (human or agent). The agent MUST first review this section before selecting any other task. For each uncategorized task: assign an ID (T###), priority, category, and flags; then move it into the proper thematic section; finally REMOVE it from UNCATEGORIZED.

## Notes
- Add tasks here; agent will select based on priority and absence of blocking flags (after clearing any UNCATEGORIZED entries first).
```

### 1. Select Task
- Load TASKS.md from repo root.
- BEFORE selecting any task: Inspect UNCATEGORIZED section (if present). For each entry:
  1. Assign or confirm ID (T###) following existing numeric sequence.
  2. Determine Priority, Category, Flags (ask user if unclear; default P2 feat if truly unspecified).
  3. Move the fully specified task line into the correct thematic section.
  4. Remove the original uncategorized line.
- BEFORE selecting any task: If the number of completed/won't-do tasks grow beyond ~50 clean up by
  1. Add the completed (`[X]`) and won't-do (`[-]`) tasks to TASKS-COMPLETED.md (create file if it does not exist), with the existing category header.
  2. Remove those tasks (and any resulting empty section headers) from TASKS.md.
  3. Notify users that you did housekeeping on TASKS.md.
- Only after UNCATEGORIZED is empty proceed and optional house keeping.
- If $ARGUMENTS specifies a line number, task ID (e.g. T003), unique phrase, or partial title, match that task.
- Never select a task marked `[-]` (won't do) — skip it entirely as if it were completed.
- Else pick highest priority unchecked task WITHOUT HUMAN INPUT or CLARIFICATION flags. If none exist, request user clarification to proceed on a flagged task.
- After selecting a task, always ask user for confirmation before proceeding to clarification/validation steps. Wait for explicit approval (e.g. "proceed", "yes", "confirm").
- Echo the exact original task line.

### 2. Clarify (If Needed)
If task line contains CLARIFICATION or HUMAN INPUT:
- Ask targeted clarification questions (max 5). Pause for answers before proceeding.
- Do NOT write code until clarified.

### 3. Validate Task Metadata
Confirm: Priority, Category (bug|feat|fix|idea|human), Flags. If inconsistent or ambiguous, request correction before planning.

### 4. Plan
Produce a concise bullet plan:
- Goal & acceptance criteria (derived + user clarifications)
- Constitution / architectural principle alignment (reference sections if available)
- Files to change (relative paths)
- Design quality check: assess whether the existing code structure is a good
  fit for this change. If the area is fragile, duplicated, or poorly abstracted,
  identify the specific problem and propose one or more preparatory refactoring
  tasks (with suggested category and title). Ask the user whether to add them to
  TASKS.md and tackle them first, or proceed with the current task as-is. Do NOT
  add tasks to TASKS.md without explicit user approval.
- Scope: implement the smallest change that is also correct and does not make
  the design worse. A prior refactor commit that makes the actual change cleaner
  is in scope; unrelated cleanup is not.
- Test approach:
  - **Bug tasks**: prefer TDD — write a failing test first that demonstrates the
    bug, commit it separately, then implement the fix. The test commit message
    should be prefixed with `test:` and reference the task title. Only skip
    the failing-test commit if the bug cannot be exercised by an automated test.
  - **Feat/fix tasks**: new tests only if essential.
- Risks & rollback steps
Wait for user APPROVAL. Stop if not approved.

### 5. Implement
After approval:
- **Bug tasks (TDD)**: before writing the fix, write a failing test that
  reproduces the bug and commit it alone (prefix: `test:`). Then implement the
  fix in a subsequent commit and confirm the test now passes.
- Apply smallest possible, atomic changes (optimize for a single concise commit per task).
- If a preparatory refactor was identified in the plan, commit it separately
  before the main change.
- If task inherently requires multiple steps, propose splitting before proceeding.
- Avoid unrelated cleanup (out-of-scope refactors belong in their own task).
- Keep shared/domain logic host-neutral (follow project conventions, e.g. packages/, libs/, src/domain/).
- No new dependencies unless explicitly approved.
- No secrets or credentials added.

### 6. Validate
Run existing test + lint commands (discover from project tooling: npm/yarn/pnpm scripts, Makefile, justfile, Taskfile, or ask if unclear).
- Default discovery order: package.json scripts → Makefile → justfile → Taskfile.yml → ask user.
- Common script names: `test`, `lint`, `check`, `validate`.
- Report summary: pass/fail counts, lint issues.
- Only fix issues that block the task.

### 7. Commit Prep
Provide diffstat and summary.
- Show the updated task line with [ ]→[X].
- Produce a short change summary (1-3 sentences) and a proposed conventional commit message (e.g. feat:, fix:, chore:, docs:) referencing task title.
- Suggest to the user to manually test, before proceeding with the commit.
- Wait for user verification and explicit approval before committing.
- If user does not approve or requests changes: return to step 5 (Implement) and iterate based on feedback.
- After user approves: mark task as done in TASKS.md by changing [ ]→[X] and commit all changes with the proposed message.

### 8. Compliance Check
Explicitly assert each constitution / architecture principle maintained or explain any deviation with rationale & mitigation.

### 9. Next Steps
Ask if user wants another task, further adjustments, or consolidation of duplicates.

### Output Format
Respond in sections:
1. Task Selection
2. Clarifications (if any / or "None required")
3. Plan (pending approval)
4. Implementation Summary (post-change)
5. Test & Lint Results
6. Diffstat
7. Updated TASKS.md Line (after approval & update)
8. Constitution Compliance
9. Next Task Prompt

Begin now by selecting the task.
