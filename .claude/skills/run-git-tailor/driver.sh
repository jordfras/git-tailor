#!/usr/bin/env bash
# Drive the gt TUI inside a detached tmux session.
#
#   driver.sh start REPO [GT_ARGS...]   launch gt in REPO, wait for the commit list
#   driver.sh keys KEY...               send tmux keys, one per step, 0.3s apart
#   driver.sh wait TEXT [SECS]          block until TEXT is on screen (default 10s)
#   driver.sh screen                    print the screen, blank dialog rows dropped
#   driver.sh stop                      kill the session
#
# GT defaults to target/debug/gt of the repo this script lives in.
set -euo pipefail

SESSION=${GT_SESSION:-gt}
HERE=$(cd "$(dirname "$0")" && pwd)
GT=${GT:-$(git -C "$HERE" rev-parse --show-toplevel)/target/debug/gt}

wait_for() {
    local text=$1 secs=${2:-10}
    if ! timeout "$secs" bash -c \
        "until tmux capture-pane -t '$SESSION' -p | grep -qF -- \"\$0\"; do sleep 0.2; done" "$text"; then
        echo "driver: '$text' did not appear within ${secs}s; screen was:" >&2
        tmux capture-pane -t "$SESSION" -p >&2
        return 1
    fi
}

case ${1:-} in
start)
    repo=$2; shift 2
    tmux kill-session -t "$SESSION" 2>/dev/null || true
    # gt reads GIT_EDITOR before core.editor, and a tmux server keeps the
    # environment it started with, so pass the caller's value in explicitly.
    env_args=()
    [ -n "${GIT_EDITOR+set}" ] && env_args=(-e "GIT_EDITOR=$GIT_EDITOR")
    # 140x30 fits the commit list, the fragmap and every dialog.
    tmux new-session -d -s "$SESSION" -x 140 -y 30 -c "$repo" ${env_args[@]+"${env_args[@]}"} "$GT $*"
    # The status line always ends with the help hint once the list is drawn.
    wait_for "Press 'h' for help"
    ;;
keys)
    shift
    for key in "$@"; do
        tmux send-keys -t "$SESSION" "$key"
        sleep 0.3
    done
    ;;
wait)
    wait_for "$2" "${3:-10}"
    ;;
screen)
    tmux capture-pane -t "$SESSION" -p | grep -v '^ *│ *$'
    ;;
stop)
    tmux kill-session -t "$SESSION" 2>/dev/null || true
    ;;
*)
    sed -n '2,9p' "$0" >&2
    exit 2
    ;;
esac
