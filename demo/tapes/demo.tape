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
Set FontSize 16
Set Width 1100
Set Height 460
# Windows Terminal "Campbell" palette — the WSL default, and the same colours
# the project's screenshot generator (examples/gen_screenshot.rs) uses.
Set Theme { "name": "Campbell", "background": "#0C0C0C", "foreground": "#CCCCCC", "black": "#0C0C0C", "red": "#C50F1F", "green": "#13A10E", "yellow": "#C19C00", "blue": "#0037DA", "magenta": "#881798", "cyan": "#3A96DD", "white": "#CCCCCC", "brightBlack": "#767676", "brightRed": "#E74856", "brightGreen": "#16C60C", "brightYellow": "#F9F1A5", "brightBlue": "#3B78FF", "brightMagenta": "#B4009E", "brightCyan": "#61D6D6", "brightWhite": "#F2F2F2", "selection": "#3A96DD", "cursor": "#CCCCCC" }
Set Padding 20
Set TypingSpeed 60ms

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
Up@350ms 3
Sleep 700ms
Enter
Sleep 500ms
# Widen the detail pane (one step at a time), then scroll down to the diff.
Ctrl+Left
Sleep 350ms
Ctrl+Left
Sleep 350ms
Ctrl+Left
Sleep 350ms
Ctrl+Left
Sleep 1.2s
Down@60ms 14
Sleep 1.6s
Type "q"
Sleep 700ms

# ── Fold that fix into the parser commit it belongs to ─────────────────────
Type "f"
Sleep 900ms
Up@350ms 4
Sleep 700ms
Enter
Sleep 1.6s

# ── Drop the debug commit that should never be merged ──────────────────────
# The fixup left the cursor on the parser commit; step down to
# "DROPME: hack in debug tracing".
Down@350ms 4
Sleep 700ms
Type "d"
Sleep 900ms
Enter
Sleep 1.4s

# ── A later fix touched the same lines, so the drop conflicts. Resolve it ───
# (kept brief — the point is that git-tailor drops you straight into $EDITOR).
Type "e"
Sleep 1s
Type "/^<<<<<<<"
Enter
Sleep 250ms
Type "3dd"
Sleep 250ms
Type "/^>>>>>>>"
Enter
Sleep 250ms
Type "dd"
Sleep 250ms
Type ":wq"
Enter
Sleep 700ms

# ── Continue: git-tailor lands the rebase with the conflict resolved ───────
Enter
Sleep 2.5s

# ── Done: a clean, merge-ready history ─────────────────────────────────────
Type "q"
Sleep 800ms
