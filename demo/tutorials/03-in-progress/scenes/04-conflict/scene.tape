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
# This scene needs a clean tree: the guard that stops a rewrite over
# uncommitted changes is scene 05's subject, not this one's.
Type 'git reset -q --hard'
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

# Fold the last commit back into the one that introduced the warning, four rows
# up. The fixup! in between touched the same line, so the two disagree and the
# squash tree conflicts. Fixup rather than squash: this scene is about the
# conflict, not about a combined message.
Type "G"
Sleep 3s
Type "f"
Sleep 2s
Up@450ms 4
Sleep 1.5s
Enter
Sleep 3.5s

# The resolution dialog, resolved in vimdiff -- a real merge tool, running in
# the terminal where the recording can see it.
#
# git-tailor opens it as `vimdiff -d $LOCAL $MERGED $REMOTE`, so the panes are
# ours, the working file, theirs, and the cursor starts on the left. One window
# right is the file being written; `]c` jumps to the change, `:diffget 3` takes
# the incoming side, and `:wqa` saves and leaves.
#
# This move stops twice: first on the moved commit itself, then on a commit
# replayed on top of it. Both rounds are the same.
Type "m"
Sleep 3s
Ctrl+w
Type "l"
Sleep 1s
Type "]c"
Sleep 1.2s
Type ":diffget 3"
Enter
Sleep 1.5s
Type ":wqa"
Enter
Sleep 2s
Enter
Sleep 2s

Type "m"
Sleep 3s
Ctrl+w
Type "l"
Sleep 1s
Type "]c"
Sleep 1.2s
Type ":diffget 3"
Enter
Sleep 1.5s
Type ":wqa"
Enter
Sleep 2s
Enter
Sleep 3s

# The narration outlasts the resolution, so walk the result rather than holding
# a frozen frame: the commit that was folded in is gone from the list.
Type "g"
Sleep 2s
Down@600ms 2
Sleep 3s
