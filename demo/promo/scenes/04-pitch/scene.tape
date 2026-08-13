# Scene 04 — the same cleanup in git-tailor, and the branch it lands on.
#
# Split the bundled fix, send each half to the commit it belongs to, drop the
# WIP commit. The log at the end is character-for-character scene 01.
#
# Navigation is absolute — `g` to the top, then down — rather than relative to
# wherever the cursor happens to sit. Every operation reloads the list and may
# move the selection, so counting from a known edge is what keeps this
# reproducible.
#
# Rows after the split (top is oldest):
#   1 feat: add token lexer          <- the lexer half belongs here
#   2 feat: add expression parser    <- the parser half belongs here
#   3 feat: add evaluator
#   4 feat: wire up the CLI
#   5 fix: handle parentheses (1/2)  <- lexer.rs
#   6 fix: handle parentheses (2/2)  <- parser.rs
#   7 WIP: leftover debug output
#
# The fixup target picker opens with the cursor on the *source* commit, not on
# the row above it, so reaching row 1 from row 5 is four steps up rather than
# three. Getting this off by one still produces a correct-looking branch — the
# same six subjects and the same final tree — because every change is still
# present, just attached to the wrong commit. Only a per-commit comparison
# catches it.

Output frames/

Require gt

Set Shell bash
# A terminal with some colour in it, so the grey scenes have something to lose.
# Black-on-white terminals desaturate to almost exactly themselves, which left
# the "problem" scenes looking the same as the rest.
Set Theme "Cobalt2"
Set FontSize 26
Set Width 1920
Set Height 1080
Set Framerate 30
Set Padding 20
Set TypingSpeed 90ms

Hide
Type 'cd "${DEMO_REPO:-/tmp/demo-repo}"'
Enter
Type 'clear'
Enter
Show

Type "gt"
Sleep 300ms
Enter
Sleep 2.5s

# ── Land on the bundled commit and hold ───────────────────────────────────
# The long hold is deliberate. The narration spends nine seconds here — "the
# matrix on the right already knows the story" — and the keystrokes below are
# timed so each one lands just after the narrator calls it, rather than the
# whole cleanup finishing before he says "press p". Line starts come from
# `kokoro_tts.py --timings`; add NARRATION_DELAY to get scene time.
Type "g"
Sleep 200ms
Down@350ms 4
Sleep 6.98s

# ── p: split the bundled fix, one commit per hunk group ── voice 11.8s ────
# "Per hunk group" is the fourth strategy, so the highlight has to travel —
# which is worth watching, and is why the pause before it is shorter than the
# one that used to sit here. The three Down presses and the sleeps around them
# add 0.38s, exactly the amount the longer narration line pushes everything
# after it back, so the rest of the tape needs no retiming.
#@cue 4 +0.06
Type "p"
Sleep 0.55s
Down@250ms 3
Sleep 0.6s
Enter
Sleep 0.78s

# ── f: the lexer half, into "add token lexer" ── voice 15.8s ──────────────
Type "g"
Sleep 150ms
Down@250ms 4
#@cue 5 +0.17
Type "f"
Sleep 1.0s
Up@350ms 4
Sleep 0.6s
Enter
Sleep 1.25s

# ── f: the parser half, into "add expression parser" ── voice 21.3s ───────
Type "g"
Sleep 150ms
Down@250ms 4
#@cue 6 +0.27
Type "f"
Sleep 1.0s
Up@350ms 3
Sleep 0.6s
Enter
Sleep 0.55s

# ── d: drop the WIP commit ── voice 25.6s ─────────────────────────────────
Type "g"
Sleep 150ms
Down@250ms 4
#@cue 7 +0.61
Type "d"
Sleep 1.2s
Enter
Sleep 1.5s

# ── The branch you wanted ── voice 29.5s ──────────────────────────────────
# Quitting and re-running the log is hidden: vhs captures no frames inside
# Hide, so the log simply appears rather than eating four seconds of typing
# the viewer has already watched twice.
Hide
Type "q"
Sleep 400ms
Type "clear"
Enter
Type "git log --format='%s' main..HEAD"
Enter
Show

Sleep 8s
