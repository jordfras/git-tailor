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
Show

Type "gt"
Sleep 300ms
Enter
Sleep 2.5s

# Two drops, then one undo -- leaving the other still done when the program
# quits, so the undo after the restart has something from before it to take
# back. Undoing both first would leave an empty stack and prove nothing.
Type "G"
Sleep 1.5s
Type "d"
Sleep 1.2s
Enter
Sleep 2.5s
Type "G"
Sleep 800ms
Type "d"
Sleep 1.2s
Enter
Sleep 2s

Type "u"
Sleep 2.5s

# Quit and come back: the stack is in the repository, not in memory.
Type "q"
Sleep 1.5s
Type "gt"
Sleep 300ms
Enter
Sleep 3s
Type "u"
Sleep 5s

# The caveat about the branch moving runs past the undo itself; walk the
# restored list under it.
Type "g"
Sleep 1.5s
Down@600ms 3
Sleep 3.5s
