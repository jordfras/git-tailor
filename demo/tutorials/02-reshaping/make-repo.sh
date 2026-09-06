#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 Thomas Johannesson
#
# Build the repository the reshaping tutorial films on.
#
# Shaped for the operations performed on it, one obvious target per operation:
#
#   * a commit with a message worth rewording;
#   * a debugging commit worth dropping;
#   * a documentation commit that belongs at the other end, and touches nothing
#     else, so moving it can never conflict on camera;
#   * a pair touching one region with nothing in between, which squashes;
#   * a commit touching three regions that lead back to three different places,
#     so splitting it per hunk group produces three independent pieces;
#   * a commit touching one region alone, which per hunk group *refuses* -- the
#     limitation the tutorial explains.
#
# The file shapes are the matrix tutorial's, so a viewer arriving from it is
# looking at a repository they recognize. The spacer comments are load-bearing
# there and here: regions that touch merge into one column.
#
# Two modes. Without an argument the branch is as the split scene finds it; with
# `after-split`, commit 6 is already the three commits that scene leaves behind,
# so the scenes that fold and reword those pieces can start from them. Every
# scene gets a fresh repository, so state cannot be carried from one to the next
# any other way -- and building both shapes from the same file contents is what
# keeps the pieces identical to the ones the real split produces.
#
# Usage:
#   demo/tutorials/02-reshaping/make-repo.sh [OUTPUT_DIR] [after-split]
set -euo pipefail

OUT="${1:-}"
MODE="${2:-whole}"
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
# Reword and squash open an editor, and git-tailor has no `vi` fallback: with
# nothing configured they fail rather than guess.
git -C "$OUT" config core.editor vim

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

# --- 1. config's load region: first toucher. --------------------------------
config_rs 3000 | write src/config.rs
commit "fix: use port 3000 by default"

# --- 2. the same region, nothing in between: squashes into 1. In
#        `after-folds` this commit is what scene 04 leaves behind: the config
#        piece squashed in, under the message that scene writes for it.
if [ "$MODE" = after-folds ]; then
    config_rs 3000 "0.0.0.0" | write src/config.rs
    commit "fix: bind to every interface, not just loopback"
else
    config_rs 3000 "127.0.0.1" | write src/config.rs
    commit "fix: bind to loopback, not every interface"
fi

# --- 3. log's warn region: its own column. In `after-folds` the logging piece
#        is already fixed up into it -- the content changes, the message does
#        not, which is what fixup means.
if [ "$MODE" = after-folds ]; then
    log_rs "eprintln!" "warning" | write src/log.rs
else
    log_rs "eprintln!" | write src/log.rs
fi
commit "fix: send warnings to stderr"

# --- 4. the one to reword. The message says nothing. ------------------------
server_rs "body.trim()" | write src/server.rs
commit "fix stuff"

# --- 5. the one to drop. Deliberately the *newest* toucher of the request
#        path: nothing after it depends on its lines, so the drop replays the
#        rest cleanly. A dropped commit that conflicts is a fine thing for the
#        tool to handle and a poor thing to film.
server_rs 'body.trim() /* dbg!(req.path()) */' | write src/server.rs
commit "debug: print the request path"

# --- 6. three regions at once, leading back to three different places: the
#        split-per-hunk-group demonstration. Config's load leads back to 2,
#        log's warn to 3, and the server's startup line to nobody, so the
#        three pieces come out independent of each other.
#        In `after-split` mode the same three changes are three commits, named
#        the way the split names them.
if [ "$MODE" = after-folds ]; then
    # Pieces 1 and 2 are already in the commits above; only the piece that
    # folds nowhere is still its own commit, which is what scene 05 rewords.
    sed -i 's/Server::bind(&addr)/Server::bind(\&addr).expect("bind")/' "$OUT/src/server.rs"
    commit "refactor: tidy up config, logging and startup (3/3)"
elif [ "$MODE" = after-split ]; then
    config_rs 3000 "0.0.0.0" | write src/config.rs
    commit "refactor: tidy up config, logging and startup (1/3)"
    log_rs "eprintln!" "warning" | write src/log.rs
    commit "refactor: tidy up config, logging and startup (2/3)"
    sed -i 's/Server::bind(&addr)/Server::bind(\&addr).expect("bind")/' "$OUT/src/server.rs"
    commit "refactor: tidy up config, logging and startup (3/3)"
else
    config_rs 3000 "0.0.0.0" | write src/config.rs
    log_rs "eprintln!" "warning" | write src/log.rs
    sed -i 's/Server::bind(&addr)/Server::bind(\&addr).expect("bind")/' "$OUT/src/server.rs"
    commit "refactor: tidy up config, logging and startup"
fi

# --- 7. one region, related to nothing: per hunk group has nothing to work
#        with, and says so. Also the commit the tutorial moves, since it
#        shares no file with anything above it.
cat >>"$OUT/README.md" <<'MD'

Run with `cargo run` and open http://localhost:3000.
MD
commit "docs: explain how to run it"

git -C "$OUT" checkout -q work
echo "$OUT"
