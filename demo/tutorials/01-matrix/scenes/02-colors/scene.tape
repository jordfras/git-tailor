# Scene 02 — dimming, and the clean fold. One idea per scene: the connector and
# the red case follow next, rather than four concepts inside forty seconds.

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

# The pair with nothing in between: green, and on screen well before the line
# about it lands.
Type "g"
Sleep 200ms
Down@600ms 1
Sleep 14.8s
