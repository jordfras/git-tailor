# Scene 01 — the dream: the branch you would enjoy reviewing.
#
# Plain git, no git-tailor. The tool has not been introduced yet, and the point
# of this scene is the shape of a tidy history, not any way of producing one.
#
# Subjects only, no SHAs, so this is character-for-character what scene 04
# arrives at — the fixture's `tidy` branch is verified to be exactly what the
# cleanup produces, trees and all.

Output frames/

Set Shell bash
# A terminal with some color in it, so the gray scenes have something to lose.
# Black-on-white terminals desaturate to almost exactly themselves, which left
# the "problem" scenes looking the same as the rest.
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
Show

Type "git log --format='%s' main..tidy"
Sleep 400ms
Enter
Sleep 9s
