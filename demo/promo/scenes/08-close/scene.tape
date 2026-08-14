# Scene 08 — the close. Both ways to get it, on screen together: cargo, or a
# pre-built binary for people who do not have a Rust toolchain and should not
# need one.

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
# Fast, because the narration reaches the binaries line at 5.4s and at 110ms a
# 69-character comment took until 14.6s to appear — the screen trailing the
# voice by most of a sentence. The holds below absorb the time saved, so the
# scene still runs its 24.9s and the video does not shorten.
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
# command stays visible while the second route is typed underneath it — both
# ways of getting the tool on screen at once.
Ctrl+c
Sleep 1.4s

#@cue 3 +0.10
Type "# no Rust toolchain? pre-built binaries for Linux, macOS and Windows:"
Enter
Sleep 500ms
Type "# github.com/jordfras/git-tailor/releases/latest"
Enter
# Both routes are on screen by 12.5s, just before the voice finishes saying so.
# The rest of the hold carries the sign-off.
Sleep 12.4s
