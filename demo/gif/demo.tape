# git-tailor README demo — a single, follow-along flow:
#   view an unstaged fix -> fold it into the commit it belongs to -> start a
#   fold that conflicts and abort it -> split that commit per hunk group and
#   fold each piece into its own commit -> drop a debug commit -> undo back to
#   the start. Operations are picked from the Space menu, so the GIF shows
#   where they live.
#
# Assumptions provided by the build (see demo/render.sh + demo/build.sh):
#   * `gt` (git-tailor) is on PATH
#   * $DEMO_REPO points at a freshly generated demo repo
#     (demo/gif/make-repo.sh), checked out on its feature branch
#
# Run from the repo root:  vhs demo/gif/demo.tape

Output demo/out/demo.gif

Require gt

Set Shell bash
Set FontSize 18
Set Width 1240
Set Height 500
# 24fps keeps the file small (vhs defaults to 50); post-render gifsicle trims
# the rest. See demo/render.sh.
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

# ── Inspect the unstaged fix ───────────────────────────────────────────────
# The cursor starts on the unstaged row, at the bottom.
Enter
Sleep 500ms
# Widen the detail pane until the longest diff line fits, then scroll down to
# the diff. The width outlives the detail view, so put it back before closing.
Ctrl+Left
Sleep 80ms
Ctrl+Left
Sleep 80ms
Ctrl+Left
Sleep 80ms
Ctrl+Left
Sleep 80ms
Ctrl+Left
Sleep 80ms
Ctrl+Left
Sleep 80ms
Ctrl+Left
Sleep 80ms
Ctrl+Left
Sleep 80ms
Ctrl+Left
Sleep 80ms
Ctrl+Left
Sleep 80ms
Sleep 1s
Down@60ms 8
Sleep 3s
Ctrl+Right
Sleep 40ms
Ctrl+Right
Sleep 40ms
Ctrl+Right
Sleep 40ms
Ctrl+Right
Sleep 40ms
Ctrl+Right
Sleep 40ms
Ctrl+Right
Sleep 40ms
Ctrl+Right
Sleep 40ms
Ctrl+Right
Sleep 40ms
Ctrl+Right
Sleep 40ms
Ctrl+Right
Sleep 40ms
Sleep 400ms
Type "q"
Sleep 700ms

# ── Fold it into the REPL commit it belongs to ─────────────────────────────
# Menu: Squash, Fixup, ... The picker opens on the unstaged row; six rows up
# is the commit that owns its column.
Space
Sleep 900ms
Down
Sleep 500ms
Enter
Sleep 700ms
Up@150ms 6
Sleep 1.5s
Enter
Sleep 1.6s

# ── Fold a fix commit whose fold conflicts, and back out of it ─────────────
# The fold left the cursor on the REPL commit; "fix: guard against division by
# zero" is five rows down; the matrix marks it red. Menu: Split, Squash,
# Fixup, ... Six rows up from it is the evaluator commit.
Down@150ms 5
Sleep 700ms
Space
Sleep 700ms
Down 2
Sleep 500ms
Enter
Sleep 700ms
Up@150ms 6
Sleep 1.5s
Enter
# The conflict dialog — hold so it can be read, then abort.
Sleep 3s
Escape
Sleep 1.8s

# ── Split it per hunk group, and fold each piece into its own commit ──────
# The abort left the cursor on the fix. Split is first in the menu, Per hunk
# group fourth in the split dialog.
Space
Sleep 700ms
Enter
Sleep 900ms
Down 3
Sleep 700ms
Enter
Sleep 1.6s
# The cursor lands on piece 2/2, the REPL line; six rows up is the REPL
# commit, which its column connects to.
Space
Sleep 700ms
Down 2
Sleep 500ms
Enter
Sleep 700ms
Up@150ms 6
Sleep 1.5s
Enter
Sleep 1.6s
# The fold left the cursor on the REPL commit; piece 1/2, the Div arm, is five
# rows down, and the evaluator commit six rows up from it.
Down@150ms 5
Sleep 700ms
Space
Sleep 700ms
Down 2
Sleep 500ms
Enter
Sleep 700ms
Up@150ms 6
Sleep 1.5s
Enter
Sleep 1.8s

# ── Drop the debug commit that should never be merged ──────────────────────
# The fold left the cursor on the evaluator commit; "DROPME: hack in debug
# tracing" is four rows down. Drop is sixth in the menu.
Down@150ms 4
Sleep 700ms
Space
Sleep 700ms
Down 5
Sleep 700ms
Enter
Sleep 900ms
Enter
Sleep 1.8s

# ── Undo everything: back to the original branch and working tree ──────────
# Five operations: the drop, both folds, the split and the unstaged fold; the
# aborted fold left nothing to undo. The first undo comes from the menu, where
# Undo is ninth for a commit, so the `u` used for the rest has been seen.
Space
Sleep 700ms
Down@120ms 8
Sleep 600ms
Enter
Sleep 1.4s
Type "u"
Sleep 1.2s
Type "u"
Sleep 1.2s
Type "u"
Sleep 1.2s
Type "u"
# Hold on the restored state so it can be read.
Sleep 2.5s

Type "q"
Sleep 800ms
