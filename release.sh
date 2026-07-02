#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 Thomas Johannesson
#
# Prepare a git-tailor release: set the crate version, stamp the changelog with
# the version and today's date, and regenerate the README screenshot and demo
# GIF. This covers the mechanical parts of RELEASE.md — it does NOT commit, tag
# or publish; review the changes afterwards and continue with the checklist.
#
# Usage: release.sh <version>          e.g.  release.sh 1.1.0
set -euo pipefail

VERSION="${1:-}"
if [[ -z "$VERSION" ]]; then
    echo "usage: $0 <version>   (e.g. $0 1.1.0)" >&2
    exit 1
fi
if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.-]+)?$ ]]; then
    echo "error: '$VERSION' is not a valid semantic version (expected X.Y.Z)" >&2
    exit 1
fi

REPO_ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
cd "$REPO_ROOT"
DATE=$(date +%F) # YYYY-MM-DD

hash_of() { [[ -f "$1" ]] && sha256sum "$1" | awk '{print $1}' || echo "-missing-"; }

# ── 1. Cargo.toml version (first `version = "..."` line, i.e. the package) ──
old_version=$(grep -m1 -oP '^version = "\K[^"]+' Cargo.toml || true)
sed -i -E "0,/^version = / s/^version = \"[^\"]*\"/version = \"$VERSION\"/" Cargo.toml
echo ">> Cargo.toml version: ${old_version:-?} -> $VERSION"

# ── 2. CHANGELOG: [Unreleased] -> [version] - date ─────────────────────────
if grep -qE '^## \[Unreleased\]' CHANGELOG.md; then
    sed -i -E "s/^## \[Unreleased\].*/## [$VERSION] - $DATE/" CHANGELOG.md
    changelog_status="stamped [$VERSION] - $DATE"
else
    changelog_status="NO [Unreleased] SECTION FOUND — left unchanged"
fi
echo ">> CHANGELOG.md: $changelog_status"

# ── 3. README screenshot — deterministic, so it changes only if the UI did ──
echo ">> regenerating doc/tui_example.png (cargo run --example gen_screenshot) ..."
png_before=$(hash_of doc/tui_example.png)
if cargo run --quiet --example gen_screenshot; then
    [[ "$(hash_of doc/tui_example.png)" != "$png_before" ]] &&
        png_status="updated" || png_status="unchanged"
else
    png_status="FAILED"
fi

# ── 4. Demo GIF — rendered in Docker; note it is not byte-deterministic, so a
#      fresh render will usually report "updated" even without a visible change ─
echo ">> regenerating doc/demo.gif (demo/build.sh publish) ..."
gif_before=$(hash_of doc/demo.gif)
if demo/build.sh publish; then
    [[ "$(hash_of doc/demo.gif)" != "$gif_before" ]] &&
        gif_status="updated" || gif_status="unchanged"
else
    gif_status="FAILED"
fi

# ── Summary ────────────────────────────────────────────────────────────────
echo
echo "Release $VERSION ($DATE) prepared:"
echo "  Cargo.toml version  : ${old_version:-?} -> $VERSION"
echo "  CHANGELOG.md         : $changelog_status"
echo "  doc/tui_example.png  : $png_status"
echo "  doc/demo.gif         : $gif_status"
echo
echo "Review the changes, then continue with RELEASE.md (deps, CI, tag, publish)."
