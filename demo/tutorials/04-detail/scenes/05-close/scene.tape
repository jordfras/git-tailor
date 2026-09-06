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

# Everything up to here happens off camera. A chapter should open on the thing
# it is about, not on gt starting for the fifth time -- and the separator has to
# be moved before the capture starts, or every scene begins by widening itself.
Type "gt"
Enter
Sleep 2s

# The commit worth reading: four files in one idea, second from the bottom.
Type "G"
Sleep 200ms
Up@600ms 1
Sleep 500ms

# Ctrl-left walks the separator left. The diff is what this video is about, so
# it gets the room; the commit list only has to stay readable.
Ctrl+Left
Ctrl+Left
Ctrl+Left
Ctrl+Left
Ctrl+Left
Ctrl+Left
Ctrl+Left
Ctrl+Left
Sleep 500ms
# Open on the diff: this chapter is about moving around inside it.
Enter
Sleep 800ms
Show

Sleep 3s

# d does nothing here -- no confirmation, no dialog, nothing.
Type "d"
Sleep 2.5s
Type "s"
Sleep 3.5s

# And h says why: the same key that lists every operation in the commit list
# lists none of them here. The overlay follows the view it was opened from.
#
# Walked to the bottom rather than held at the top: the list is longer than the
# dialog, so a still frame of it is a claim about the half that is showing. The
# presses past the end are harmless -- the scroll clamps -- and buy a beat to
# read the tail.
Type "h"
Sleep 2.6s
Down@180ms 16
Sleep 1.0s
Type "h"
Sleep 2.9s

# Closed, the same key opens the drop confirmation. Escape, since dropping it
# is not the point.
Type "q"
Sleep 1.7s
Type "d"
Sleep 3s
Escape
Sleep 5s
