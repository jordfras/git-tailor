# Bonus 2 — move. "m" opens the position picker; the arrows choose where the
# commit lands and Enter drops it there.

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
Type "g"
Sleep 200ms
Down@450ms 6
Sleep 1.2s

Type "m"
Sleep 1.2s
Up@450ms 2
Sleep 1.2s
Enter
Sleep 3s
