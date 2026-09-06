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

# Held until the chapter title has cleared, so the caption can follow the
# keypress it describes instead of arriving seconds later.
Sleep 4.5s
Type "/"
Sleep 500ms
Type "timeout"
Sleep 1.2s
Enter
Sleep 1.2s
Type "n"
Sleep 1.1s
Type "n"
Sleep 1.1s
Type "n"
Sleep 1.1s
Escape
Sleep 1.5s
