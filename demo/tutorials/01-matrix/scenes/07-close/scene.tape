# Scene 07 -- the close: the key list, and choosing what you are looking at.
#
# The only chapter in this tutorial that starts gt on camera, three times over.
# Everywhere else the launch is noise; here it is the subject, since what the
# matrix shows is decided by what gt is given. The key list moved to tutorial 2,
# where the keys it lists are the subject.

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

# This chapter starts gt three times on camera, which every other chapter avoids
# -- here the launch is the subject: what it shows depends on what it is given.
Type "gt"
Enter
Sleep 2s
Show

# Held on the default view while the narration says what that default is: your
# branch, back to where it left the default branch.
Sleep 12.6s

# The branch this one was cut from, so the list holds what this branch added and
# not what it inherited.
Type "q"
Sleep 1.4s
Type "gt feature/config"
Sleep 700ms
Enter
Sleep 6.7s

# And the whole history, down to the commit that created the files.
Type "q"
Sleep 1.2s
Type "gt --all"
Sleep 700ms
Enter
Sleep 7s
