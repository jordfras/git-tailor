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
# it is about, not on gt starting for the fifth time.
Type "gt"
Enter
Sleep 2s

# The commit worth reading: four files in one idea, second from the bottom.
Type "G"
Sleep 200ms
Up@600ms 1
Sleep 500ms
Show

# Held until the chapter title has cleared, so the caption can follow the
# keypress it describes instead of arriving seconds later.
Sleep 4.5s
Enter

# The only chapter that widens the view on camera. The rest arrive already
# widened -- a tutorial should not open by rearranging itself five times -- so
# this is the one place the divider is explained. Slowly enough to see it move a
# step at a time.
Sleep 6.4s
Ctrl+Left
Sleep 500ms
Ctrl+Left
Sleep 500ms
Ctrl+Left
Sleep 500ms
Ctrl+Left
Sleep 500ms
Ctrl+Left
Sleep 500ms
Ctrl+Left
Sleep 500ms
Ctrl+Left
Sleep 500ms
Ctrl+Left
Sleep 500ms

# The voice finishes on the divider before the diff starts moving.
Sleep 6.4s
PageDown
Sleep 1.6s
PageDown
Sleep 1.5s
Type "G"
Sleep 1.2s
Type "g"
Sleep 900ms

# The narration says q closes it; show that rather than only saying it. The
# tail runs to the last caption's fade rather than ending under it.
Type "q"
Sleep 2.5s
