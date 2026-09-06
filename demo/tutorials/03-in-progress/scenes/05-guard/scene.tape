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

# The tree is dirty, so a drop is refused, and the message says why. The target
# is the last commit: nothing is replayed after it, so the run with the flag
# below succeeds instead of stopping on a conflict that is not this scene's
# subject.
Type "G"
Sleep 200ms
Up@600ms 2
Sleep 1.5s
Type "d"
Sleep 1.5s
Enter
Sleep 3.5s
Type "q"
Sleep 1.5s

# The same operation, with the flag.
Type "gt --autostash"
Sleep 300ms
Enter
Sleep 2s
Type "G"
Sleep 200ms
Up@600ms 2
Sleep 1s
Type "d"
Sleep 1.5s
Enter
Sleep 7s
