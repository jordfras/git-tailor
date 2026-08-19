#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 Thomas Johannesson
#
# Turn the rendered scene captures into the finished promo video: narration per
# scene, title cards, cross-fades, a ducked music bed and transition cues.
#
# Runs INSIDE the toolchain image, after demo/render.sh has captured a PNG frame
# sequence per scene onto the cache volume (see SCENE_FRAMES below).
#
# Usage (inside the container):  compose.sh VIDEO [SCENE_NAME...]
# VIDEO is a directory under demo/ holding scenes/ — "promo",
# "tutorials/01-matrix". With no scene names every demo/VIDEO/scenes/*/ is
# composed, in name order.
#
# What differs between videos lives in demo/VIDEO/video.conf (sourced shell, all
# keys optional); what they share is the composition itself. A tutorial is the
# same machinery with the ident, the bed and the announcer chain turned off.
#
# Composing a subset writes demo/out/promo-preview.mp4 rather than promo.mp4, so
# checking one scene cannot quietly replace the finished video with a ten-second
# clip of itself.
#
# Video and audio are built as two separate passes and muxed at the end. One
# combined filter graph would work, but the two halves have entirely different
# failure modes (frame geometry vs. level balance) and are far easier to debug
# — and to re-run selectively — when kept apart.
set -euo pipefail

REPO=/work
cd "$REPO"

VIDEO=${1:?usage: compose.sh VIDEO [SCENE_NAME...]}
shift
VIDEO_DIR=$REPO/demo/$VIDEO
[ -d "$VIDEO_DIR/scenes" ] || {
    echo "compose: no video at demo/$VIDEO (wants demo/$VIDEO/scenes/)" >&2
    exit 1
}
SCENES_DIR=$VIDEO_DIR/scenes
OUT=$REPO/demo/out
# Captured frames live on the cache volume rather than in the host tree; see
# the scene branch of render.sh for why.
SCENE_FRAMES=${SCENE_FRAMES:-/cache/scene-frames}
WORK=$OUT/compose
ASSETS_GEN=$OUT/assets
ASSETS_REAL=$VIDEO_DIR/assets

# Composition constants.
W=1920
H=1080
FPS=30
XFADE=0.5      # cross-fade length between scenes, seconds
TAIL=0.6       # quiet time after the narration before a scene may transition
# How long the logo takes to spin in and land. LOGO_SPIN=0 in a video.conf
# means a logo that simply appears: no spin, no overshoot, and no impact cue
# under it. A title card is not an entrance.
# How far outside the marked box a marker's shape is drawn. Small, because the
# matrix's columns are contiguous -- 17px each with no gutter -- so anything
# generous makes a marker on one column bleed into its neighbours.
MARKER_GAP=3

LOGO_SPIN=0.55

# Per-video settings. The defaults below are the promo's; video.conf overrides
# them, and "none" switches a whole feature off rather than needing a flag.
OUT_BASE=promo
VOICE_FILTER=$REPO/demo/promo/scripts/broadcast-filter.sh

# The channel ident the video opens on. It fades to the terminal's own
# background rather than to black, so the join into the first scene is
# invisible — the card dissolves and the terminal is simply already there.
# The color is Cobalt2's background, the theme the scene tapes pin.
BUMPER=demo/promo/assets/hsn-bumper.svg
BUMPER_HOLD=2.8
BUMPER_FADE=0.5
BUMPER_TO=0x122636
# shellcheck disable=SC1090
[ -f "$VIDEO_DIR/video.conf" ] && . "$VIDEO_DIR/video.conf"

# tts.sh reads these from the environment, so a video picks its own voice.
export TTS_VOICE TTS_SPEED

[ "$BUMPER" = none ] && BUMPER="" BUMPER_HOLD=0 BUMPER_FADE=0
BUMPER_LEN=$(awk "BEGIN{printf \"%.3f\", $BUMPER_HOLD + $BUMPER_FADE}")

mkdir -p "$WORK" "$OUT/narration"

# Float helpers — bash has no floating point, and awk is always present.
fcalc() { awk "BEGIN{printf \"%.3f\", $1}"; }
fmax() { awk "BEGIN{print ($1>$2)?$1:$2}"; }
fgt() { awk "BEGIN{exit !($1>$2)}"; }

duration() {
    ffprobe -v error -show_entries format=duration -of default=nw=1:nk=1 "$1"
}

