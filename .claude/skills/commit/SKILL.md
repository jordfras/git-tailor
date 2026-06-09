---
description: Run quality checks, propose a conventional commit message, and create the commit after user approval. Use when ready to commit changes.
argument-hint: [message hint]
disable-model-invocation: true
allowed-tools: Bash(cargo fmt) Bash(cargo clippy *) Bash(cargo test *) Bash(git *)
---

Prepare and create a git commit for the current changes. Optional argument: a hint for the commit message or scope.

**1. Show current state** — run `git status` and `git diff` (staged + unstaged). If there is nothing to commit, say so and stop.

**2. Run quality checks** in order:
```
cargo fmt
cargo clippy --all-targets
cargo test
```
If `cargo fmt` produces changes, show the diff and stage them. If `clippy` reports warnings, list them and ask whether to fix before committing (the codebase targets zero warnings). If tests fail, stop and report.

**3. Propose a commit message**
- Prefix: `feat:`, `fix:`, `test:`, `refactor:`, `docs:`, `chore:`, or `tasks:` (for commits that only add/change/reorganise entries in `TASKS.md`).
- Subject line: ≤80 characters, capitalised first word after the prefix, imperative mood, no trailing period. Example: `feat: Add split-out-file strategy for multi-file commits`.
- If the change spans multiple unrelated concerns, suggest splitting into smaller commits instead.
- Show the proposed message and the diffstat.

**4. Wait for approval** — do NOT commit until the user explicitly approves. If changes are requested, revise and re-show.

**5. Commit** — clarify which unstaged changes to include if ambiguous (do not silently `git add .`). Create the commit with the approved message.

**6. Confirm** — run `git show --stat HEAD` and show the one-line summary.
