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

# The three-region commit is second from the bottom. Sit on it while the
# narration counts its squares, then split.
Type "G"
Sleep 200ms
Up@600ms 1
Sleep 9s

Type "p"
Sleep 2.5s
# Per hunk group is the fourth strategy in the menu.
Down@450ms 3
Sleep 2s
Enter
Sleep 5s

# The result: three rows where there was one. The last hold is for the line
# about the two green squares, so the matrix has to stay on screen.
Type "G"
Sleep 200ms
Up@600ms 2
Sleep 14s
Down@600ms 1
Sleep 4s
