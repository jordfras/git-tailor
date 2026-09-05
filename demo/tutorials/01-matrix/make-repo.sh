#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 Thomas Johannesson
#
# Build the repository the matrix tutorial films on.
#
# The other fixtures here are shaped for the operations performed on them. This
# one is shaped for what the matrix *shows*, so the history is arranged to put
# each thing the tutorial explains on screen with an obvious reason:
#
#   * several files and well-separated regions, so there are enough columns for
#     the matrix to read as a matrix rather than a sliver;
#   * a pair of commits touching one region with nothing in between, which folds
#     cleanly -- the green square and connector;
#   * a commit whose changes lead back to two *different* commits, which can
#     therefore fold into neither -- the red one;
#   * a pair separated by unrelated commits, so a connector has to span them.
#
# Green and red are worth stating precisely, because the fixture only works if
# it produces them deliberately. A square is green when the whole commit can
# fold into one earlier commit: every region it touches leads back to the same
# one, with nobody else having touched those regions in between. It is red when
# no such single commit exists -- usually because the commit changed two
# unrelated things at once.
#
# Everything on `main` is the starting point, so the matrix shows only the
# commits under discussion. A commit that creates a file touches every column it
# creates at once, which is a confusing first thing to look at.
#
# Files stay small: a diff has to be legible at 26pt on a 1080p screen, and a
# viewer pausing on a frame should be able to read every line.
#
# Usage:
#   demo/tutorials/01-matrix/make-repo.sh [OUTPUT_DIR]
set -euo pipefail

OUT="${1:-}"
if [[ -z "$OUT" ]]; then
    OUT="$(mktemp -d -t git-tailor-tutorial-XXXXXX)"
elif [[ -e "$OUT" ]]; then
    if [[ -n "$(ls -A "$OUT" 2>/dev/null)" && "${FORCE:-0}" != "1" ]]; then
        echo "error: '$OUT' exists and is not empty (set FORCE=1 to overwrite)" >&2
        exit 1
    fi
    rm -rf "${OUT:?}"
    mkdir -p "$OUT"
else
    mkdir -p "$OUT"
fi

BASE_TS=1700000000
SEQ=0

git -C "$OUT" init -q -b main
git -C "$OUT" config user.name "Ada Lovelace"
git -C "$OUT" config user.email "ada@example.com"

write() {
    local full="$OUT/$1"
    mkdir -p "$(dirname "$full")"
    cat >"$full"
}

commit() {
    local ts=$((BASE_TS + SEQ * 3600))
    SEQ=$((SEQ + 1))
    git -C "$OUT" add -A
    GIT_AUTHOR_DATE="$ts +0000" GIT_COMMITTER_DATE="$ts +0000" \
        git -C "$OUT" commit -q -m "$1"
}

# ---------------------------------------------------------------------------
# The starting point, on main.
#
# Each file has two regions far enough apart to stay separate columns. The
# spacer comments are load-bearing: regions that touch merge into one column.
# ---------------------------------------------------------------------------
server_rs() {
    cat <<RS
pub fn start(port: u16) -> Server {
    let addr = format!("0.0.0.0:{port}");
    Server::bind(&addr)
}

// -------------------------------------------------------------
// Everything below is the request path; above is startup. The gap
// keeps the two apart in the hunk group matrix.
// -------------------------------------------------------------

pub fn handle(req: Request) -> Response {
    let body = render(req.path());
    Response::ok(${1:-body})
}
RS
}

config_rs() {
    cat <<RS
pub struct Config {
    pub port: u16,
    pub host: String,
}

// -------------------------------------------------------------

impl Config {
    pub fn load() -> Config {
        Config { port: ${1:-8080}, host: "${2:-localhost}".into() }
    }
}
RS
}

log_rs() {
    cat <<RS
pub fn info(msg: &str) {
    println!("[info] {msg}");
}

// -------------------------------------------------------------

pub fn warn(msg: &str) {
    ${1:-println!}("[${2:-warn}] {msg}");
}
RS
}

cat >"$OUT/README.md" <<'MD'
# tinyserve

A very small HTTP server.
MD
server_rs | write src/server.rs
config_rs | write src/config.rs
log_rs | write src/log.rs
commit "feat: add the server, config and logging"

git -C "$OUT" checkout -q -b work

# --- 1. config's load region. First toucher there: a white square. ----------
config_rs 3000 | write src/config.rs
commit "fix: use port 3000 by default"

# --- 2. the same region again, nothing in between. Folds into 1: green. -----
config_rs 3000 "127.0.0.1" | write src/config.rs
commit "fix: bind to loopback, not every interface"

# --- 3. log's warn region. Its own first toucher, another column. -----------
log_rs "eprintln!" | write src/log.rs
commit "fix: send warnings to stderr"

# --- 4. server's request region. Another column again. ---------------------
server_rs "body.trim()" | write src/server.rs
commit "fix: trim trailing whitespace from responses"

# --- 5. the same line as 3, with 4 in between. This is the green connector the
#        tutorial explains, so it matters that this commit touches *only* that
#        one region: a commit whose regions lead back to two different places
#        cannot fold into either, and its connectors go red instead.
log_rs "eprintln!" "warning" | write src/log.rs
commit "style: spell the warning prefix out in full"

# --- 6. two regions at once, leading back to two different commits. There is
#        no single commit this can fold into: red.
config_rs 3000 "0.0.0.0" | write src/config.rs
server_rs "body.trim().to_string()" | write src/server.rs
commit "refactor: tidy up config and responses"

# --- 7. a quiet one, related to nothing above it. --------------------------
cat >>"$OUT/README.md" <<'MD'

Run with `cargo run` and open http://localhost:3000.
MD
commit "docs: explain how to run it"

git -C "$OUT" checkout -q work
echo "$OUT"
