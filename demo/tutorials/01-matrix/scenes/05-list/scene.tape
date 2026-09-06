# Scene 05 — the two list colors that follow the selection.

Output frames/

Require gt

Set Shell bash
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

# gt starts off camera: a chapter should open on what it is about, not on the
# program starting up again. The settle after it stays on camera, so every
# marker and caption still lands where it was tuned to.
Type "gt"
Enter
Show
Sleep 2.5s

# A commit with a green partner above it.
Type "g"
Sleep 200ms
Down@600ms 1
Sleep 13.0s

# Then the entangled one, for the line about red.
Down@600ms 4
Sleep 3.4s
