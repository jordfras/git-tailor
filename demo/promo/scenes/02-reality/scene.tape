# Scene 02 — the reality: the same project, the branch you actually have.
#
# Still plain git. Two problems are visible in the log — a WIP commit that
# should never be merged, and "fix: handle parentheses", which the --stat
# reveals to be two fixes in one commit, touching two different files.

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
# Faster than the other scenes on purpose. The scene's length is set by the
# narration, not by the tape, so every millisecond spent watching characters
# appear is a millisecond not spent looking at the log — which is the whole
# point of the scene. Typing at 60ms buys two seconds of reading time.
Set TypingSpeed 60ms

Hide
Type 'cd "${DEMO_REPO:-/tmp/demo-repo}"'
Enter
Type 'clear'
Enter
Show

Type "git log --format='%s' main..HEAD"
Sleep 300ms
Enter
Sleep 2.0s

# The bundled commit, opened up. The narrator says "And this." from 5.4s and is
# done with it by 6.0, which is when this has to run — the reveal is the
# punctuation on the phrase. The log above stays on screen meanwhile, since the
# new command simply types underneath it.
Type "git show --stat HEAD~3"
Sleep 300ms
# Late on purpose: the reveal is the punctuation on "And this.", so it runs as
# the phrase lands rather than as it starts.
#@cue 3 +0.53
Enter
Sleep 8.1s
