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

# The unstaged row is the last row. Fixup it into the commit that owns its
# column -- the first commit on the branch.
Type "G"
Sleep 6s
Type "f"
Sleep 2.5s
# The picker opens on the source row, and confirming there is refused as
# squashing a commit into itself. Seven rows up is the commit that owns the
# column -- past the staged row and every commit on the branch but the first.
# Count this again if the fixture gains a commit: it silently folds into the
# wrong one otherwise.
Up@400ms 7
Sleep 2s
Enter
Sleep 3.5s

# What is left: the staged row alone, still staged.
Type "G"
Sleep 7s
