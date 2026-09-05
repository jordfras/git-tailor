---
description: Implement a single task from TASKS.md using the design-fit workflow. Use when the user asks to work on, pick, or implement a task.
argument-hint: [task-id | phrase]
disable-model-invocation: true
---

Implement a single task from `TASKS.md`. Argument: optional task ID (e.g. `T218`), line number, or phrase. If omitted, pick the highest-priority eligible task automatically.

---

### 1. Select Task

Load `TASKS.md`.

**UNCATEGORIZED first:** Inspect `## UNCATEGORIZED`. For each entry assign an ID (T###, continuing the sequence), priority, category, and flags — ask if unclear, default P2 feat. Move into the correct section, remove from UNCATEGORIZED. Do not proceed until UNCATEGORIZED is empty.

**Housekeeping:** If completed (`[X]`) and won't-do (`[-]`) tasks exceed ~50 total, move them to `TASKS-COMPLETED.md` (create if missing) under their section headers, remove from `TASKS.md`, and notify the user.

**Selection:** Match $ARGUMENTS if given (ID / line / phrase). Otherwise pick the highest-priority unchecked task without HUMAN INPUT or CLARIFICATION flags. Never select `[-]`. If only flagged tasks remain, ask the user.

Echo the exact task line and ask for confirmation. Wait for explicit approval before proceeding.

---

### 2. Clarify (if needed)

If the task has CLARIFICATION or HUMAN INPUT flags, ask targeted questions and wait for answers before writing any code.

---

### 3. Validate Metadata

Confirm Priority, Category (bug | feat | fix | idea | human), and Flags are consistent. Request correction if not.

---

### 4. Plan

Produce a concise bullet plan:
- **Goal & acceptance criteria**
- **Files to change** (relative paths)
- **Design quality check:** does the existing code fit this change cleanly? If the area is fragile, duplicated, or poorly abstracted, name the problem and propose a preparatory refactoring task. Ask whether to add it to `TASKS.md` and tackle it first — do not add tasks without explicit approval.
- **Test approach:**
  - *Bug tasks* — TDD: write a failing test first, commit it with `test:` prefix, then fix.
  - *Feat/fix tasks* — add tests only when essential.
- **Risks & rollback steps**

Wait for explicit approval. Stop if not approved.

---

### 5. Implement

- *Bug tasks (TDD):* commit the failing test alone (`test:` prefix) before writing the fix.
- Commit preparatory refactors separately from the main change.
- No unrelated cleanup in the same commit — surface it as a suggested new task instead.
- No new dependencies without explicit approval. No secrets or credentials.

---

### 6. Validate

```
cargo fmt
cargo clippy --all-targets
cargo test
```
Report pass/fail and any warnings. Fix only issues that block the task.

---

### 7. Commit

- Show the updated task line (`[ ]` → `[X]`).
- Suggest the user manually test the change.
- Wait for explicit approval. If rejected, return to step 5.
- After approval: update `TASKS.md` (`[ ]` → `[X]`), then follow the `commit` skill to create the commit.

---

### 8. Changelog

Ask whether the change warrants a `CHANGELOG.md` entry:
- **Yes for:** new CLI flags, new TUI features, changed user-visible behavior, bug fixes.
- **No for:** internal refactors, test additions, CI tweaks, doc corrections. Confirm if unsure.

If yes, follow the `update-changelog` skill.

---

### 9. Next Steps

Ask if the user wants another task, further adjustments, or to consolidate duplicates.

---

### Output Sections

1. Task Selection
2. Clarifications (or "None required")
3. Plan *(pending approval)*
4. Implementation Summary
5. Test & Lint Results
6. Diffstat
7. Updated TASKS.md line
8. Next Task Prompt
