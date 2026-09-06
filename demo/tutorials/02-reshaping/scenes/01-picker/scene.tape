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

# Down onto a commit worth operating on, then open the picker and leave it open:
# the narration is about what it lists, so it has to stay readable.
Type "g"
Sleep 200ms
Down@600ms 3
Sleep 1.5s
Space
Sleep 11.4s

# Then the other way of asking: the picker is what this row can do, the help
# overlay is every key this view has. Walked to the bottom rather than paged or
# held: the list is longer than the dialog, and a still frame of it is a claim
# about the half that is showing. The presses past the end are harmless -- the
# scroll clamps -- and buy a beat to read the tail.
Escape
Sleep 700ms
Type "h"
Sleep 1.6s
Down@180ms 22
Sleep 2.2s
Type "h"
Sleep 6s
