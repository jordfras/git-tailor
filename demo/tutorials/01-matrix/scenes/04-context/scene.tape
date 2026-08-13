# Scene 04 — the caveat, and it stays on the matrix.
#
# Not the diff view: that renders with three lines of context by default and +/-
# changes how much it shows, none of which has anything to do with how the
# matrix groups hunks. Putting it on screen here would suggest the grouping
# follows the context setting, which is exactly the wrong idea to plant.
#
# So the picture is the green pair the last scene established, and the narration
# does the work.

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

# The green commit from the last scene, so the caveat attaches to something the
# viewer has already been told is safe.
Type "g"
Sleep 200ms
Down@600ms 1
Sleep 8.0s

# And its partner, the commit it would fold into.
Up@700ms 1
Sleep 6.0s
Down@700ms 1
Sleep 8.0s

Down@700ms 1
Sleep 6.7s
