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

mkdir -p "$OUT_DIR"
REPO="$(mktemp -d "${TMPDIR:-/tmp}/agent-orch-m0-05-task-XXXXXX")"
{
  echo "host=$(hostname)"
  echo "os=$(sw_vers -productVersion) $(uname -m)"
  echo "codex_path=$CODEX"
  echo "codex_version=$("$CODEX" --version 2>&1)"
  echo "git_version=$(git --version 2>&1)"
  echo "bash=$BASH_VERSION"
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
printf 'M0_TASK_BEFORE\n' > "$REPO/TASK.txt"
git -C "$REPO" add TASK.txt
git -C "$REPO" commit -q -m "init: TASK.txt with M0_TASK_BEFORE"
git -C "$REPO" rev-parse HEAD > "$OUT_DIR/init-commit.txt"

# Fixed task run: workspace-write sandbox confined to the temp repo via -C.
# stdin from /dev/null so a bare `codex exec` never blocks on an interactive read.
"$CODEX" exec --json --color never \
  -C "$REPO" \
  --sandbox workspace-write \
  -o "$OUT_DIR/last-message.txt" \
  "$PROMPT" < /dev/null > "$OUT_DIR/stdout.jsonl" 2> "$OUT_DIR/stderr.txt"
echo "$?" > "$OUT_DIR/exit.txt"

git -C "$REPO" status --porcelain=v1 > "$OUT_DIR/post-status.txt"
git -C "$REPO" diff > "$OUT_DIR/post-diff.txt"
git -C "$REPO" diff --check > "$OUT_DIR/diff-check.txt" 2>&1
# xxd is not on stock macOS; use od for the byte dump (space-free hex).
od -An -v -tx1 "$REPO/TASK.txt" | tr -d ' \n' > "$OUT_DIR/task-file-hex.txt"
echo >> "$OUT_DIR/task-file-hex.txt"

git -C "$PRIMARY" status --porcelain=v1 -b > "$OUT_DIR/primary-status-after.txt"

# Verification (python3 ships with macOS).
python3 - "$OUT_DIR" "$REPO" <<'PY'
import json, sys
out, repo = sys.argv[1], sys.argv[2]
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
# diff --git lines: "diff --git a/TASK.txt b/TASK.txt" -> last token "b/TASK.txt"
changed = [l.split()[-1][2:] for l in
           open(f"{out}/post-diff.txt", encoding="utf-8-sig").read().splitlines()
           if l.startswith("diff --git")]
check("only-task-txt-changed", changed == ["TASK.txt"], f"changed={changed}")
before = open(f"{out}/primary-status-before.txt").read().splitlines()
after = open(f"{out}/primary-status-after.txt").read().splitlines()
tracked = lambda ls: [l for l in ls if not l.startswith("??") and not l.startswith("##")]
new_untracked = [l for l in after if l.startswith("??") and l not in before]
iso = tracked(before) == tracked(after) and not new_untracked
check("primary-checkout-isolated", iso,
      f"tracked-identical={tracked(before) == tracked(after)}, "
      f"new-untracked={new_untracked!r}")

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
