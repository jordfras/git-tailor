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
Show

Type "gt"
Sleep 300ms
Enter
Sleep 2.5s

# The tree is dirty, and the drop runs anyway: the changes are set aside and
# brought back. The target is the last commit, so nothing is replayed after it
# and no conflict takes over a scene that is not about one.
Type "G"
Sleep 200ms
Up@600ms 2
Sleep 1.5s
Type "d"
Sleep 1.5s
Enter
Sleep 3s
Type "q"
Sleep 400ms

# With the stash kept out of it, the same operation is refused, and the message
# says why. The rows above have shifted by the drop; refused either way.
#@cue 2
Type "gt --no-stash"
Sleep 300ms
Enter
Sleep 1.5s
Type "G"
Sleep 200ms
Up@600ms 2
Sleep 600ms
Type "d"
Sleep 1.5s
Enter
Sleep 4s
