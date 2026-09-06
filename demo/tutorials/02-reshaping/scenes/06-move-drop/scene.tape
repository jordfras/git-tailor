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

# The documentation commit, last on the branch, moved to the front. It touches
# only the README, so nothing it passes can conflict with it.
Type "G"
Sleep 2.5s
Type "m"
Sleep 2s
Up@450ms 6
Sleep 1.5s
Enter
Sleep 4s

# The debugging commit is now the sixth row; drop it.
Type "g"
Sleep 200ms
Down@500ms 5
Sleep 2s
Type "d"
Sleep 1.5s
Enter
Sleep 4s
