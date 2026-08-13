#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 Thomas Johannesson
#
# Render one line of narration through several Kokoro voices so they can be
# compared by ear, both raw and through the broadcast processing chain the
# finished video uses.
#
# Kokoro has no style or emotion control, so an announcer read comes from the
# voice, the speed, and the processing together — judging a voice on raw TTS
# alone is misleading, hence both versions.
#
# Usage (inside the container):
#   audition-voice.sh [OUTDIR] [VOICE...]
set -euo pipefail

REPO=/work
out=${1:-$REPO/demo/out/audition}
shift || true

voices=("$@")
if [ ${#voices[@]} -eq 0 ]; then
    voices=(am_onyx am_fenrir am_eric am_puck bm_george bm_lewis am_santa)
fi

LINE=${AUDITION_LINE:-"There has to be a better way! There is. git-tailor. \
Select the commit, press p for split, pick the pieces. And that's not all!"}

BROADCAST=$("$REPO/demo/promo/scripts/broadcast-filter.sh")

mkdir -p "$out"
printf '%s\n' "$LINE" >"$out/line.txt"
echo ">> line: $LINE"
echo

for voice in "${voices[@]}"; do
    TTS_ENGINE=kokoro TTS_VOICE="$voice" TTS_SPEED="${TTS_SPEED:-1.1}" \
        "$REPO/demo/promo/scripts/tts.sh" "$out/line.txt" "$out/$voice-raw.wav" >/dev/null
    ffmpeg -hide_banner -loglevel error -y -i "$out/$voice-raw.wav" \
        -af "$BROADCAST" "$out/$voice-broadcast.wav"
    printf '   %-12s %ss\n' "$voice" \
        "$(ffprobe -v error -show_entries format=duration -of default=nw=1:nk=1 \
            "$out/$voice-raw.wav")"
done

if [ -n "${HOST_UID:-}" ] && [ -n "${HOST_GID:-}" ]; then
    chown -R "$HOST_UID:$HOST_GID" "$out"
fi

echo
echo ">> $out:"
ls -1 "$out"
