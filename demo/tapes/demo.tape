# git-tailor README demo — a single, follow-along flow:
#   view a fix commit -> fold it in as a fixup -> drop a debug commit ->
#   resolve the resulting conflict in the editor -> land the cleaned history.
#
# Assumptions provided by the build (see demo/scripts + demo/build.sh):
#   * `gt` (git-tailor) is on PATH
#   * $DEMO_REPO points at a freshly generated demo repo
#     (demo/scripts/make-demo-repo.sh), checked out on its feature branch
#   * $EDITOR is vim (the Docker image sets this)
#
# Run from the repo root:  vhs demo/tapes/demo.tape

Output demo/out/demo.gif

Require gt

Set Shell bash
Set FontSize 18
Set Width 1240
Set Height 500
# 24fps keeps the file small (vhs defaults to 50); post-render gifsicle trims
# the rest. See demo/scripts/render.sh.
Set Framerate 24
Set Padding 20
Set TypingSpeed 125ms

# ── Setup (not recorded): step into the prepared repo and clear the screen ──
Hide
Type 'cd "${DEMO_REPO:-/tmp/demo-repo}"'
Enter
Type 'clear'
Enter
Show

# ── Launch git-tailor ──────────────────────────────────────────────────────
Type "gt"
Sleep 400ms
Enter
Sleep 1.5s

# ── Inspect the stray "fix" commit ─────────────────────────────────────────
# Cursor starts on HEAD (bottom); step up one row at a time to
# "fix: parse parenthesised expressions".
Up 3
Sleep 700ms
Enter
Sleep 500ms
# Widen the detail pane (one step at a time), then scroll down to the diff.
Ctrl+Left
Sleep 125ms
Ctrl+Left
Sleep 125ms
Ctrl+Left
Sleep 125ms
Ctrl+Left
Sleep 1.2s
Down@60ms 14
Sleep 1.6s
Type "q"
Sleep 700ms

# ── Fold that fix into the parser commit it belongs to ─────────────────────
Space
Down 2
Sleep 700ms
Enter
Sleep 400ms
Up 4
Sleep 2s
Enter
Sleep 1.6s

# ── Drop the debug commit that should never be merged ──────────────────────
# The fixup left the cursor on the parser commit; step down to
# "DROPME: hack in debug tracing".
Down 4
Sleep 700ms
Space
Down 5
Sleep 700ms
Enter
Sleep 900ms
Enter
Sleep 1.4s

# ── A later fix touched the same lines, so the drop conflicts. Resolve it ───
# (kept brief — the point is that git-tailor drops you straight into $EDITOR).
Type "e"
Sleep 500ms
Type@60ms "/^<<<<<<<"
Enter
Sleep 150ms
Type "3dd"
Sleep 150ms
Type@60ms "/^>>>>>>>"
Enter
Sleep 150ms
Type "dd"
Sleep 250ms
Type ":wq"
Enter
# Back in the resolved conflict dialog — hold so it can be read.
Sleep 2.5s

# ── Continue: git-tailor lands the rebase with the conflict resolved ───────
Enter
Sleep 2.5s

# ── Done: a clean, merge-ready history ─────────────────────────────────────
Type "q"
Sleep 800ms
