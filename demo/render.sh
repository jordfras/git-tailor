#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 Thomas Johannesson
#
# Container-side render step for the git-tailor demo artifacts. This runs INSIDE
# the toolchain image (see demo/Dockerfile), driven by demo/build.sh: it builds
# git-tailor, then renders one or more vhs tapes to demo/out/, regenerating the
# demo repo before each one.
#
# Usage (inside the container):  demo/render.sh [TAPE...]
# With no arguments every demo/gif/*.tape is rendered.
set -euo pipefail

REPO=/work
cd "$REPO"

# Build git-tailor into a cache volume (mounted at /cache) so the host's
# ./target is left untouched and crate downloads/builds are reused across runs.
export CARGO_HOME=/cache/cargo
export CARGO_TARGET_DIR=/cache/target
echo ">> building git-tailor (release) ..."
cargo build --release --manifest-path "$REPO/Cargo.toml"
export PATH="$CARGO_TARGET_DIR/release:$PATH"
command -v gt >/dev/null || {
    echo "error: gt not on PATH after build" >&2
    exit 1
}

export DEMO_REPO=/tmp/demo-repo

# Where captured scene frames are kept for the compose step. On the cache
# volume, so they survive between the two container runs without landing in the
# host tree — see the scene branch below.
SCENE_FRAMES=${SCENE_FRAMES:-/cache/scene-frames}

# Render the requested tapes (default: all of them). Each tape names its own
# Output path, resolved relative to $REPO since that is our cwd.
tapes=("$@")
if [ ${#tapes[@]} -eq 0 ]; then
    tapes=("$REPO"/demo/gif/*.tape)
fi
mkdir -p "$REPO/demo/out"
for tape in "${tapes[@]}"; do
    # Tapes rewrite history in the fixture, so each one needs a pristine copy —
    # otherwise a tape would drive whatever the previous one left behind, and
    # rendering a single tape would not reproduce the same frames.
    #
    # Which fixture is a matter of convention rather than a list to extend: a
    # tape at <root>/scenes/<name>/scene.tape belongs to the video rooted at
    # <root>, and films on <root>/make-repo.sh. Every video therefore brings its
    # own repository without this script learning its name.
    #
    # They are separate because a fixture is shaped for the story told on it —
    # the GIF's engineers a rebase conflict and its tape navigates by row
    # offsets, so sharing one would mean recalibrating and republishing the GIF
    # for every change to a video.
    #
    # Normalised to a repo-relative path first, since the caller may pass either
    # that or the absolute path the default glob produces.
    rel=${tape#"$REPO"/}
    case "$rel" in
    */scenes/*/scene.tape) scene=1 fixture=${rel%/scenes/*}/make-repo.sh ;;
    *) scene=0 fixture=demo/gif/make-repo.sh ;;
    esac
    [ -x "$REPO/$fixture" ] || {
        echo "render: $rel has no fixture at $fixture" >&2
        exit 1
    }
    echo ">> generating fixture ($fixture) at $DEMO_REPO ..."
    rm -rf "$DEMO_REPO"
    "$REPO/$fixture" "$DEMO_REPO" >/dev/null

    echo ">> rendering $tape ..."
    if [ "$scene" = 1 ]; then
        # Scenes capture a lossless PNG frame sequence, which compose.sh
        # encodes the video from. Two vhs quirks shape this:
        #
        #  * it resolves the output directory against its working directory and
        #    its parser rejects any "/" in an Output path, so vhs has to be run
        #    from where the frames should land; and
        #  * it writes nothing at all — silently, still exiting 0 — unless that
        #    directory is on the container's own filesystem. Neither the host
        #    bind mount nor the cache volume works.
        #
        # So capture under /tmp and move the frames to the cache volume, which
        # the compose step (a separate container) also mounts. That also keeps
        # a few hundred MB of frames off the host tree, like the Rust target
        # dir; `demo/build.sh clean` drops the volume and with it the frames.
        name=$(basename "$(dirname "$tape")")
        # Keyed by video as well as scene: the cache is shared by every video
        # and scene names are a video's own, so a second tutorial numbering
        # from 00-title would otherwise pick up the first one's frames.
        video=${rel#demo/}
        video=${video%/scenes/*}
        # Resolved before the cd below, while the path is still relative to here.
        tape_abs=$(readlink -f "$tape")
        scratch=/tmp/vhs-$name
        rm -rf "$scratch"
        mkdir -p "$scratch"
        (cd "$scratch" && vhs "$tape_abs")

        dest=$SCENE_FRAMES/$video/$name
        rm -rf "$dest"
        mkdir -p "$(dirname "$dest")"
        mv "$scratch/frames" "$dest"
        rm -rf "$scratch"
        echo ">> captured $name: $(find "$dest" -name 'frame-text-*.png' | wc -l) frames"
        continue
    fi
    vhs "$tape"

    # Shrink any GIF this tape just produced. Terminal captures use very few
    # colors, so palette reduction plus lossy LZW cuts size substantially with
    # no visible degradation. Scoped to this tape's own output: the compression
    # is lossy, so re-running it over GIFs left in demo/out/ by earlier renders
    # would degrade them a little more every time.
    gif=$(sed -n 's/^Output  *\(.*\.gif\) *$/\1/p' "$tape" | head -1)
    if [ -n "$gif" ] && [ -e "$REPO/$gif" ]; then
        before=$(stat -c%s "$REPO/$gif")
        gifsicle -O3 --lossy=30 --colors 128 -b "$REPO/$gif"
        echo ">> optimized $(basename "$gif"): $before -> $(stat -c%s "$REPO/$gif") bytes"
    fi
done

# Hand ownership of the rendered artifacts back to the invoking host user so the
# repo is not left with root-owned files.
if [ -n "${HOST_UID:-}" ] && [ -n "${HOST_GID:-}" ]; then
    chown -R "$HOST_UID:$HOST_GID" "$REPO/demo/out"
fi

echo ">> done. Artifacts in demo/out/:"
ls -la "$REPO/demo/out"
