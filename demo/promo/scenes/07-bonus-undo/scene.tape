# Bonus 3 — undo and redo. Drops HEAD, which nothing sits on top of, so the
# drop is clean and the beat stays about the undo.

Output frames/

Require gt

Set Shell bash
# A terminal with some color in it, so the gray scenes have something to lose.
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
Type "gt"
Enter
Sleep 2s
Show

Sleep 800ms

Type "d"
Sleep 1.2s
Enter
Sleep 2.2s

Type "u"
Sleep 2.4s

Ctrl+r
Sleep 2.2s
