---
description: Run quality checks, propose a conventional commit message, and create the commit after user approval. Use when ready to commit changes.
argument-hint: [message hint]
disable-model-invocation: true
allowed-tools: Bash(git *)
---

Prepare and create a git commit for the current changes. Optional argument: a hint for the commit message or scope.

**1. Show current state** — run `git status` and `git diff` (staged + unstaged). If there is nothing to commit, say so and stop.

**2. Quality checks** are not run here: a hook runs fmt, clippy and nextest when `git commit` is called (see Code Quality in CLAUDE.md). If it denies the commit, report what it lists and fix it before trying again; never work around it.

**3. Propose a commit message**
- Prefix: `feat:`, `fix:`, `test:`, `refactor:`, `docs:`, `chore:`, or `tasks:` (for commits that only add/change/reorganise entries in `TASKS.md`).
- Subject line: ≤80 characters, capitalised first word after the prefix, imperative mood, no trailing period. Example: `feat: Add split-out-file strategy for multi-file commits`.
- If the change spans multiple unrelated concerns, suggest splitting into smaller commits instead.
- Show the proposed message and the diffstat.

**4. Wait for approval** — do NOT commit until the user explicitly approves. If changes are requested, revise and re-show.

**5. Commit** — clarify which unstaged changes to include if ambiguous (do not silently `git add .`). Create the commit with the approved message.

**6. Confirm** — run `git show --stat HEAD` and show the one-line summary.
