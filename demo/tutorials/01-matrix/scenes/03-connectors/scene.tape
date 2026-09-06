# Scene 03 — the connector, then red. Both are about commits that are not
# adjacent, which is why they share a scene.

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

# "add an error level" shares the logging region with a commit two rows above
# it, so its connectors have to span them.
Type "g"
Sleep 200ms
Down@500ms 4
Sleep 8.6s

# And the one that belongs to two different commits at once: red.
Down@600ms 1
Sleep 9.7s
