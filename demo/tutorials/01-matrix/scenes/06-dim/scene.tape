# Scene 06 — dim, which is the same idea in both panes.
#
# The documentation commit shares no code with anything, so nothing is related
# to it: the two commits that can still fold somewhere show up dim green, and
# the rest go plain. That contrast is the whole point of the scene, and it only
# exists from a selection that is related to nothing.

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

Type "g"
Sleep 200ms
Down@600ms 6
Sleep 14.0s

# Back to a commit that *is* entangled, under the closing line.
Up@600ms 5
Sleep 10.6s
