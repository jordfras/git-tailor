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
Type 'git reset -q --hard'
Enter
Type 'clear'
Enter

# The crash. Nothing can be typed into the shell while gt owns the terminal, so
# the kill is armed before gt starts -- and it waits for the conflict to exist
# rather than for a wall clock, so it cannot fire before there is anything to
# recover. Unmerged entries in the index are what a stopped fold leaves behind,
# and `git ls-files -u` lists them.
#
# SIGKILL rather than Ctrl+C: Ctrl+C aborts the rebase on the way out and leaves
# nothing behind, which is the opposite of what this chapter is about. The
# escape sequences put the terminal back on the normal screen with a cursor,
# which is what a shell inherits after the program it was running dies.
Type '( while [ -z "$(git ls-files -u)" ]; do sleep 0.2; done; sleep 10.5; pkill -9 -x gt; sleep 0.4; printf "\033[?1049l\033[?25h" > /dev/tty ) &'
Enter
Type 'clear'
Enter

Type "gt"
Enter
Sleep 2s

# The same fold as the conflict chapter, off camera: this chapter opens on the
# stopped operation, not on the operation being started.
Type "G"
Sleep 500ms
Type "f"
Sleep 700ms
Up@250ms 4
Sleep 500ms
Enter
Sleep 2.5s
Show

# The conflict dialog, then the process dies under it.
Sleep 8.5s

# The shell it fell back to.
Sleep 4.5s
Type "gt"
Enter
Sleep 12s

# Resume: the same dialog, from the same place.
Enter
Sleep 6.5s

# And out, leaving the branch as it was before the fold: the fixup! commits are
# back where they were. Walked rather than held, so the last ten seconds are not
# one frozen frame while the narration finishes.
Escape
Sleep 5s
Type "g"
Sleep 2.5s
Down@700ms 4
Sleep 3.5s
