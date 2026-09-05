# Scene 00 — the title card. Nothing happens in the terminal: the picture is
# the logo and the title, overlaid by compose.sh, on an empty prompt.

Output frames/

Set Shell bash
Set Theme "Cobalt2"
Set FontSize 26
Set Width 1920
Set Height 1080
Set Framerate 30
Set Padding 20
Set TypingSpeed 90ms

Hide
# An empty prompt, so the card is the only thing on screen. Hiding the terminal
# instead is not an option: vhs captures no frames while hidden, and a scene
# with no frames has nothing to compose.
Type 'PS1=""'
Enter
# tput civis hides the cursor, which otherwise sits in the corner as a block.
# Both it and the prompt have to go: a title card should have nothing on it but
# the card.
Type 'clear; tput civis'
Enter
Show

Sleep 7s
