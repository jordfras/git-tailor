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
Show

Type "gt"
Sleep 300ms
Enter
Sleep 1.6s

# A commit with a green partner above it.
Type "g"
Sleep 200ms
Down@600ms 1
Sleep 13.0s

# Then the entangled one, for the line about red.
Down@600ms 4
Sleep 3.4s
