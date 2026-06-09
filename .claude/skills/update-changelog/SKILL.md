---
description: Add an entry to the [Unreleased] section of CHANGELOG.md. Use when a user-visible change needs to be recorded.
argument-hint: <Added|Changed|Fixed|Removed> - <description>
allowed-tools: Read Edit
---

Add a changelog entry. Argument format: `<category> - <description>`, e.g. `Fixed - Squash no longer reports a spurious conflict across renames`.

Categories (case-insensitive): **Added**, **Changed**, **Fixed**, **Removed**.

**Rules:**
- All new entries go in the `## [Unreleased]` block at the top.
- Append to the matching `### <Category>` heading if it exists; create it if not.
- When creating a heading, maintain this order: Added → Changed → Deprecated → Removed → Fixed → Security.
- Write the bullet from the user's perspective: what changed and why it matters, not implementation details.
- No entries for internal refactors, test additions, CI tweaks, or doc corrections unless explicitly requested.

**Steps:**
1. If $ARGUMENTS is empty, ask for the category and description.
2. Parse category and description from $ARGUMENTS.
3. Read `CHANGELOG.md`, locate `## [Unreleased]`, find or create the correct `### <Category>` heading.
4. Append `- <description>` (capitalise first word, no trailing period unless multi-sentence).
5. Show the added line in context.
