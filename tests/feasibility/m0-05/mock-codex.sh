#!/usr/bin/env bash
# Throwaway mock of `codex` for local self-testing of run-m0-05.sh mechanics.
# Emits the same JSONL event shape and produces the exact expected file content.
# Invoked as: codex [--version] | [login status] | exec [args...]
case "$1" in
  --version) echo "codex-cli 0.167.0-mock"; exit 0 ;;
  login) echo "Logged in using ChatGPT (mock)"; exit 0 ;;
  exec)
    REPO=""; OUT=""; PREV=""
    for a in "$@"; do
      case "$PREV" in
        "-C") REPO="$a" ;;
        "-o") OUT="$a" ;;
      esac
      PREV="$a"
    done
    python3 - "$REPO" "$OUT" <<'PY'
import json, sys
repo, out = sys.argv[1], sys.argv[2]
open(repo + "/TASK.txt", "w").write("M0_TASK_AFTER\n")
open(out, "w").write("TASK_DEMO_OK\n")
ev = [
  {"type": "thread.started", "thread_id": "mock-00000000-0000-0000-0000-000000000000"},
  {"type": "turn.started"},
  {"type": "item.completed", "item": {"id": "item_0", "type": "agent_message", "text": "TASK_DEMO_OK"}},
  {"type": "turn.completed", "usage": {"input_tokens": 1, "output_tokens": 2}},
]
sys.stdout.write("\n".join(json.dumps(e) for e in ev) + "\n")
PY
    exit 0 ;;
esac
echo "mock codex: unknown subcommand $1" >&2
exit 2
