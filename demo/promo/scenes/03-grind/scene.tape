# Scene 03 — the grind: splitting that one commit by hand.
#
# The rebase is real. vim opens on the real todo list, "pick" really becomes
# "edit", the reset really unstages the work, and `git add -p` really walks the
# hunks — the joke only lands if the tedium is genuine.
#
# This is only the *split*. The two fixups and the drop that follow in scene 04
# would each be another rebase; the narration says so rather than filming it.
#
# `git add -p` reads a whole line, not a keypress, so each answer needs its own
# Enter — without it the letters queue up on the prompt and the next command
# gets typed into it too.

Output frames/

Set Shell bash
# A terminal with some colour in it, so the grey scenes have something to lose.
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

Type "git rebase -i HEAD~4"
Enter
Sleep 2.5s

# vim, on the todo list: mark the bundled commit for editing.
Type "cwedit"
Escape
Sleep 800ms
Type ":wq"
Enter
Sleep 2s

Type "git reset HEAD~"
Enter
Sleep 2s

# Stage one concern, leave the other. Three hunks, not two: the lexer change
# lands in two places — the token enum and the match arm — and the parser
# change in one. Answering one prompt short leaves the next command to be
# typed into the prompt that is still waiting.
Type "git add -p"
Enter
Sleep 2s
Type "y"
Enter
Sleep 1.2s
Type "y"
Enter
Sleep 1.2s
Type "n"
Enter
Sleep 1.5s

Type 'git commit -m "fix: tokenize parentheses"'
Enter
Sleep 1.8s

Type "git add -A"
Enter
Sleep 600ms
Type 'git commit -m "fix: parse parenthesised expressions"'
Enter
Sleep 1.8s

Type "git rebase --continue"
Enter
Sleep 2.5s
