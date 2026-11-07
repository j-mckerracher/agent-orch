#!/usr/bin/env bash
# M0-05 single-task feasibility runner (macOS).
# Runs one fixed Codex CLI task in a fresh temporary Git repository and
# verifies every acceptance check, preserving raw artifacts in OUT_DIR.
#
# Usage:
#   bash tests/feasibility/m0-05/run-m0-05.sh <codex path> <primary checkout> <out dir>
#
# Safety: creates its own unique mktemp -d repo, never removes fixed/shared
# directories, never commits into the primary checkout, and passes
# --sandbox workspace-write confined to the temp repo via -C. The prompt is
# read from task-prompt.txt next to this script: the single canonical M0-05
# task, identical on both OSes. Do not edit it per run.
set -u
CODEX="${1:?codex path required}"
PRIMARY="${2:?primary checkout path required}"
OUT_DIR="${3:?output dir required}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROMPT="$(cat "$SCRIPT_DIR/task-prompt.txt")"
if ! command -v python3 >/dev/null 2>&1; then
  echo "Python 3 is required by this runner; install it and retry." >&2
  exit 2
fi

# Collision safety: never reuse or merge into an existing evidence directory.
if [ -e "$OUT_DIR" ]; then
  echo "OutDir already exists; refusing to reuse it: $OUT_DIR" >&2
  exit 2
fi
mkdir -p "$(dirname "$OUT_DIR")"
if ! mkdir "$OUT_DIR"; then
  echo "Could not reserve a fresh OutDir: $OUT_DIR" >&2
  exit 2
fi
REPO="$(mktemp -d "${TMPDIR:-/tmp}/agent-orch-m0-05-task-XXXXXX")"
{
  echo "host=$(hostname)"
  echo "os=$(sw_vers -productVersion) $(uname -m)"
  echo "codex_path=$CODEX"
  echo "codex_version=$("$CODEX" --version 2>&1)"
  echo "git_version=$(git --version 2>&1)"
  echo "bash=$BASH_VERSION"
  echo "python3_version=$(python3 --version 2>&1)"
  echo "primary_checkout=$PRIMARY"
  echo "task_repo=$REPO"
  echo "source_head=$(git -C "$PRIMARY" rev-parse HEAD 2>&1)"
  echo "source_branch=$(git -C "$PRIMARY" rev-parse --abbrev-ref HEAD 2>&1)"
  echo "login_status=$("$CODEX" login status 2>&1)"
} > "$OUT_DIR/environment.txt"

# Primary checkout status BEFORE (isolation proof).
git -C "$PRIMARY" status --porcelain=v1 -b > "$OUT_DIR/primary-status-before.txt"

# Fresh disposable Git repository; LF pinned so byte checks are deterministic.
git -C "$REPO" init -q -b main
git -C "$REPO" config core.autocrlf false
git -C "$REPO" config core.eol lf
# Local identity so the init commit does not depend on host git config.
git -C "$REPO" config user.name "M0-05 Demo"
git -C "$REPO" config user.email "m0-05-demo@agent-orch.local"
printf 'M0_TASK_BEFORE\n' > "$REPO/TASK.txt"
git -C "$REPO" add TASK.txt
git -C "$REPO" commit -q -m "init: TASK.txt with M0_TASK_BEFORE"
git -C "$REPO" rev-parse HEAD > "$OUT_DIR/init-commit.txt"

# Fixed task run: workspace-write sandbox confined to the temp repo via -C.
# stdin from /dev/null so a bare `codex exec` never blocks on an interactive
# read. Bounded (60 s) so a hung CLI can never hang the runner. Python starts
# Codex in a new process group and terminates the whole group on timeout.
RUN_SECONDS=60
python3 - "$RUN_SECONDS" "$OUT_DIR" "$CODEX" exec --json --color never \
  -C "$REPO" --sandbox workspace-write \
  -o "$OUT_DIR/last-message.txt" "$PROMPT" <<'PY'
import os
import signal
import subprocess
import sys
import time

