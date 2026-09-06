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

# The two rows sit above the commits; the cursor walks onto them so the matrix
# redraws around a working-tree row rather than a commit.
Type "G"
Sleep 4s
Up@700ms 1
Sleep 7s
Down@700ms 1
Sleep 6s

# Up into the commits and back, so the matrix redraws around a commit and then
# around a row again while the line about tracked files runs. The narration
# grew; without the motion the picture just sits here.
Up@700ms 3
Sleep 5s
Down@700ms 3
Sleep 3s
