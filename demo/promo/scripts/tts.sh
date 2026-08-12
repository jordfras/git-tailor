#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 Thomas Johannesson
#
# Render a narration script to speech. Every TTS engine hides behind this one
# interface so the promo pipeline never needs to know which one is in use — and
# so swapping in a different (or paid, API-backed) voice later is a change to
# this file alone.
#
# Usage:  tts.sh NARRATION.txt OUT.wav
#         tts.sh --timings NARRATION.txt      # where each line lands, no WAV
#
# --timings prints one row per narration line, "SECONDS  TEXT", measured with
# the same voice, speed and pause as a real render. Align a tape by these rather
# than by calling the engine directly: the engine's own defaults are a different
# voice at a different speed, so a hand-written invocation that forgets a flag
# reports times the finished video will not have. cue-check.sh uses this.
#
# Engine comes from $TTS_ENGINE:
#   kokoro  (default) — local neural voice, needs the model baked into the image
#   flite   — ffmpeg's built-in synthesiser; no extra dependencies, but robotic.
#             Useful for a fast pipeline smoke test.
set -euo pipefail

ENGINE=${TTS_ENGINE:-kokoro}
# bm_lewis: British, and dry enough that the infomercial script reads as a joke
# the narrator is in on rather than one played straight.
VOICE=${TTS_VOICE:-bm_lewis}
# Announcers rush; Kokoro's default is 1.0.
SPEED=${TTS_SPEED:-1.1}
# Seconds of silence between narration paragraphs that carry no [n] marker.
PAUSE=${TTS_PAUSE:-0.45}
KOKORO_HOME=${KOKORO_HOME:-/opt/tts}

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)

TIMINGS=0
if [ "${1:-}" = "--timings" ]; then
    TIMINGS=1
    shift
fi

script=${1:?usage: tts.sh [--timings] NARRATION.txt [OUT.wav]}
if [ "$TIMINGS" = 1 ]; then
    out=${2:-$(mktemp -u --suffix=.wav)}
else
    out=${2:?usage: tts.sh NARRATION.txt OUT.wav}
fi

[ -r "$script" ] || {
    echo "tts: cannot read $script" >&2
    exit 1
}
mkdir -p "$(dirname "$out")"

timings_flag=()
[ "$TIMINGS" = 1 ] && timings_flag=(--timings)

case "$ENGINE" in
kokoro)
    "$KOKORO_HOME/venv/bin/python" "$SCRIPT_DIR/kokoro_tts.py" \
        --text-file "$script" \
        --out "$out" \
        --voice "$VOICE" \
        --speed "$SPEED" \
        --pause "$PAUSE" \
        "${timings_flag[@]}"
    ;;
flite)
    [ "$TIMINGS" = 0 ] || {
        echo "tts: --timings needs TTS_ENGINE=kokoro (flite reports no timings)" >&2
        exit 1
    }
    # flite reads the file itself; it wants one line, so fold the wrapped
    # narration into a single paragraph first.
    tmp=$(mktemp)
    tr '\n' ' ' <"$script" | sed 's/  */ /g' >"$tmp"
    ffmpeg -hide_banner -loglevel error -y \
        -f lavfi -i "flite=textfile=$tmp:voice=slt" \
        -ar 24000 -ac 1 "$out"
    rm -f "$tmp"
    ;;
*)
    echo "tts: unknown TTS_ENGINE '$ENGINE' (want: kokoro, flite)" >&2
    exit 1
    ;;
esac

[ "$TIMINGS" = 1 ] || echo ">> narration: $(basename "$out") ($(ffprobe -v error \
    -show_entries format=duration -of default=nw=1:nk=1 "$out")s, $ENGINE)"
