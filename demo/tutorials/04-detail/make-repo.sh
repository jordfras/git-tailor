#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 Thomas Johannesson
#
# Build the repository the detail-view tutorial films on.
#
# This one is shaped for *reading* rather than for operations, so what it needs
# is a commit worth opening:
#
#   * four files in one commit, so jumping file to file has somewhere to jump;
#   * a diff longer than the screen, so scrolling is a real problem and not a
#     demonstration of a scrollbar;
#   * a word that occurs in several files, so a search has more than one match
#     to step through.
#
# Files stay small enough to read at 26pt: a viewer pausing on a frame should be
# able to follow the diff line by line.
#
# Usage:
#   demo/tutorials/04-detail/make-repo.sh [OUTPUT_DIR]
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

# `timeout` is the word the search scene looks for: it appears in all four
# files, so stepping between matches crosses file boundaries.
server_rs() {
    cat <<RS
pub fn start(port: u16, timeout: Duration) -> Server {
    let addr = format!("0.0.0.0:{port}");
    Server::bind(&addr)${1:-}
}

// -------------------------------------------------------------

pub fn handle(req: Request) -> Response {
    let body = render(req.path());
    Response::ok(${2:-body})
}
RS
}

config_rs() {
    cat <<RS
pub struct Config {
    pub port: u16,
    pub host: String,
    pub timeout: Duration,
}

// -------------------------------------------------------------

impl Config {
    pub fn load() -> Config {
        Config {
            port: ${1:-8080},
            host: "${2:-localhost}".into(),
            timeout: Duration::from_secs(${3:-30}),
        }
    }
}
RS
}

# The two changed lines sit far enough apart to be two hunks at three lines of
# context and one at five. That gap is what the context scene is about: with the
# functions crammed together, + and - have nothing to merge or separate, and the
# scene demonstrates nothing at all.
client_rs() {
    cat <<RS
pub fn get(url: &str, timeout: Duration) -> Result<Response> {
    let req = Request::get(url)${1:-};
    send(req)
}

// -------------------------------------------------------------
// Everything below is the POST path. The gap is this wide on
// purpose: two hunks at three lines of context, one at five, which
// is what the tutorial's + and - keys are there to show.
// -------------------------------------------------------------

pub fn post(url: &str, body: &[u8], timeout: Duration) -> Result<Response> {
    let req = Request::post(url).body(body)${1:-};
    send(req)
}
RS
}

log_rs() {
    cat <<RS
pub fn info(msg: &str) {
    println!("[info] {msg}");
}

// -------------------------------------------------------------

pub fn timeout_warning(secs: u64) {
    ${1:-println!}("[warn] ${2:-request exceeded {secs}s}");
}
RS
}

cat >"$OUT/README.md" <<'MD'
# tinyserve

A very small HTTP server.
MD
server_rs | write src/server.rs
config_rs | write src/config.rs
client_rs | write src/client.rs
log_rs | write src/log.rs
commit "feat: add the server, config, client and logging"

git -C "$OUT" checkout -q -b work

config_rs 3000 | write src/config.rs
commit "fix: use port 3000 by default"

log_rs "eprintln!" | write src/log.rs
commit "fix: send warnings to stderr"

# The commit the tutorial opens: four files, one idea, and long enough that the
# diff does not fit on screen.
server_rs '.with_timeout(timeout)' "body.trim()" | write src/server.rs
config_rs 3000 "127.0.0.1" 5 | write src/config.rs
client_rs '.timeout(timeout)' | write src/client.rs
log_rs "eprintln!" "request exceeded the {secs}s timeout" | write src/log.rs
commit "feat: carry a request timeout through the stack"

cat >>"$OUT/README.md" <<'MD'

Run with `cargo run` and open http://localhost:3000.
MD
commit "docs: explain how to run it"

git -C "$OUT" checkout -q work
echo "$OUT"
