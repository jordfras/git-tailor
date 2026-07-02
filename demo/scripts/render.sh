#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 Thomas Johannesson
#
# Container-side render step for the git-tailor demo artifacts. This runs INSIDE
# the toolchain image (see demo/Dockerfile), driven by demo/Makefile: it builds
# git-tailor, generates a fresh demo repo, and renders one or more vhs tapes to
# demo/out/.
#
# Usage (inside the container):  demo/scripts/render.sh [TAPE...]
# With no arguments every demo/tapes/*.tape is rendered.
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

# A fresh demo repository for the tapes to drive.
export DEMO_REPO=/tmp/demo-repo
echo ">> generating demo repo at $DEMO_REPO ..."
rm -rf "$DEMO_REPO"
"$REPO/demo/scripts/make-demo-repo.sh" "$DEMO_REPO" >/dev/null

# Render the requested tapes (default: all of them). Each tape names its own
# Output path, resolved relative to $REPO since that is our cwd.
tapes=("$@")
if [ ${#tapes[@]} -eq 0 ]; then
    tapes=("$REPO"/demo/tapes/*.tape)
fi
mkdir -p "$REPO/demo/out"
for tape in "${tapes[@]}"; do
    echo ">> rendering $tape ..."
    vhs "$tape"
done

# Hand ownership of the rendered artifacts back to the invoking host user so the
# repo is not left with root-owned files.
if [ -n "${HOST_UID:-}" ] && [ -n "${HOST_GID:-}" ]; then
    chown -R "$HOST_UID:$HOST_GID" "$REPO/demo/out"
fi

echo ">> done. Artifacts in demo/out/:"
ls -la "$REPO/demo/out"