# Gain that brings a cue to `target` mean dBFS, without letting its peak exceed
# CUE_CEILING.
#
# Cues are meant to be swappable by dropping a file into demo/promo/assets/, and
# sources differ wildly in level, so the level is measured and corrected rather
# than hard-coded per file.
#
# Matching on *mean* rather than peak is what makes that work. A one-shot effect
# is nearly all transient: the Kenney bell peaks at -1dB but averages -22, a
# crest factor of 21dB, where a synthesized tone of the same peak averages -16.
# Lining their peaks up therefore lines up nothing a listener notices — it put
# the bell's actual energy at about -41dB, inaudible under the music. Mean
# tracks perceived loudness; the ceiling then keeps the transient from clipping.
cue_gain() {
    local file=$1 target=$2 mean peak
    # Silence is stripped before measuring. Library files are often padded —
    # the whoosh is 4.2s of file for 1.5s of sound — and averaging over the
    # padding understates the loudness, so the cue then gets pushed far too
    # loud to compensate.
    read -r mean peak <<<"$(ffmpeg -hide_banner -nostats -i "$file" \
        -af "silenceremove=stop_periods=-1:stop_threshold=-60dB:stop_duration=0.05,volumedetect" \
        -f null - 2>&1 |
        sed -n 's/.*mean_volume: \(.*\) dB/\1/p; s/.*max_volume: \(.*\) dB/\1/p' |
        tr '\n' ' ')"
    [ -n "$mean" ] || mean=$target
    [ -n "$peak" ] || peak=0
    awk -v mean="$mean" -v peak="$peak" -v want="$target" -v ceiling="$CUE_CEILING" '
        BEGIN {
            g = want - mean
            if (peak + g > ceiling) g = ceiling - peak
            printf "%.4f", 10^(g/20)
        }'
}

# Loudness the cues are mixed at. The smack is a punchline and sits with the
# narration; the transition cues are punctuation and sit just under it.
SFX_MEAN=-15
SMACK_MEAN=-8
CUE_CEILING=-2
# The smack alone is allowed above the ceiling, so the limiter ducks the rest of
# the mix around it rather than the smack being squeezed in beside it.
SMACK_CEILING=8
# The music bed, and how it is treated where the story turns glum.
#
# Rolling the top off alone just makes the bed quieter, which reads as the music
# having been turned down rather than having gone flat. The low end is lifted to
# put back what the filter takes out, so the bed stays exactly as present and
# only its life is gone. MUSIC_DULL_GAIN is then a small nudge, not the effect.
MUSIC_MEAN=-27
MUSIC_DULL_HZ=1100
MUSIC_DULL_BASS=7
MUSIC_DULL_GAIN=0.92
# Two semitones down. 2^(-2/12); a whole tone is enough to hear as a sag
# without the bed sounding broken.
MUSIC_DULL_PITCH=0.8909

# Prefer real audio dropped into demo/promo/assets/ over the synthesized placeholders.
asset() {
    local name=$1 ext
    for ext in wav mp3 flac ogg m4a opus; do
        if [ -f "$ASSETS_REAL/$name.$ext" ]; then
            echo "$ASSETS_REAL/$name.$ext"
            return
        fi
    done
    echo "$ASSETS_GEN/$name.wav"
}

# Turns the flat TTS output into an announcer read; shared with the audition
# script so a voice test predicts the finished mix. A teaching video wants none
# of it, so VOICE_FILTER=none leaves the voice as synthesized.
if [ "$VOICE_FILTER" = none ]; then
    BROADCAST=anull
else
    BROADCAST=$("$VOICE_FILTER")
fi

FONT=$(fc-match -f '%{file}' 'JetBrains Mono:style=Bold')
[ -n "$FONT" ] || {
    echo "compose: no font found for title cards" >&2
    exit 1
}

