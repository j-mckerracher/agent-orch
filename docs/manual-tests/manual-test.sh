#!/bin/sh
# Helpers for the manual UI tests. See docs/manual-tests/00-setup.md. Run from the repository root.
#
#   sh docs/manual-tests/manual-test.sh fixture           Recreate the two test projects.
#   sh docs/manual-tests/manual-test.sh capture           Log agent command lines until Ctrl-C.
#   sh docs/manual-tests/manual-test.sh hold <marker>     Pause task runs whose prompt has <marker>
#                                                         until Ctrl-C. Paused runs stay paused.
#   sh docs/manual-tests/manual-test.sh release <marker>  Let those task runs continue.
#
# Nothing here replaces or wraps an agent. The app starts the real CLI; these helpers only create
# the fixture repositories, read process arguments, and pause or continue processes.
set -eu

root="${HOME:?}/agent-orch-manual-test"

# Task runs carry --session-id (a new run) or --resume (a resumed run). Chat turns carry neither.
task_pids() {
  for pid in $(pgrep -f -- "$1" || true); do
    if ps -ww -o args= -p "$pid" 2>/dev/null | grep -q -e '--session-id' -e '--resume'; then
      echo "$pid"
    fi
  done
}

case "${1:-}" in
  fixture)
    for name in manual-test-project other-project; do
      dir="$root/projects/$name"
      rm -rf "${dir:?}"
      mkdir -p "$dir"
      (
        cd "$dir"
        git init -q -b main
        printf '# %s\n\nThis repository is a fixture for the MCAO manual UI tests.\n' "$name" > README.md
        git add README.md
        GIT_AUTHOR_NAME='MCAO manual test' GIT_AUTHOR_EMAIL='manual-test@example.invalid' \
          GIT_AUTHOR_DATE='2026-01-01T00:00:00Z' GIT_COMMITTER_NAME='MCAO manual test' \
          GIT_COMMITTER_EMAIL='manual-test@example.invalid' GIT_COMMITTER_DATE='2026-01-01T00:00:00Z' \
          git -c commit.gpgsign=false -c core.hooksPath=/dev/null commit -q --no-verify -m 'Add the fixture README'
        printf '%s %s\n' "$name" "$(git rev-parse HEAD)"
      )
    done
    ;;
  capture)
    log="$root/logs/prompts.log"
    mkdir -p "$root/logs"
    echo "Logging agent command lines to $log. Press Ctrl-C to stop."
    seen=" "
    while :; do
      for pid in $(pgrep -f -- '--permission-prompts' || true); do
        case "$seen" in *" $pid "*) continue ;; esac
        seen="$seen$pid "
        {
          printf '=== %s pid %s ===\n' "$(date -u +%Y%m%dT%H%M%SZ)" "$pid"
          ps -ww -o args= -p "$pid" 2>/dev/null || true
        } >> "$log"
      done
      sleep 0.5
    done
    ;;
  hold)
    marker="${2:?usage: manual-test.sh hold <marker>}"
    echo "Pausing task runs whose prompt has $marker. Press Ctrl-C to stop; paused runs stay paused."
    while :; do
      for pid in $(task_pids "$marker"); do
        kill -STOP "$pid" 2>/dev/null || true
      done
      sleep 0.2
    done
    ;;
  release)
    marker="${2:?usage: manual-test.sh release <marker>}"
    for pid in $(task_pids "$marker"); do
      kill -CONT "$pid" 2>/dev/null || true
      echo "Continued $pid"
    done
    ;;
  *)
    sed -n '2,8p' "$0"
    exit 2
    ;;
esac
