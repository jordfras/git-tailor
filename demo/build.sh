#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 Thomas Johannesson
#
# Host-side entry point for building the git-tailor demo artifacts. Everything
# runs inside the toolchain Docker image (see demo/Dockerfile), so the only host
# requirement is Docker.
#
#   demo/build.sh gif [TAPE...]   # render tapes -> demo/out/ (default: all)
#   demo/build.sh video [SCENE...] # render scenes + compose -> demo/out/promo.mp4
#   demo/build.sh cues [SCENE...]  # check keystrokes still follow the narration
#   demo/build.sh image           # (re)build the toolchain image
#   demo/build.sh shell           # interactive shell in the image
#   demo/build.sh clean           # remove artifacts and the build-cache volume
set -euo pipefail

IMAGE=git-tailor-demo
CACHE_VOLUME=git-tailor-demo-cache
SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd -- "$SCRIPT_DIR/.." && pwd)

build_image() {
    docker build -t "$IMAGE" "$REPO_ROOT/demo"
}

# The cache volume holds the out-of-tree cargo target + registry so the host's
# ./target is untouched and repeat renders reuse compiled crates.
common_mounts=(
    -v "$REPO_ROOT:/work"
    -v "$CACHE_VOLUME:/cache"
    -w /work
)

in_container() {
    docker run --rm "${common_mounts[@]}" \
        -e HOST_UID="$(id -u)" -e HOST_GID="$(id -g)" \
        -e TTS_ENGINE="${TTS_ENGINE:-}" -e TTS_VOICE="${TTS_VOICE:-}" \
        "$IMAGE" "$@"
}

render_gif() {
    build_image
    in_container demo/render.sh "$@"
}

# Render the promo scenes and compose them into demo/out/promo.mp4. Scene names
# are directory names under demo/promo/scenes/ (default: all of them).
render_video() {
    build_image
    local scenes=("$@") tapes=()
    if [ ${#scenes[@]} -eq 0 ]; then
        for dir in "$REPO_ROOT"/demo/promo/scenes/*/; do
            scenes+=("$(basename "$dir")")
        done
    fi
    for name in "${scenes[@]}"; do
        tapes+=("demo/promo/scenes/$name/scene.tape")
    done
    in_container demo/render.sh "${tapes[@]}"
    in_container demo/promo/scripts/compose.sh "${scenes[@]}"
}

# Report where each cued keystroke lands relative to the line of narration that
# calls for it. Cheap (synthesis only, no capture), so it is the fast way to
# check a narration edit before paying for a render.
check_cues() {
    build_image
    in_container demo/promo/scripts/cue-check.sh "$@"
}

cmd=${1:-gif}
shift || true
case "$cmd" in
image)
    build_image
    ;;
gif)
    render_gif "$@"
    ;;
video)
    render_video "$@"
    ;;
cues)
    check_cues "$@"
    ;;
publish)
    # Render the README demo and copy it to its committed home in doc/. README.md
    # references it there via an absolute raw URL so it renders on both GitHub
    # and crates.io. Run this before a release (see RELEASE.md) and commit the
    # updated doc/demo.gif.
    render_gif demo/gif/demo.tape
    cp "$REPO_ROOT/demo/out/demo.gif" "$REPO_ROOT/doc/demo.gif"
    echo "published -> doc/demo.gif"
    ;;
shell)
    build_image
    docker run --rm -it "${common_mounts[@]}" "$IMAGE" bash
    ;;
clean)
    rm -rf "$REPO_ROOT/demo/out"
    docker volume rm "$CACHE_VOLUME" 2>/dev/null || true
    ;;
*)
    echo "usage: $0 {gif|video|cues|publish|image|shell|clean} [tape...|scene...]" >&2
    exit 1
    ;;
esac
