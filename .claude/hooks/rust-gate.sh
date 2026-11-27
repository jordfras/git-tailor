#!/usr/bin/env bash
# Enforces the Code Quality rules in CLAUDE.md from the Claude Code harness.
#
#   rust-gate.sh stop    Stop hook: fmt + clippy whenever Rust files have changed.
#   rust-gate.sh commit  PreToolUse(Bash) hook: fmt + clippy + nextest before a
#                        `git commit`. A `test:` commit is a TDD red step, so its
#                        tests must fail; every other commit's tests must pass.
#
# Exit 2 is the harness's "block": it refuses the stop or the tool call and
# hands stderr to Claude as the reason.
set -uo pipefail

cd "${CLAUDE_PROJECT_DIR:-$(git rev-parse --show-toplevel)}" || exit 1
export CARGO_TERM_COLOR=never

rust_paths=('*.rs' 'Cargo.toml' 'Cargo.lock')
tail_lines=60

block() {
    printf '%s\n' "$1" >&2
    exit 2
}

clippy() {
    cargo clippy --all-targets --message-format=short -- -D warnings 2>&1
}

stop_mode() {
    [[ -n $(git status --porcelain -- "${rust_paths[@]}") ]] || exit 0

    cargo fmt >/dev/null 2>&1
    local out
    if ! out=$(clippy); then
        block "cargo clippy --all-targets -- -D warnings failed. Fix it before ending the turn:
$(tail -n "$tail_lines" <<<"$out")"
    fi
}

commit_mode() {
    if ! command -v jq >/dev/null; then
        echo "rust-gate: jq is not installed, so commits are not being checked" >&2
        exit 1
    fi
    local cmd
    cmd=$(jq -r '.tool_input.command // empty' | tr -d '\r')
    grep -Eq '(^|[;&|(])[[:space:]]*git([[:space:]]+-C[[:space:]]+[^[:space:]]+)?[[:space:]]+commit([[:space:]]|$)' \
        <<<"$cmd" || exit 0

    # A `git add` or `commit -a` in the same command stages files only after
    # this hook has run, so the index alone would understate the commit.
    local changed
    if grep -Eq 'git[[:space:]]+add|commit.*[[:space:]](-[[:alpha:]]*a[[:alpha:]]*|--all)([[:space:]]|$)' <<<"$cmd"; then
        changed=$(git status --porcelain -- "${rust_paths[@]}")
    else
        changed=$(git diff --cached --name-only -- "${rust_paths[@]}")
    fi
    [[ -n $changed ]] || exit 0

    local prefix
    prefix=$(grep -oE '\b(feat|fix|test|refactor|docs|chore|tasks):' <<<"$cmd" | head -n 1)
    if [[ -z $prefix && $cmd == *--amend* ]]; then
        prefix=$(git log -1 --format=%s | grep -oE '^[a-z]+:')
    fi

    local out
    if ! out=$(cargo fmt --check 2>&1); then
        block "cargo fmt --check failed; run cargo fmt and commit again:
$(tail -n "$tail_lines" <<<"$out")"
    fi
    if ! out=$(clippy); then
        block "cargo clippy --all-targets -- -D warnings failed:
$(tail -n "$tail_lines" <<<"$out")"
    fi

    out=$(cargo nextest run --no-fail-fast --status-level fail --final-status-level fail 2>&1)
    local status=$?
    # nextest exits 100 when tests ran and some failed; anything else nonzero
    # means the run never got that far (build failure, setup error).
    if [[ $prefix == test: ]]; then
        case $status in
            100) ;;
            0) block "This is a test: commit, a TDD red step, but every test passes. Its new test must fail before the fix lands." ;;
            *) block "cargo nextest run failed to run (exit $status). A test: commit must build and then fail:
$(tail -n "$tail_lines" <<<"$out")" ;;
        esac
    elif ((status != 0)); then
        block "cargo nextest run failed (exit $status). Only a test: commit, a TDD red step, may have failing tests:
$(tail -n "$tail_lines" <<<"$out")"
    fi
}

case ${1:-} in
    stop) stop_mode ;;
    commit) commit_mode ;;
    *) echo "usage: $0 stop|commit" >&2; exit 1 ;;
esac
exit 0
