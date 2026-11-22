# Scene 08 — the close. Every way to get it, on screen together: cargo, and for
# people who do not have a Rust toolchain and should not need one, Homebrew or a
# pre-built binary.

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
# Fast, because the narration moves on to the next route within a few seconds,
# and at 110ms a 69-character comment took 7.6s to appear — the screen trailing
# the voice by most of a sentence. The final hold absorbs the time saved.
Set TypingSpeed 55ms

Hide
Type 'cd "${DEMO_REPO:-/tmp/demo-repo}"'
Enter
Type 'clear'
Enter
Show

Type "cargo install --locked git-tailor"
Sleep 2.2s

# Ctrl-C rather than Enter: a real install takes minutes and prints nothing
# worth watching. It abandons the line without clearing the screen, so the
# command stays visible while the next route is typed underneath it — every way
# of getting the tool on screen at once.
Ctrl+c
Sleep 1.4s

# Typed out just before the voice reaches "Homebrew".
#@cue 3 +0.10
Type "brew install jordfras/tap/git-tailor"
Sleep 1.0s
Ctrl+c
Sleep 1.18s

#@cue 4 +0.10
Type "# or pre-built binaries for Linux, macOS and Windows:"
Enter
Sleep 500ms
Type "# github.com/jordfras/git-tailor/releases/latest"
Enter
# Every route is on screen by 15.9s, just before the voice finishes saying so.
# The rest of the hold carries the sign-off.
Sleep 11.13s
