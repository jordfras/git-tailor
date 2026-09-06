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
# This scene starts where the split scene left off, so the fixture is rebuilt in
# its `after-split` shape: commit 6 is already the three commits that split
# produced. Every scene gets a fresh repository, so there is no other way to
# carry that state across.
#
# Rebuilt from outside it, and entered afterwards: FORCE=1 removes the directory
# and makes it again, so a shell already standing in it is left on a deleted
# inode and everything after fails with "No such file or directory".
Type 'cd /'
Enter
Type 'FORCE=1 /work/demo/tutorials/02-reshaping/make-repo.sh "${DEMO_REPO:-/tmp/demo-repo}" after-split >/dev/null'
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

# Piece 1 of 3, five rows down, squashed into the commit that last touched its
# column -- four rows up, "bind to loopback", not the port commit below it.
# Folding past a commit that touched the same lines is what the matrix draws
# red, and it conflicts.
Type "g"
Sleep 200ms
Down@500ms 5
Sleep 2s
Type "s"
Sleep 2s
Up@400ms 4
Sleep 1.5s
Enter
Sleep 2.5s
# Both messages arrive stacked; replace them with one that describes what the
# joined commit actually does, which is the opposite of what the target's own
# message said.
Type "ggVGd"
Sleep 400ms
Type "i"
Type "fix: bind to every interface, not just loopback"
Escape
Sleep 700ms
Type ":wq"
Enter
Sleep 2.5s

# Piece 2 of 3 is now where piece 1 was; fixup into "send warnings to stderr",
# three rows above it.
Type "g"
Sleep 200ms
Down@500ms 5
Sleep 2s
Type "f"
Sleep 2s
Up@400ms 3
Sleep 1.5s
Enter
Sleep 3.5s

# The narration runs past the fold; walk the two commits that took the pieces
# rather than holding a still frame.
Type "g"
Sleep 1.5s
Down@600ms 2
Sleep 3s
