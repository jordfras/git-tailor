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
# Edit spawns $SHELL, falling back to /bin/sh -- which is dash here, and dash
# prints vhs's bash-style prompt escapes literally: the viewer sees
# \[\e[38;2;90;86;224m\]> where a prompt should be. Naming bash fixes that but
# lands on bash's own default, which is `root@<container id>` -- a hostname
# nobody watching should have to look at. So: bash, with a prompt of its own and
# no rc files to overwrite it.
Type 'printf "#!/bin/bash\nexec env PS1=\"> \" bash --norc -i\n" >/tmp/demo-shell; chmod +x /tmp/demo-shell'
Enter
Type 'export SHELL=/tmp/demo-shell'
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

# Drop into the shell on the logging commit, make a real change, and exit.
Type "g"
Sleep 200ms
Down@600ms 2
Sleep 1.2s
Type "E"
Sleep 2s
Type "sed -i s/warning/warn/ src/log.rs"
Enter
Sleep 1s
Type "git commit -aqm x --amend --no-edit"
Enter
Sleep 1s
Type "exit"
Enter
Sleep 2.5s
