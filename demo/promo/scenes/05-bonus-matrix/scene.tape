# Bonus 1 — the hunk group matrix. Nothing is changed; the cursor moves and the
# colouring re-paints relative to whatever is selected. The bundled commit is
# the interesting row: it lights up columns owned by two different commits,
# which is the reason it wanted splitting in the first place.

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
Type "gt"
Enter
Sleep 2s
Show

Sleep 1s
Type "g"
Sleep 300ms
Down@600ms 4
Sleep 2.2s
Up@600ms 3
Sleep 1.8s
Down@600ms 5
Sleep 2s