timeout_seconds = int(sys.argv[1])
out_dir = sys.argv[2]
command = sys.argv[3:]
timed_out = False
exit_code = 127
with open(os.path.join(out_dir, "stdout.jsonl"), "wb") as stdout, \
     open(os.path.join(out_dir, "stderr.txt"), "wb") as stderr:
    try:
        child = subprocess.Popen(command, stdin=subprocess.DEVNULL,
                                 stdout=stdout, stderr=stderr,
                                 start_new_session=True)
        try:
            exit_code = child.wait(timeout=timeout_seconds)
        except subprocess.TimeoutExpired:
            timed_out = True
            try:
                os.killpg(child.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            deadline = time.monotonic() + 5
            while child.poll() is None and time.monotonic() < deadline:
                time.sleep(0.05)
            # Also clean descendants that outlive the Codex parent.
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            exit_code = child.wait()
    except OSError as error:
        stderr.write((f"Could not launch Codex: {error}\n").encode("utf-8"))
        exit_code = 127

with open(os.path.join(out_dir, "timeout.txt"), "w", encoding="ascii") as f:
    f.write(f"{str(timed_out).lower()}\n")
with open(os.path.join(out_dir, "exit.txt"), "w", encoding="ascii") as f:
    f.write(f"{exit_code}\n")
sys.exit(0 if exit_code == 0 and not timed_out else 1)
PY
code=$?

git -C "$REPO" status --porcelain=v1 > "$OUT_DIR/post-status.txt"
git -C "$REPO" diff > "$OUT_DIR/post-diff.txt"
git -C "$REPO" diff --check > "$OUT_DIR/diff-check.txt" 2>&1
# xxd is not on stock macOS; use od for the byte dump (space-free hex).
od -An -v -tx1 "$REPO/TASK.txt" | tr -d ' \n' > "$OUT_DIR/task-file-hex.txt"
echo >> "$OUT_DIR/task-file-hex.txt"

git -C "$PRIMARY" status --porcelain=v1 -b > "$OUT_DIR/primary-status-after.txt"

# Verification uses the Python 3 executable recorded above.
python3 - "$OUT_DIR" "$REPO" "$PRIMARY" <<'PY'
import json, sys
import os
out, repo, primary = sys.argv[1:]
checks = []
def check(name, ok, detail=""):
    checks.append((name, bool(ok), detail))

lines = [l for l in open(f"{out}/stdout.jsonl", encoding="utf-8-sig").read().splitlines() if l.strip()]
parsed, all_ok = [], True
for i, l in enumerate(lines, 1):
    try:
        parsed.append(json.loads(l))
    except Exception as e:
        all_ok = False
        checks.append((f"jsonl-line-{i}-parse", False, str(e)))
check("jsonl-all-lines-parse", all_ok and len(parsed) == len(lines), f"lines={len(lines)}")
try:
    exit_code = int(open(f"{out}/exit.txt").read().strip())
except Exception as e:
    exit_code = -1
    checks.append(("exit-status-0", False, f"exit.txt unreadable: {e}"))
check("exit-status-0", exit_code == 0, f"exit={exit_code}")
turns = sum(1 for e in parsed if e.get("type") == "turn.completed")
check("turn-completed-present", turns == 1, f"count={turns}")
last = open(f"{out}/last-message.txt", encoding="utf-8").read().strip("\r\n")
check("last-message-exact", last == "TASK_DEMO_OK", repr(last))
msgs = [e["item"] for e in parsed
        if e.get("type") == "item.completed"
        and e.get("item", {}).get("type") == "agent_message"]
mtxt = msgs[0].get("text", "").strip("\r\n") if len(msgs) == 1 else None
check("single-agent-message-exact", len(msgs) == 1 and mtxt == "TASK_DEMO_OK", repr(mtxt))
hexdump = open(f"{out}/task-file-hex.txt", encoding="utf-8-sig").read().strip().lower()
# "M0_TASK_AFTER\n" in hex: 4d30 5f54 4153 4b5f 4146 5445 520a
check("task-file-exact", hexdump == "4d305f5441534b5f41465445520a", f"hex={hexdump}")
# utf-8-sig: tolerates a BOM so the same check logic runs on captured evidence
# from either OS (macOS git output has no BOM; this is belt-and-suspenders).
diff_check = open(f"{out}/diff-check.txt", encoding="utf-8-sig").read().strip()
check("git-diff-check", diff_check == "", repr(diff_check))
# Status-based change list: catches both tracked changes and untracked files
# (diff only sees tracked). Porcelain v1: XY SP PATH -> take field 2.
changed = [l.split(' ', 2)[2] for l in
           open(f"{out}/post-status.txt", encoding="utf-8-sig").read().splitlines()
           if l.strip()]
check("only-task-txt-changed", changed == ["TASK.txt"], f"changed={changed}")
before = open(f"{out}/primary-status-before.txt").read().splitlines()
after = open(f"{out}/primary-status-after.txt").read().splitlines()
tracked = lambda ls: [l for l in ls if not l.startswith("??") and not l.startswith("##")]
untracked = lambda ls: [l[3:].rstrip("/") for l in ls if l.startswith("??")]
allowed = os.path.relpath(os.path.abspath(out), os.path.abspath(primary)).replace(os.sep, "/")
if allowed == ".." or allowed.startswith("../"):
    allowed = None
before_other = [p for p in untracked(before) if p != allowed]
after_other = [p for p in untracked(after) if p != allowed]
iso = tracked(before) == tracked(after) and before_other == after_other
check("primary-checkout-isolated", iso,
      f"tracked-identical={tracked(before) == tracked(after)}, "
      f"other-untracked-identical={before_other == after_other}")

report = ["M0-05 MACOS RUN: " + ("PASS" if all(c[1] for c in checks) else "FAIL"),
          "", "Run artifacts in the same directory as this file.", "Checks:", ""]
for name, ok, detail in checks:
    report.append(f"{name:<28} {'PASS' if ok else 'FAIL'}  {detail}")
report += [
    "",
    f"Task repo (disposable, created by this run): {repo}",
    f"Codex exit: {exit_code}",
    f"Codex last message: {last!r}",
    f"TASK.txt bytes: {hexdump}",
    f"Primary checkout tracked status identical before/after: {iso}",
]
open(f"{out}/result.md", "w").write("\n".join(report) + "\n")
print("\n".join(report))
sys.exit(0 if all(c[1] for c in checks) else 1)
PY
rc=$?

# Print task repo path for the operator; the repo itself is left in place
# (disposable, unique per run) rather than removed.
echo "task_repo=$REPO"
exit $rc
