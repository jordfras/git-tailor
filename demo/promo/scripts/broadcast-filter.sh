#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 Thomas Johannesson
#
# Print the ffmpeg filter chain that turns Kokoro's flat output into an
# announcer read. Kokoro has no style control, so the character of the voice
# comes from here: roll off the rumble, compress hard so every word lands at
# the same level, lift the chest and presence bands, and add just enough room
# that it sounds like a booth rather than a headset.
#
# Shared by compose.sh (which applies it to the narration) and audition-voice.sh
# (which needs to sound exactly like the finished video, or auditioning voices
# tells you nothing).
set -euo pipefail

# A note on acompressor: `makeup` is a linear multiplier, not decibels. An
# earlier version used makeup=3 — nearly +10dB — with a threshold down at -22dB,
# which compressed almost everything and dragged Kokoro's own noise floor up
# with it. The result was audible hiss that faded in with the voice and hung on
# after it, following the compressor's release. Compress only what is actually
# loud, make up a modest amount, and denoise ahead of all of it.
printf '%s' "afftdn=nf=-32,\
highpass=f=85,\
acompressor=threshold=0.15:ratio=4:attack=8:release=180:makeup=1.6,\
equalizer=f=140:t=q:w=1.2:g=2.5,\
equalizer=f=3200:t=q:w=1.5:g=3,\
aecho=0.9:0.75:28:0.10,\
alimiter=limit=0.95"
