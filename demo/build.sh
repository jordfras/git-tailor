#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 Thomas Johannesson
#
# Host-side entry point for building the git-tailor demo artifacts. Everything
# runs inside the toolchain Docker image (see demo/Dockerfile), so the only host
# requirement is Docker.
#
#   demo/build.sh gif [TAPE...]   # render tapes -> demo/out/ (default: all)
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

cmd=${1:-gif}
shift || true
case "$cmd" in
image)
    build_image
    ;;
gif)
    build_image
    docker run --rm "${common_mounts[@]}" \
        -e HOST_UID="$(id -u)" -e HOST_GID="$(id -g)" \
        "$IMAGE" demo/scripts/render.sh "$@"
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
    echo "usage: $0 {gif|image|shell|clean} [tape...]" >&2
    exit 1
    ;;
esac
