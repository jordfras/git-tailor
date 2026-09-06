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

Sleep 3.5s
Type "F"
Sleep 3.5s

# Rewrite the message the first target will end up with; the list marks it
# "(edited)" before anything moves. The dialog opens on that target, so the
# message typed below has to be its message, not another row's.
Sleep 1.2s
Type "r"
Sleep 2s
Type "cc"
Sleep 300ms
Type "fix: send warnings to stderr, not stdout"
Escape
Sleep 600ms
Type ":wq"
Enter
Sleep 3.5s

Enter
Sleep 7.5s
