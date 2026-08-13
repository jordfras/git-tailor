#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 Thomas Johannesson
#
# Print how many seconds of *visible* action a vhs tape scripts.
#
# Needed because a capture's frame count does not tell you how long the capture
# was. At 1080p vhs samples the terminal more slowly than the frame rate it
# claims, and how much more slowly depends on machine load — the same scene tape
# has yielded anywhere from 308 to 673 frames here. Encoding those at the
# nominal rate makes the video play too fast, by a factor that changes run to
# run. The tape, by contrast, says exactly how long the scene is meant to take,
# so compose.sh stretches whatever frames it got across that duration.
#
# Commands inside a Hide/Show block are excluded: vhs does not capture frames
# while output is hidden.
#
# Usage:  tape-duration.sh [--positions] SCENE.tape
#
# With --positions, print one row per visible command instead of the total:
#
#     START_SECONDS <TAB> TAPE_LINE_NUMBER <TAB> COMMAND
#
# where START is when the command begins, in tape seconds — before any SPEED
# fast-forward, which is a property of the scene rather than of the tape.
# cue-check.sh uses this to compare a keystroke against the narration line it is
# supposed to follow.
set -euo pipefail

positions=0
if [ "${1:-}" = "--positions" ]; then
    positions=1
    shift
fi
tape=${1:?usage: tape-duration.sh [--positions] SCENE.tape}

awk -v positions="$positions" '
function secs(v) {
    if (v ~ /ms$/) { sub(/ms$/, "", v); return v / 1000 }
    if (v ~ /s$/)  { sub(/s$/, "", v);  return v + 0 }
    return v + 0
}
# The delay a command carries: "@250ms" if given, otherwise TypingSpeed.
function rate(word,   at) {
    at = index(word, "@")
    return at ? secs(substr(word, at + 1)) : typing
}
function emit(text) {
    if (positions) printf "%.3f\t%d\t%s\n", total, NR, text
}
BEGIN { typing = 0.05; hidden = 0; total = 0 }
{
    line = $0
    sub(/^[ \t]+/, "", line)
    if (line ~ /^#/ || line == "") next

    split(line, f, /[ \t]+/)
    cmd = f[1]
    bare = cmd
    sub(/@.*$/, "", bare)

    if (bare == "Hide") { hidden = 1; next }
    if (bare == "Show") { hidden = 0; next }
    if (bare == "Set") {
        if (f[2] == "TypingSpeed") typing = secs(f[3])
        next
    }
    if (hidden) next

    if (bare == "Sleep") { emit(line); total += secs(f[2]); next }

    if (bare == "Type") {
        # Everything between the first and last quote is the typed text; each
        # character costs one keystroke.
        q = substr(line, index(line, f[2]))
        body = substr(q, 2, length(q) - 2)
        gsub(/[ \t]+$/, "", body)
        emit(line)
        total += length(body) * rate(cmd)
        next
    }

    # Anything else is a key press, optionally repeated: "Up@700ms 4".
    n = (f[2] ~ /^[0-9]+$/) ? f[2] : 1
    emit(line)
    total += n * rate(cmd)
}
END { if (!positions) printf "%.3f\n", total }
' "$tape"
