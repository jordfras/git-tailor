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
# This scene starts where the *fold* scene left off, so the fixture is rebuilt in
# its `after-folds` shape: the two green pieces are already in the commits they
# belonged with, and only the third is still its own commit. Every scene gets a fresh repository, so there is no other way to
# carry that state across.
#
# Rebuilt from outside it, and entered afterwards: FORCE=1 removes the directory
# and makes it again, so a shell already standing in it is left on a deleted
# inode and everything after fails with "No such file or directory".
Type 'cd /'
Enter
Type 'FORCE=1 /work/demo/tutorials/02-reshaping/make-repo.sh "${DEMO_REPO:-/tmp/demo-repo}" after-folds >/dev/null'
Enter
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

# The branch as scene 04 left it: the other two pieces are folded away, so the
# remaining one is five rows down rather than seven.
Type "g"
Sleep 200ms
Down@500ms 5
Sleep 5s
Type "r"
Sleep 2s
Type "cc"
Sleep 300ms
Type "feat: fail loudly when the port is taken"
Escape
Sleep 700ms
Type ":wq"
Enter
Sleep 5s
Type "g"
Sleep 1.5s
Down@600ms 5
Sleep 2.5s
