#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 Thomas Johannesson
#
# Build the repository the work-in-progress tutorial films on.
#
# This fixture is the only one that ships a *dirty* working tree, because the
# video is about the two rows a dirty tree puts above the commits:
#
#   * a staged edit and an unstaged edit, in different files, so the two rows
#     are separable and each can be folded on its own;
#   * the unstaged edit belongs to a commit several rows up, which is the fold
#     the video exists to show;
#   * two fixup! commits waiting for autofixup, naming their targets the way
#     `git commit --fixup` writes them;
#   * a pair that collides, so the conflict scene has something real to resolve.
#
# File shapes are the matrix tutorial's, so a viewer arriving from either of the
# earlier videos is looking at a repository they recognize.
#
# Usage:
#   demo/tutorials/03-in-progress/make-repo.sh [OUTPUT_DIR]
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
# Squash and fixup open an editor, and git-tailor has no `vi` fallback.
git -C "$OUT" config core.editor vim
# A real merge tool for the conflict scene. vimdiff is one git-tailor knows
# built in, so this is the whole configuration a viewer would need -- and it
# runs in the terminal, which is the reason to pick it over kdiff3 or meld: the
# recording captures it like anything else rather than a window nobody sees.
git -C "$OUT" config merge.tool vimdiff

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

# --- 1. the commit the unstaged edit belongs to. ----------------------------
config_rs 3000 | write src/config.rs
commit "fix: use port 3000 by default"

# --- 2. and 3. two ordinary commits, so the fold in scene 02 has to reach
#        back past something rather than into the row above it.
log_rs "eprintln!" | write src/log.rs
commit "fix: send warnings to stderr"

server_rs "body.trim()" | write src/server.rs
commit "fix: trim trailing whitespace from responses"

# --- 4. and 5. the fixup! pair autofixup folds away. `git commit --fixup`
#        writes exactly this subject, and that subject is what matches.
log_rs "eprintln!" "warning" | write src/log.rs
commit "fixup! fix: send warnings to stderr"

server_rs "body.trim().to_string()" | write src/server.rs
commit "fixup! fix: trim trailing whitespace from responses"

# --- 6. the conflict scene's source. It belongs with the commit that
#        introduced the warning, and folding it there has to reach past the
#        fixup! that renamed the prefix -- so the two disagree about the same
#        line and collide. Changing the macro rather than the prefix is what
#        makes them disagree: two commits that both end at the same text merge
#        cleanly, however far apart they are.
log_rs "log::warn!" "warning" | write src/log.rs
commit "refactor: send warnings through the logger"

# --- The dirty working tree the video is about. ----------------------------
#
# Staged: a change to the response path, which belongs where it is.
# Unstaged: the host, which belongs back in commit 1 — the fold the video
# shows. Two files, so the rows separate cleanly.
#
# Deliberately *not* log.rs: scene 05 drops the last commit, which is a log.rs
# commit, and an auto-stash holding a change to the same line cannot be
# reapplied over the result -- the scene would end on "auto-stash NOT restored"
# instead of the successful operation it exists to show.
server_rs "body.trim().to_owned()" | write src/server.rs
git -C "$OUT" add src/server.rs

config_rs 3000 "127.0.0.1" | write src/config.rs

git -C "$OUT" checkout -q work
echo "$OUT"
