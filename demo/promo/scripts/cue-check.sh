#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 Thomas Johannesson
#
# Check that each keystroke a tape declares a cue for still lands just after the
# narration line that calls for it — and say by how much it does not.
#
# Some keystrokes have to follow the voice: the narrator says "press p" and the
# key is pressed a moment later. Nothing in a tape expresses that. A tape holds
# `Sleep 6.98s`, a number that was correct against one recording of one script,
# and editing a single word anywhere earlier in the narration silently moves
# every line after it — so the keystrokes drift out from under the voice with
# nothing failing and nothing to notice until someone watches the result.
#
# So a tape declares its cues, and this reports the drift:
#
#     #@cue 4 +0.05
#     Type "p"
#
# means "this command should start 0.05s after narration line 4 begins".
# Lines are counted from 1 over the spoken lines only — blank lines, comments,
# `[0.9]` gaps and `{speed=…}` directives do not count. The lead is optional and
# defaults to +0.05.
#
# The leads recorded in the tapes are the ones the video was tuned with, not an
# ideal: they run from 0.05s to 0.6s because a keystroke may land anywhere
# inside the sentence that calls it. This tool is for keeping what was tuned,
# not for flattening it.
#
# Usage (inside the container):  cue-check.sh VIDEO [SCENE_NAME...]
#     demo/build.sh cues                        # the promo, every scene
#     demo/build.sh cues promo 04-pitch         # one of them
#     demo/build.sh cues tutorials/01-matrix    # another video
#
# Exits non-zero if any cue has drifted more than TOLERANCE seconds, so it can
# gate a render.
set -euo pipefail

REPO=/work
cd "$REPO"

VIDEO=${1:?usage: cue-check.sh VIDEO [SCENE_NAME...]}
shift
VIDEO_DIR=$REPO/demo/$VIDEO
SCENES_DIR=$VIDEO_DIR/scenes
SCRIPT_DIR=$REPO/demo/promo/scripts
[ -d "$SCENES_DIR" ] || {
    echo "cue-check: no video at demo/$VIDEO" >&2
    exit 1
}

# Timings must come from the voice the video is actually narrated with, so the
# same per-video settings compose.sh reads apply here.
# shellcheck disable=SC1090
[ -f "$VIDEO_DIR/video.conf" ] && . "$VIDEO_DIR/video.conf"
export TTS_VOICE TTS_SPEED
TOLERANCE=${TOLERANCE:-0.15}

scenes=("$@")
if [ ${#scenes[@]} -eq 0 ]; then
    for dir in "$SCENES_DIR"/*/; do scenes+=("$(basename "$dir")"); done
fi

status=0
total_cues=0

for name in "${scenes[@]}"; do
    dir=$SCENES_DIR/$name
    tape=$dir/scene.tape
    [ -f "$tape" ] || {
        echo "cue-check: no such scene '$name'" >&2
        exit 1
    }

    # Cues are the only reason to read the narration, so a tape without any
    # costs nothing — skip before paying for synthesis.
    grep -q '^[[:space:]]*#@cue' "$tape" || continue

    NARRATION_DELAY=0.8 SPEED=1
    # shellcheck disable=SC1090
    [ -f "$dir/scene.conf" ] && . "$dir/scene.conf"

    timings=$("$SCRIPT_DIR/tts.sh" --timings "$dir/narration.txt")
    positions=$("$SCRIPT_DIR/tape-duration.sh" --positions "$tape")

    echo "$name"
    # Join the three sources in awk: the declared cues (from the tape), the
    # command positions (from the parser) and the line starts (from the voice).
    out=$(awk -v delay="$NARRATION_DELAY" -v speed="$SPEED" -v tol="$TOLERANCE" \
        -v timings="$timings" -v positions="$positions" '
    BEGIN {
        n = split(timings, trows, "\n")
        for (i = 1; i <= n; i++) {
            if (trows[i] ~ /^[ \t]*[0-9]/) {
                voice[++vn] = trows[i] + 0
            }
        }
        n = split(positions, prows, "\n")
        for (i = 1; i <= n; i++) {
            if (prows[i] == "") continue
            split(prows[i], p, "\t")
            pn++
            pos_at[pn] = p[1] + 0
            pos_line[pn] = p[2] + 0
            pos_text[pn] = p[3]
        }
    }
    # The tape itself is the input: find the cue markers and the command each
    # one introduces.
    /^[ \t]*#@cue/ {
        line = $0
        sub(/^[ \t]*#@cue[ \t]*/, "", line)
        split(line, c, /[ \t]+/)
        want_line = c[1] + 0
        lead = (c[2] == "") ? 0.05 : c[2] + 0

        # The command this cue introduces is the first visible one below it.
        cmd_at = ""
        for (i = 1; i <= pn; i++) {
            if (pos_line[i] > NR) {
                cmd_at = pos_at[i]
                cmd_text = pos_text[i]
                break
            }
        }
        if (cmd_at == "") {
            printf "  line %d: cue %d has no command after it\n", NR, want_line
            bad++
            next
        }
        if (want_line > vn) {
            printf "  line %d: cue %d but the narration has only %d lines\n", NR, want_line, vn
            bad++
            next
        }

        # SPEED fast-forwards the capture, so tape seconds are not scene
        # seconds; the voice is laid over the scene, never over the tape.
        actual = cmd_at / speed
        want = voice[want_line] + delay
        drift = actual - want - lead
        flag = (drift < 0 ? -drift : drift) > tol ? "  <-- DRIFTED" : ""
        if (flag != "") bad++
        printf "  cue %-2d %-22s voice %6.2f  key %6.2f  lead %+.2f (want %+.2f)  drift %+.2f%s\n",
               want_line, substr(cmd_text, 1, 22), want, actual, actual - want, lead, drift, flag
        cues++
    }
    END {
        printf "@@ %d %d\n", cues, bad
    }
    ' "$tape")

    echo "$out" | grep -v '^@@'
    counts=$(echo "$out" | sed -n 's/^@@ //p')
    total_cues=$((total_cues + ${counts%% *}))
    [ "${counts##* }" = "0" ] || status=1
done

if [ "$total_cues" = 0 ]; then
    echo "cue-check: no cues declared in the selected scenes"
elif [ "$status" = 0 ]; then
    echo "cue-check: $total_cues cue(s) within ${TOLERANCE}s"
else
    echo "cue-check: drift beyond ${TOLERANCE}s — retime the Sleep above each"
    echo "           drifted command, then re-render." >&2
fi
exit $status
