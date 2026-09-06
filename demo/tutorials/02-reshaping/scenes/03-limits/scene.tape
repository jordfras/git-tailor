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

# The documentation commit touches one region, related to nothing. Per hunk
# group has nothing to work with, and the refusal is the point of the scene, so
# the message stays on screen while the narration lands.
Sleep 8s
Type "G"
Sleep 1.5s
Type "p"
Sleep 2s
Down@450ms 3
Sleep 1.5s
Enter
Sleep 7s
