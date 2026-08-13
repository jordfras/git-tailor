#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 Thomas Johannesson
#
# Synthesise placeholder music and sound effects with ffmpeg alone, so the promo
# pipeline can be built and proven end to end without first settling on licensed
# audio. These are deliberately plain — a tonal bed and two short cues — enough
# to verify levels, ducking and cue timing.
#
# To use real audio instead, drop files into demo/promo/assets/ with the same base
# names (music.*, whoosh.*, ding.*); compose.sh prefers those and skips these.
#
# Usage (inside the container):  make-audio-beds.sh OUTDIR
set -euo pipefail

out=${1:?usage: make-audio-beds.sh OUTDIR}
mkdir -p "$out"

ff() { ffmpeg -hide_banner -loglevel error -y "$@"; }

# Music bed: a plucked arpeggio over four chords — Am, F, C, G — as an 8s loop.
#
# Held sine tones were tried first and were a mistake: low partials under a
# lowpass have no attack and no harmonics, so they read as mains hum rather than
# as music, and anyone reviewing the video hears a fault. Notes with a decay
# envelope sound deliberate even when they are obviously synthetic, and the
# higher register keeps them clear of the voice.
#
# loudnorm pins the output at a known loudness so compose.sh's mix level means
# something: without it the bed's level is an accident of the gains here, and a
# real track dropped into demo/promo/assets/ would sit somewhere else again.
NOTE_LEN=1.1  # seconds, so tails overlap the next note
STEP=0.5      # seconds between notes
# The last note is still decaying when the 8s loop wraps, so the ends get a
# short fade to keep the seam from clicking on every repeat.
# Four bars of four: Am, F, C, G.
NOTES=(
    220.00 261.63 329.63 261.63
    174.61 220.00 261.63 220.00
    130.81 164.81 196.00 164.81
    196.00 246.94 293.66 246.94
)

inputs=()
filters=""
labels=""
for i in "${!NOTES[@]}"; do
    inputs+=(-f lavfi -i "sine=frequency=${NOTES[$i]}:duration=$NOTE_LEN")
    at=$(awk "BEGIN{printf \"%d\", $i * $STEP * 1000}")
    # Exponential decay gives the pluck; the gentle lowpass takes the edge off
    # the sine's harshness without hollowing the note out.
    filters="$filters[$i:a]afade=t=out:st=0:d=$NOTE_LEN:curve=exp,"
    filters="$filters""lowpass=f=2400,volume=0.5,adelay=$at|$at[p$i];"
    labels="$labels[p$i]"
done

ff "${inputs[@]}" \
   -filter_complex "\
     $filters\
     $labels""amix=inputs=${#NOTES[@]}:normalize=0,\
     aecho=0.8:0.6:220|340:0.28|0.18,\
     atrim=0:8,asetpts=N/SR/TB,\
     afade=t=in:st=0:d=0.08,afade=t=out:st=7.9:d=0.1,\
     loudnorm=I=-20:TP=-2[m]" \
   -map "[m]" -ar 48000 -ac 2 "$out/music.wav"

# Whoosh: a rising chirp, used on scene transitions.
#
# The first version was a 1.2s burst of brown noise behind a *fixed* highpass —
# the comment claimed a sweep, but nothing swept. Filtered noise holding one
# band for over a second is just static, and firing it a beat before each
# narration made the video sound broken. What makes a whoosh a whoosh is the
# movement, so this is an explicit frequency ramp (300Hz climbing quadratically)
# rather than noise, and short enough to read as punctuation.
ff -f lavfi -i "aevalsrc='0.5*sin(2*PI*(300*t+1500*t*t))':d=0.55:s=48000" \
   -filter_complex "\
     [0:a]afade=t=in:st=0:d=0.08:curve=exp,\
     afade=t=out:st=0.15:d=0.4:curve=log,\
     aecho=0.9:0.6:60:0.2,\
     volume=0.7,\
     aformat=channel_layouts=stereo[w]" \
   -map "[w]" -ar 48000 "$out/whoosh.wav"

# Ding: a struck bell — fundamental plus a quiet octave, exponential decay.
ff -f lavfi -i "sine=frequency=880:duration=1.5" \
   -f lavfi -i "sine=frequency=1760:duration=1.5" \
   -filter_complex "\
     [0:a]volume=0.8[d0];\
     [1:a]volume=0.25[d1];\
     [d0][d1]amix=inputs=2:normalize=0,\
     afade=t=out:st=0:d=1.5:curve=exp,\
     aformat=channel_layouts=stereo[d]" \
   -map "[d]" -ar 48000 "$out/ding.wav"

# Smack: the impact under the logo landing. A bright crack for the transient
# plus a low thump for the weight, both decaying fast — a comic-book hit, not a
# drum. Anything with a tail reads as a crash instead of a landing.
ff -f lavfi -i "anoisesrc=color=white:duration=0.4:amplitude=0.9" \
   -f lavfi -i "sine=frequency=105:duration=0.4" \
   -filter_complex "\
     [0:a]highpass=f=500,lowpass=f=7000,\
     afade=t=out:st=0:d=0.16:curve=exp,volume=0.9[crack];\
     [1:a]afade=t=out:st=0:d=0.3:curve=exp,volume=0.8[thump];\
     [crack][thump]amix=inputs=2:normalize=0,\
     alimiter=limit=0.95,\
     aformat=channel_layouts=stereo[s]" \
   -map "[s]" -ar 48000 "$out/smack.wav"

echo ">> audio beds in $out:"
ls -1 "$out"
