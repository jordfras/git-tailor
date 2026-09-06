# Scene 01 — what the matrix is made of. The tool opens and sits still;
# the narration does the work, so the only motion is the cursor stepping
# down the list once the rows have been named.

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
# program starting up again.
Type "gt"
Enter
Show
# Still while the list and the reading order are described — there is nothing
# to point at yet, and a cursor wandering under an explanation is a distraction.
Sleep 17.8s

# From "each row is a commit" onward the cursor steps, so the rows being named
# are the rows moving. The scene lasts as long as the narration either way; a
# tape that stops early just freezes on its last frame.
Down@700ms 1
Sleep 2.5s
Down@700ms 1
Sleep 3.3s
Down@700ms 1
Sleep 3.0s
Up@700ms 2
Sleep 4.1s
