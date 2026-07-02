# git-tailor README demo — a single, follow-along flow:
#   view a fix commit -> fold it in as a fixup -> drop a debug commit ->
#   resolve the resulting conflict in the editor -> land the cleaned history.
#
# Assumptions provided by the build (see demo/scripts + the Makefile):
#   * `gt` (git-tailor) is on PATH
#   * $DEMO_REPO points at a freshly generated demo repo
#     (demo/scripts/make-demo-repo.sh), checked out on its feature branch
#   * $EDITOR is vim (the Docker image sets this)
#
# Run from the repo root:  vhs demo/tapes/demo.tape

Output demo/out/demo.gif

Require gt

Set Shell bash
Set FontSize 16
Set Width 1100
Set Height 700
Set Theme "Dracula"
Set Padding 20
Set TypingSpeed 90ms

# ── Setup (not recorded): step into the prepared repo and clear the screen ──
Hide
Type 'cd "${DEMO_REPO:-/tmp/demo-repo}"'
Enter
Type 'clear'
Enter
Show

# ── Launch git-tailor ──────────────────────────────────────────────────────
Type "gt"
Sleep 500ms
Enter
Sleep 2.5s

# ── Inspect the stray "fix" commit ─────────────────────────────────────────
# Cursor starts on HEAD (bottom); move up to
# "fix: parse parenthesised expressions".
Up 3
Sleep 1.5s
Enter
Sleep 3s
Type "q"
Sleep 1.5s

# ── Fold that fix into the parser commit it belongs to ─────────────────────
Type "f"
Sleep 1.5s
Up 4
Sleep 1.5s
Enter
Sleep 3s

# ── Drop the debug commit that should never be merged ──────────────────────
# Jump to the top of the history, then down to "DROPME: hack in debug tracing".
Type "g"
Sleep 800ms
Down 5
Sleep 1.5s
Type "d"
Sleep 1.5s
Enter
Sleep 3s

# ── A later fix touched the same lines, so the drop conflicts. Resolve it. ──
Type "e"
Sleep 2s
Type "/^<<<<<<<"
Enter
Sleep 1s
Type "3dd"
Sleep 1.2s
Type "/^>>>>>>>"
Enter
Sleep 1s
Type "dd"
Sleep 1.2s
Type ":wq"
Enter
Sleep 2s

# ── Continue: git-tailor lands the rebase with the conflict resolved ───────
Enter
Sleep 3.5s

# ── Done: a clean, merge-ready history ─────────────────────────────────────
Type "q"
Sleep 1s