# ---------------------------------------------------------------------------
# Collect scenes
# ---------------------------------------------------------------------------
scenes=("$@")
if [ ${#scenes[@]} -eq 0 ]; then
    for dir in "$SCENES_DIR"/*/; do
        scenes+=("$(basename "$dir")")
    done
fi
[ ${#scenes[@]} -gt 0 ] || {
    echo "compose: no scenes found under $SCENES_DIR" >&2
    exit 1
}

# A partial compose is a preview of one beat, not the video, and must not land
# on the video's filename: `demo/build.sh video 04-pitch` is the natural way to
# iterate on a scene, and having it silently overwrite promo.mp4 with a
# ten-second clip is a trap worth closing. Compare against the full set rather
# than the argument count, since build.sh always passes names explicitly.
all_scenes=()
for dir in "$SCENES_DIR"/*/; do all_scenes+=("$(basename "$dir")"); done
if [ "${scenes[*]}" = "${all_scenes[*]}" ]; then
    OUT_NAME=$OUT_BASE.mp4
else
    OUT_NAME=$OUT_BASE-preview.mp4
fi

# ---------------------------------------------------------------------------
# Placeholder music/SFX (skipped for any asset supplied for real)
# ---------------------------------------------------------------------------
echo ">> synthesizing placeholder audio beds ..."
"$REPO/demo/promo/scripts/make-audio-beds.sh" "$ASSETS_GEN" >/dev/null
if [ "${MUSIC:-}" = none ]; then
    HAVE_MUSIC=0
else
    HAVE_MUSIC=1
    MUSIC=$(asset music)
fi

# The music is looped to cover the video, so only a musically whole section of
# it can be used. These bounds are 8 bars at 89.1bpm, measured off the track's own
# downbeat grid, and they stop before the vocal the recording ends with — a jingle
# for someone else's bank, which would be quite a thing to leave in.
#
# The loop is built once here rather than in the main graph: the section is cut
# out, and the audio just past its end is crossfaded back over its beginning, so
# the point where the loop wraps is already blended and does not click. The file
# in demo/promo/assets/ is left exactly as downloaded.
MUSIC_LOOP_START=${MUSIC_LOOP_START:-5.410}
MUSIC_LOOP_END=${MUSIC_LOOP_END:-26.959}
MUSIC_XFADE=0.5

if [ "$HAVE_MUSIC" = 1 ] && [ -n "$MUSIC_LOOP_END" ] &&
    [ "$MUSIC" != "$ASSETS_GEN/music.wav" ]; then
    loop=$WORK/music-loop.wav
    ffmpeg -hide_banner -loglevel error -y -i "$MUSIC" -filter_complex "\
[0:a]asplit=2[a][b];\
[a]atrim=$MUSIC_LOOP_START:$MUSIC_LOOP_END,asetpts=N/SR/TB,\
afade=t=in:st=0:d=$MUSIC_XFADE:curve=tri[body];\
[b]atrim=$MUSIC_LOOP_END:$(fcalc "$MUSIC_LOOP_END + $MUSIC_XFADE"),asetpts=N/SR/TB,\
afade=t=out:st=0:d=$MUSIC_XFADE:curve=tri[tail];\
[body][tail]amix=inputs=2:duration=first:normalize=0[m]" \
        -map "[m]" -ar 48000 -ac 2 "$loop"
    MUSIC=$loop
    echo ">> music loop: ${MUSIC_LOOP_START}s-${MUSIC_LOOP_END}s ($(duration "$loop")s)"
fi
[ "$HAVE_MUSIC" = 1 ] && echo ">> music bed: $MUSIC"

# ---------------------------------------------------------------------------
# Per-scene narration and timing
#
# A scene lasts as long as whichever runs longer: the screen capture, or the
# narration written over it (plus its lead-in and a short tail). Whichever is
# shorter gets padded — video by freezing its last frame, audio by silence.
# ---------------------------------------------------------------------------
names=()
videos=()
vdurs=()
rates=()
narrations=()
scene_durs=()
starts=()
delays=()
titles=()
subtitles=()
sfxs=()
sfx_ats=()
holds=()
title_ats=()
grays=()
dulls=()
logos=()
logo_ats=()
logo_holds=()
stamps=()
stamp_ats=()
stamp_holds=()
stamp_sizes=()
stamp_xs=()
stamp_ys=()
markers=()

cursor=$BUMPER_LEN
for name in "${scenes[@]}"; do
    dir=$SCENES_DIR/$name
    # Captures are lossless PNG frame sequences. vhs writes two layers per
    # frame — the terminal contents and the cursor — which get composited below.
    frames=$SCENE_FRAMES/$VIDEO/$name
    nframes=0
    [ -d "$frames" ] && nframes=$(find "$frames" -name 'frame-text-*.png' | wc -l)
    [ "$nframes" -gt 0 ] || {
        echo "compose: no captured frames for '$name' in $frames (run render.sh first)" >&2
        exit 1
    }

    TITLE="" SUBTITLE="" NARRATION_DELAY=0.8 SFX=none SFX_AT=0 MUSIC_DULL=0
    TITLE_HOLD=3.3 TITLE_AT=0.4 GRAYSCALE=0 SPEED=1
    LOGO="" LOGO_AT=0.3 LOGO_HOLD=2.2
    STAMP="" STAMP_AT=1.0 STAMP_HOLD=2.5 STAMP_SIZE=260
    # Boxes drawn around whatever the narration is naming, one per element:
    #   "AT HOLD X Y W H [SHAPE]"  -- seconds, the box in frame pixels, and
    # optionally rect (default) | arrow-up | arrow-down.
    # Pixels rather than terminal cells, because a marker points at whatever
    # needs pointing at and that is not always on the grid. Read the numbers off
    # a frame; see demo/promo/README.md.
    MARKERS=()
    # In overlay expressions W/H are the frame and w/h are the stamp — mixing
    # them up silently parks the picture in the top-left corner.
    STAMP_X="W*0.78-w/2" STAMP_Y="H*0.40-h/2"
    # shellcheck disable=SC1090
    [ -f "$dir/scene.conf" ] && . "$dir/scene.conf"

    narration=$OUT/narration/$name.wav
    echo ">> narrating $name ..."
    "$REPO/demo/promo/scripts/tts.sh" "$dir/narration.txt" "$narration"

    # How long the scene runs comes from the tape, not from the frame count:
    # vhs captures more slowly than the requested rate at 1080p, by a margin
    # that shifts with machine load. See tape-duration.sh.
    vdur=$("$REPO/demo/promo/scripts/tape-duration.sh" "$dir/scene.tape")
    # SPEED is a fast-forward: the frames are the same, they just play over a
    # shorter time, so it needs no filter of its own — only a higher input rate.
    vdur=$(fcalc "$vdur / $SPEED")
    # ... so the frames that were captured get spread evenly across it.
    rate=$(fcalc "$nframes / $vdur")
    adur=$(duration "$narration")
    needed=$(fcalc "$NARRATION_DELAY + $adur + $TAIL")
    sdur=$(fmax "$vdur" "$needed")

    names+=("$name")
    videos+=("$frames")
    vdurs+=("$vdur")
    rates+=("$rate")
    narrations+=("$narration")
    scene_durs+=("$sdur")
    starts+=("$cursor")
    delays+=("$NARRATION_DELAY")
    titles+=("$TITLE")
    subtitles+=("$SUBTITLE")
    sfxs+=("$SFX")
    sfx_ats+=("$SFX_AT")
    holds+=("$TITLE_HOLD")
    title_ats+=("$TITLE_AT")
    grays+=("$GRAYSCALE")
    dulls+=("$MUSIC_DULL")
    logos+=("$LOGO")
    logo_ats+=("$LOGO_AT")
    logo_holds+=("$LOGO_HOLD")
    stamps+=("$STAMP")
    stamp_ats+=("$STAMP_AT")
    stamp_holds+=("$STAMP_HOLD")
    stamp_sizes+=("$STAMP_SIZE")
    stamp_xs+=("$STAMP_X")
    stamp_ys+=("$STAMP_Y")
    # Flattened with a separator: bash has no nested arrays.
    markers+=("$(printf '%s|' ${MARKERS[@]+"${MARKERS[@]}"})")

    printf '   %-10s tape %ss (%s frames @ %sfps), voice %ss -> scene %ss @ %ss\n' \
        "$name" "$vdur" "$nframes" "$rate" "$adur" "$sdur" "$cursor"

    # The next scene starts where this one ends, minus the overlap the
    # cross-fade eats.
    cursor=$(fcalc "$cursor + $sdur - $XFADE")
done

n=${#names[@]}
TOTAL=$(fcalc "$cursor + $XFADE")
echo ">> $n scene(s), $TOTAL s total"

# ---------------------------------------------------------------------------
# Pass 1 — video: normalize, title card, pad, cross-fade
# ---------------------------------------------------------------------------
echo ">> composing video ..."

# Title cards fade in shortly after the scene starts and back out once the
# scene's TITLE_HOLD has elapsed — the bonus round wants ~2s bursts where the
# opening wants a leisurely one. Single-quoted at the point of use so the commas
# inside the conditionals stay part of the expression rather than splitting the
# filter graph.
# Largest font size at which `text` still fits across the frame, capped at
# `max`. JetBrains Mono is monospaced with an advance width of ~0.6em, so the
# width of a string is predictable — which is just as well, since drawtext has
# no way to fit text to a box and silently runs long titles off the screen.
fit_size() {
    local text=$1 max=$2
    awk -v n="${#text}" -v max="$max" -v usable="$((W * 88 / 100))" \
        'BEGIN { s = int(usable / (0.6 * n)); print (s < max) ? s : max }'
}

# Outline width for a given font size. Cards are drawn as white glyphs with a
# black outline rather than white-on-a-black-box: the box was a slab across the
# picture, while an outline keeps the text readable over anything underneath —
# terminal, diff colors, the logo — without hiding it. Scaled with the font so
# a small card is not smothered and a large one is not left too thin.
outline() {
    awk -v s="$1" 'BEGIN { b = int(s / 16); print (b < 2) ? 2 : b }'
}

title_alpha() {
    local hold=$1 in=$2 fade=0.5 out
    out=$(fcalc "$in + $fade + $hold")
    echo "if(lt(t,$in),0,\
if(lt(t,$(fcalc "$in + $fade")),(t-$in)/$fade,\
if(lt(t,$out),1,\
if(lt(t,$(fcalc "$out + $fade")),($(fcalc "$out + $fade")-t)/$fade,0))))"
}

vargs=()
vfilter=""

# Two inputs per scene: the terminal contents and the cursor layer on top. All
# of them come first so a scene's pair is always at 2i and 2i+1; any logo
# inputs are appended after, keeping that arithmetic true.
for i in $(seq 0 $((n - 1))); do
    vargs+=(-framerate "${rates[$i]}" -i "${videos[$i]}/frame-text-%05d.png")
    vargs+=(-framerate "${rates[$i]}" -i "${videos[$i]}/frame-cursor-%05d.png")
done

logo_idx=()
next_input=$((n * 2))
for i in $(seq 0 $((n - 1))); do
    logo_idx[$i]=""
    [ -n "${logos[$i]}" ] || continue
    [ -f "$REPO/${logos[$i]}" ] || {
        echo "compose: no logo at $REPO/${logos[$i]}" >&2
        exit 1
    }
    vargs+=(-loop 1 -framerate "$FPS" -t "${scene_durs[$i]}" -i "$REPO/${logos[$i]}")
    logo_idx[$i]=$next_input
    next_input=$((next_input + 1))
done

# Emoji stamps, rasterized out of the color font — ffmpeg cannot draw those
# itself, see make-emoji.py.
stamp_idx=()
for i in $(seq 0 $((n - 1))); do
    stamp_idx[$i]=""
    [ -n "${stamps[$i]}" ] || continue
    png=$ASSETS_GEN/emoji/${names[$i]}.png
    "$KOKORO_HOME/venv/bin/python" "$REPO/demo/promo/scripts/make-emoji.py" \
        --emoji "${stamps[$i]}" --out "$png" --size "${stamp_sizes[$i]}"
    vargs+=(-loop 1 -framerate "$FPS" -t "${scene_durs[$i]}" -i "$png")
    stamp_idx[$i]=$next_input
    next_input=$((next_input + 1))
done

# Annotation markers. One PNG per marker rather than one per distinct size: a
# scene rarely has more than a handful, and the files are tiny.
marker_idx=()
declare -A marker_off=()
for i in $(seq 0 $((n - 1))); do
    marker_idx[$i]=""
    IFS='|' read -r -a specs <<<"${markers[$i]}"
    for j in "${!specs[@]}"; do
        [ -n "${specs[$j]}" ] || continue
        read -r _at _hold _x _y _w _h _shape <<<"${specs[$j]}"
        png=$ASSETS_GEN/markers/${names[$i]}-$j.png
        mkdir -p "$(dirname "$png")"
        # The box in the config is the thing being marked; the shape is drawn
        # clear of it so a marker always reads as "around" rather than "on",
        # and reports back where that leaves its own top-left corner.
        marker_off[$i,$j]=$("$KOKORO_HOME/venv/bin/python" \
            "$REPO/demo/promo/scripts/make-marker.py" \
            --out "$png" --width "$_w" --height "$_h" \
            --gap "$MARKER_GAP" --shape "${_shape:-rect}")
        vargs+=(-loop 1 -framerate "$FPS" -t "${scene_durs[$i]}" -i "$png")
        marker_idx[$i]="${marker_idx[$i]}$next_input "
        next_input=$((next_input + 1))
    done
done

for i in $(seq 0 $((n - 1))); do
    a=$((i * 2))
    b=$((i * 2 + 1))

    chain="scale=$W:$H:force_original_aspect_ratio=decrease"
    chain="$chain,pad=$W:$H:(ow-iw)/2:(oh-ih)/2:color=black,setsar=1,fps=$FPS"

    # The infomercial "problem" look. Applied before the title card so the card
    # itself stays white rather than going gray with the footage.
    if [ "${grays[$i]}" != "0" ]; then
        chain="$chain,hue=s=0"
    fi

    ALPHA=$(title_alpha "${holds[$i]}" "${title_ats[$i]}")

    pad=$(fcalc "${scene_durs[$i]} - ${vdurs[$i]}")
    if fgt "$pad" 0.02; then
        chain="$chain,tpad=stop_mode=clone:stop_duration=$pad"
    fi

    # Title text is passed as a file so a colon, comma or quote in it can never
    # break the filter graph.
    if [ -n "${titles[$i]}" ]; then
        printf '%s' "${titles[$i]}" >"$WORK/title-$i.txt"
        size=$(fit_size "${titles[$i]}" 76)
        chain="$chain,drawtext=fontfile=$FONT:textfile=$WORK/title-$i.txt"
        chain="$chain:fontsize=$size:fontcolor=white"
        chain="$chain:borderw=$(outline "$size"):bordercolor=black"
        chain="$chain:x=(w-text_w)/2:y=h-280:alpha='$ALPHA'"
    fi
    if [ -n "${subtitles[$i]}" ]; then
        printf '%s' "${subtitles[$i]}" >"$WORK/sub-$i.txt"
        size=$(fit_size "${subtitles[$i]}" 40)
        chain="$chain,drawtext=fontfile=$FONT:textfile=$WORK/sub-$i.txt"
        chain="$chain:fontsize=$size:fontcolor=white"
        chain="$chain:borderw=$(outline "$size"):bordercolor=black"
        chain="$chain:x=(w-text_w)/2:y=h-180:alpha='$ALPHA'"
    fi

    # The picture, then whatever decorations this scene asks for.
    vfilter="$vfilter[$a:v][$b:v]overlay=0:0,$chain[pic$i];"
    src="[pic$i]"

    if [ -n "${stamp_idx[$i]}" ]; then
        # A quick pop with an overshoot, the same trick as the logo: scale needs
        # eval=frame to re-read its expressions, and the overlay is held off
        # until its moment with enable.
        sin=${stamp_ats[$i]}
        sout=$(fcalc "$sin + ${stamp_holds[$i]}")
        st="max(0\\,t-$sin)"
        pop="if(lt($st\\,0.22)\\,0.2+3.6*$st\\,\
if(lt($st\\,0.34)\\,1.2-($st-0.22)/0.12*0.2\\,1))"
        vfilter="$vfilter[${stamp_idx[$i]}:v]format=rgba,"
        vfilter="$vfilter""scale=w=iw*$pop:h=ih*$pop:eval=frame,"
        vfilter="$vfilter""fade=t=out:st=$sout:d=0.4:alpha=1[stamp$i];"
        vfilter="$vfilter$src[stamp$i]overlay=${stamp_xs[$i]}:${stamp_ys[$i]}"
        vfilter="$vfilter:enable='gte(t\\,$sin)'[stamped$i];"
        src="[stamped$i]"
    fi

    # Markers, after any GRAYSCALE so an annotation keeps its color on a scene
    # that has deliberately lost its own.
    IFS='|' read -r -a specs <<<"${markers[$i]}"
    mj=0
    for idx in ${marker_idx[$i]}; do
        read -r _at _hold _x _y _w _h _shape <<<"${specs[$mj]}"
        mout=$(fcalc "$_at + $_hold")
        read -r offx offy <<<"${marker_off[$i,$mj]}"
        ox=$((_x - offx))
        oy=$((_y - offy))
        # Separated indices: `mark${i}${mj}` reads the same for scene 1 marker
        # 12 as for scene 11 marker 2, and ffmpeg rejects the duplicate label
        # by failing the whole compose.
        vfilter="$vfilter[$idx:v]format=rgba,"
        vfilter="$vfilter""fade=t=in:st=$_at:d=0.25:alpha=1,"
        vfilter="$vfilter""fade=t=out:st=$mout:d=0.35:alpha=1[mark${i}_$mj];"
        vfilter="$vfilter$src[mark${i}_$mj]overlay=$ox:$oy"
        vfilter="$vfilter:enable='between(t\,$_at\,$(fcalc "$mout + 0.35"))'[marked${i}_$mj];"
        src="[marked${i}_$mj]"
        mj=$((mj + 1))
    done

    if [ -z "${logo_idx[$i]}" ]; then
        vfilter="$vfilter$src""format=yuv420p[v$i];"
        continue
    fi

    # The logo spins in from small to large and lands with a smack, in the
    # manner of a certain 1960s TV series. Then it holds and fades out.
    #
    # Two things that are easy to get wrong here. `rotate`'s c=none does not
    # mean transparent — it means "do not paint the background", so each frame
    # keeps the one before it and the spin smears into a wheel of ghost logos;
    # c=black@0 is the transparent fill. And `scale` only re-reads its
    # expressions per frame with eval=frame, without which the zoom is frozen
    # at whatever the first frame computed.
    #
    # Every comma inside an expression is escaped, since an unescaped one ends
    # the filter.
    lin=${logo_ats[$i]}
    lland=$(fcalc "$lin + $LOGO_SPIN")
    lout=$(fcalc "$lland + ${logo_holds[$i]}")
    # Time since the spin began, clamped so the logo is not mid-animation
    # before it is due; `enable` on the overlay keeps it off screen until then.
    tt="max(0\\,t-$lin)"
    # Grow to a slight overshoot, then settle back — the overshoot is what
    # sells the impact.
    zoom="if(lt($tt\\,$LOGO_SPIN)\\,0.12+1.03*$tt/$LOGO_SPIN\\,\
if(lt($tt\\,$LOGO_SPIN+0.15)\\,1.15-($tt-$LOGO_SPIN)/0.15*0.15\\,1))"
    spin="if(lt($tt\\,$LOGO_SPIN)\\,6*PI*(1-$tt/$LOGO_SPIN)\\,0)"

    vfilter="$vfilter[${logo_idx[$i]}:v]format=rgba,scale=$((W * 47 / 100)):-1,"
    if fgt "$LOGO_SPIN" 0; then
        vfilter="$vfilter""rotate=a=$spin:c=black@0:ow=hypot(iw\\,ih):oh=ow,"
        vfilter="$vfilter""scale=w=iw*$zoom:h=ih*$zoom:eval=frame,"
    else
        vfilter="$vfilter""fade=t=in:st=$lin:d=0.4:alpha=1,"
    fi
    vfilter="$vfilter""fade=t=out:st=$lout:d=0.4:alpha=1[logo$i];"
    vfilter="$vfilter$src[logo$i]overlay=(W-w)/2:(H-h)/2-80"
    vfilter="$vfilter:enable='gte(t\\,$lin)',format=yuv420p[v$i];"
done

# Chain the scenes together with cross-fades. Each fade starts where its scene
# starts on the final timeline.
prev="[v0]"
for i in $(seq 1 $((n - 1))); do
    label="[x$i]"
    [ "$i" -eq $((n - 1)) ] && label="[vout]"
    vfilter="$vfilter$prev[v$i]xfade=transition=fade:duration=$XFADE"
    # Every narration, cue and stamp time is measured from starts[], so the
    # ident shifts the whole timeline by its own length for free. Cross-fade
    # offsets are the exception: they are relative to the start of the scene
    # chain rather than the timeline, so the ident has to come back off.
    vfilter="$vfilter:offset=$(fcalc "${starts[$i]} - $BUMPER_LEN")$label;"
    prev="$label"
done
if [ "$n" -eq 1 ]; then
    vfilter="$vfilter[v0]null[vout];"
fi
# The ident, held and then dissolved into the terminal's background color. A
# video without one starts on its first scene; there is no empty input to feed
# ffmpeg in that case, so the whole branch goes rather than being zero-length.
if [ -n "$BUMPER" ]; then
    vargs+=(-loop 1 -framerate "$FPS" -t "$BUMPER_LEN" -i "$REPO/$BUMPER")
    vfilter="$vfilter[$next_input:v]scale=$W:$H,setsar=1,fps=$FPS,"
    vfilter="$vfilter""fade=t=out:st=$BUMPER_HOLD:d=$BUMPER_FADE:color=$BUMPER_TO,"
    vfilter="$vfilter""format=yuv420p[bumper];"
    vfilter="$vfilter[bumper][vout]concat=n=2:v=1:a=0[vfinal]"
else
    vfilter="$vfilter[vout]null[vfinal]"
fi

ffmpeg -hide_banner -loglevel error -y "${vargs[@]}" \
    -filter_complex "$vfilter" \
    -map "[vfinal]" -c:v libx264 -crf 20 -preset medium -pix_fmt yuv420p \
    "$WORK/video.mp4"

# ---------------------------------------------------------------------------
# Pass 2 — audio: narration bus, SFX cues, ducked music
# ---------------------------------------------------------------------------
echo ">> composing audio ..."
aargs=()
afilter=""
idx=0
voice_labels=""
for i in $(seq 0 $((n - 1))); do
    aargs+=(-i "${narrations[$i]}")
    at=$(fcalc "(${starts[$i]} + ${delays[$i]}) * 1000")
    afilter="$afilter[$idx:a]aresample=48000,aformat=channel_layouts=stereo,"
    afilter="$afilter""$BROADCAST,"
    afilter="$afilter""adelay=$at:all=1[n$i];"
    voice_labels="$voice_labels[n$i]"
    idx=$((idx + 1))
done
afilter="$afilter$voice_labels""amix=inputs=$n:normalize=0:dropout_transition=0[voiceraw];"
# Pad the voice bus out to the full length before splitting it. Both halves
# then span the whole video: the sidechain (so ducking releases cleanly instead
# of cutting the music at the last spoken word) and the mixed bus (because amix
# ends with its shortest input once two inputs share an asplit ancestor,
# whatever duration= says).
afilter="$afilter[voiceraw]volume=1.3,apad,atrim=0:$TOTAL,asetpts=N/SR/TB"
# The second branch exists only to key the music ducking. With no bed there is
# nothing to duck, and an unconsumed pad is a filter-graph error rather than a
# no-op, so the split is made only when it is used.
if [ "$HAVE_MUSIC" = 1 ]; then
    afilter="$afilter,asplit=2[voice][key];"
else
    afilter="$afilter[voice];"
fi

# One SFX cue per scene that asks for one, at the moment the scene arrives.
sfx_labels=""
sfx_count=0
for i in $(seq 0 $((n - 1))); do
    [ "${sfxs[$i]}" = "none" ] && continue
    file=$(asset "${sfxs[$i]}")
    [ -f "$file" ] || {
        echo "compose: missing sfx $file" >&2
        exit 1
    }
    aargs+=(-i "$file")
    # Usually at the cut, but a card's accent wants SFX_AT to put it where the
    # card actually becomes visible rather than where the scene technically
    # begins — the title only starts fading in 0.4s later.
    at=$(fcalc "(${starts[$i]} + ${sfx_ats[$i]}) * 1000")
    afilter="$afilter[$idx:a]aresample=48000,aformat=channel_layouts=stereo,"
    afilter="$afilter""volume=$(cue_gain "$file" "$SFX_MEAN"),adelay=$at:all=1[s$i];"
    sfx_labels="$sfx_labels[s$i]"
    sfx_count=$((sfx_count + 1))
    idx=$((idx + 1))
done

# And a smack under each logo landing, timed to the end of its spin. A logo
# that does not move has nothing to land, so it gets no cue.
for i in $(seq 0 $((n - 1))); do
    [ -n "${logos[$i]}" ] || continue
    fgt "$LOGO_SPIN" 0 || continue
    file=$(asset smack)
    [ -f "$file" ] || {
        echo "compose: missing sfx $file" >&2
        exit 1
    }
    aargs+=(-i "$file")
    at=$(fcalc "(${starts[$i]} + ${logo_ats[$i]} + $LOGO_SPIN) * 1000")
    # Allowed past the ceiling the other cues respect. An impact's peak is
    # already near full scale in the source, so gain alone cannot make it louder
    # — it just runs into the ceiling — and compressing it first only squashes
    # the transient that carries the wallop. Letting it overshoot instead means
    # the mix limiter briefly pulls the music and voice down around it, which is
    # what a wallop actually sounds like.
    afilter="$afilter[$idx:a]aresample=48000,aformat=channel_layouts=stereo,"
    afilter="$afilter""volume=$(CUE_CEILING=$SMACK_CEILING cue_gain "$file" "$SMACK_MEAN"),"
    afilter="$afilter""adelay=$at:all=1[k$i];"
    sfx_labels="$sfx_labels[k$i]"
    sfx_count=$((sfx_count + 1))
    idx=$((idx + 1))
done

if [ "$HAVE_MUSIC" = 1 ]; then
# The bed is rendered to a file first, in stretches, rather than treated inside
# the main graph. Pitch is why: `enable` can gate a filter, but not a pitch
# shift, so the only way to drop the key over part of the video is to cut the
# bed at those boundaries, shift the pieces, and put it back together.
MUSIC_BED=$WORK/music-bed.wav

# Merge the dulled scenes into ranges. They are usually adjacent — the cross-fade
# makes consecutive scenes overlap — and one long stretch beats two abutting ones.
ranges=()
for i in $(seq 0 $((n - 1))); do
    [ "${dulls[$i]}" = "1" ] || continue
    from=${starts[$i]}
    to=$(fcalc "${starts[$i]} + ${scene_durs[$i]}")
    if [ ${#ranges[@]} -gt 0 ]; then
        last=${ranges[-1]}
        prev_to=${last#*:}
        if ! fgt "$from" "$prev_to"; then
            ranges[-1]="${last%%:*}:$to"
            continue
        fi
    fi
    ranges+=("$from:$to")
done

# Walk the timeline, emitting a segment per stretch, dulled or not.
bounds=(0)
for r in "${ranges[@]}"; do
    bounds+=("${r%%:*}" "${r#*:}")
done
bounds+=("$TOTAL")

# The leveled source is written out once, and each stretch re-reads it: one
# filter output cannot be consumed by several branches without asplit, and
# asplit here would need as many outputs as there are stretches.
ffmpeg -hide_banner -loglevel error -y -stream_loop -1 -i "$MUSIC" -t "$TOTAL" \
    -af "aresample=48000,aformat=channel_layouts=stereo,volume=$(cue_gain "$MUSIC" "$MUSIC_MEAN")" \
    -ar 48000 -ac 2 "$WORK/music-flat.wav"

inputs=()
graph=""
labels=""
j=0
for k in $(seq 0 $((${#bounds[@]} - 2))); do
    from=${bounds[$k]}
    to=${bounds[$((k + 1))]}
    len=$(fcalc "$to - $from")
    fgt "$len" 0.02 || continue
    inputs+=(-i "$WORK/music-flat.wav")
    graph="$graph[$j:a]atrim=$from:$to,asetpts=N/SR/TB"
    if [ $((k % 2)) = 1 ]; then
        graph="$graph,rubberband=pitch=$MUSIC_DULL_PITCH"
        graph="$graph,lowpass=f=$MUSIC_DULL_HZ,bass=g=$MUSIC_DULL_BASS:f=220"
        graph="$graph,volume=$MUSIC_DULL_GAIN,apad,atrim=0:$len,asetpts=N/SR/TB"
    fi
    # A short fade at each internal join so the change of treatment does not
    # click. It lands on a scene cut, where the picture is changing anyway.
    [ "$j" -gt 0 ] && graph="$graph,afade=t=in:st=0:d=0.04"
    [ "$k" -lt $((${#bounds[@]} - 2)) ] &&
        graph="$graph,afade=t=out:st=$(fcalc "$len - 0.04"):d=0.04"
    graph="$graph[s$j];"
    labels="$labels[s$j]"
    j=$((j + 1))
done
ffmpeg -hide_banner -loglevel error -y "${inputs[@]}" \
    -filter_complex "$graph${labels}concat=n=$j:v=0:a=1[m]" \
    -map "[m]" -ar 48000 -ac 2 "$MUSIC_BED"
echo ">> music bed: $j stretch(es), ${#ranges[@]} dulled, $(duration "$MUSIC_BED")s"

aargs+=(-i "$MUSIC_BED")
music_idx=$idx
afilter="$afilter[$music_idx:a]aresample=48000,aformat=channel_layouts=stereo,"
afilter="$afilter""apad,atrim=0:$TOTAL,asetpts=N/SR/TB[musicraw];"

afilter="$afilter[musicraw][key]sidechaincompress=threshold=0.02:ratio=12:attack=25:release=450[music];"
fi

if [ "$HAVE_MUSIC" = 1 ]; then
    mix="[voice][music]"
    inputs=2
else
    mix="[voice]"
    inputs=1
fi
if [ "$sfx_count" -gt 0 ]; then
    mix="$mix$sfx_labels"
    inputs=$((inputs + sfx_count))
fi
afilter="$afilter$mix""amix=inputs=$inputs:normalize=0:dropout_transition=0,"
afilter="$afilter""alimiter=limit=0.95,atrim=0:$TOTAL,asetpts=N/SR/TB[aout]"

ffmpeg -hide_banner -loglevel error -y "${aargs[@]}" \
    -filter_complex "$afilter" \
    -map "[aout]" -ar 48000 -ac 2 "$WORK/audio.wav"

# ---------------------------------------------------------------------------
# Mux
# ---------------------------------------------------------------------------
echo ">> muxing ..."
ffmpeg -hide_banner -loglevel error -y \
    -i "$WORK/video.mp4" -i "$WORK/audio.wav" \
    -map 0:v -map 1:a -c:v copy -c:a aac -b:a 192k \
    -movflags +faststart -shortest \
    "$OUT/$OUT_NAME"

if [ -n "${HOST_UID:-}" ] && [ -n "${HOST_GID:-}" ]; then
    chown -R "$HOST_UID:$HOST_GID" "$OUT"
fi

echo ">> done: demo/out/$OUT_NAME ($(duration "$OUT/$OUT_NAME")s)"
