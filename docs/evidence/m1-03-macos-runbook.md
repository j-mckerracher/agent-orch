# M1-03 native macOS verification runbook

Updated: 2026-11-07.

## Assignment

Run the complete Rust workspace test suite on native macOS against the exact
M1-03 candidate below. This is required to exercise the Unix process-group
supervision path that Windows cannot compile or test. This task is the
Codex adapter and process supervisor; do not run the separate M0-02 Rust
probe or the M0-05 real Codex demo.

The repository owner delegates this run to the Mac-side agent. The agent is
responsible for checking out the pinned candidate, running the commands,
preserving every attempt, and pushing evidence-only results. Do not edit
source files, Cargo manifests or lockfiles, `board.json`, or the candidate
branch. The implementation coordinator reviews evidence and updates task
status.

## Candidate

- Repository: `https://github.com/j-mckerracher/agent-orch.git`
- Candidate branch: `codex/m1-03-priority3`
- Exact source commit to test: `82023712efb9865e07586d6b6a3b0aa5860a725c`
- This runbook is at `docs/evidence/m1-03-macos-runbook.md` on the candidate
  branch.

## Prepare an isolated checkout and unique output directory

Run these commands in Terminal. They create a fresh candidate checkout and
keep raw test output outside that checkout. A repeated or colliding path is
an error; do not reuse or overwrite an earlier attempt.

```sh
set -eu

RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)-$(uuidgen | tr '[:upper:]' '[:lower:]')"
SOURCE_DIR="/private/tmp/agent-orch-m1-03-candidate-${RUN_ID}"
EVIDENCE_DIR="/private/tmp/agent-orch-m1-03-output-${RUN_ID}"
SOURCE_SHA="82023712efb9865e07586d6b6a3b0aa5860a725c"

if [ -e "$SOURCE_DIR" ] || [ -e "$EVIDENCE_DIR" ]; then
  echo "Refusing to reuse an existing run path: $RUN_ID" >&2
  exit 1
fi

git clone --branch codex/m1-03-priority3 \
  https://github.com/j-mckerracher/agent-orch.git "$SOURCE_DIR"
cd "$SOURCE_DIR"
git switch --detach "$SOURCE_SHA"

ACTUAL_SHA="$(git rev-parse HEAD)"
if [ "$ACTUAL_SHA" != "$SOURCE_SHA" ]; then
  echo "Wrong source commit: expected $SOURCE_SHA, found $ACTUAL_SHA" >&2
  exit 1
fi
if [ -n "$(git status --porcelain)" ]; then
  echo "Candidate checkout is not clean; preserve it and stop." >&2
  git status --short >&2
  exit 1
fi

mkdir "$EVIDENCE_DIR"
```

## Record environment and run the suite

Record the actual toolchain and source revision, then run the exact command
`cargo test --locked --workspace`. The Python wrapper bounds the run to 15
minutes, captures stdout and stderr as bytes, and terminates the isolated
process group on timeout, escalating to SIGKILL if it remains after ten
seconds. Python 3 is used only as a process supervisor; it does not run or
modify the Rust tests.

```sh
{
  echo '$ sw_vers'
  sw_vers
  echo '$ uname -a'
  uname -a
  echo '$ uname -m'
  uname -m
  echo '$ rustc --version'
  rustc --version
  echo '$ cargo --version'
  cargo --version
  echo '$ python3 --version'
  python3 --version
  echo '$ git rev-parse HEAD'
  git rev-parse HEAD
  echo '$ git status --porcelain'
  git status --porcelain
} > "$EVIDENCE_DIR/environment.txt" 2>&1

cat > "$EVIDENCE_DIR/run_tests.py" <<'PY'
import os
import signal
import subprocess
import sys
import time
from pathlib import Path

source = Path(sys.argv[1])
evidence = Path(sys.argv[2])
command = ["cargo", "test", "--locked", "--workspace"]
(evidence / "command.txt").write_text(
    "cargo test --locked --workspace\ntimeout_seconds=900\n",
    encoding="utf-8",
)

timed_out = False
group_killed = False
with (evidence / "stdout.bin").open("wb") as stdout, \
     (evidence / "stderr.bin").open("wb") as stderr:
    process = subprocess.Popen(
        command,
        cwd=source,
        stdout=stdout,
        stderr=stderr,
        start_new_session=True,
    )
    try:
        process.wait(timeout=900)
    except subprocess.TimeoutExpired:
        timed_out = True
        def signal_group(sig):
            try:
                os.killpg(process.pid, sig)
                return True
            except ProcessLookupError:
                return False

        def group_exists():
            try:
                os.killpg(process.pid, 0)
                return True
            except ProcessLookupError:
                return False
            except PermissionError:
                return True

        signal_group(signal.SIGTERM)
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            process.poll()  # reap the direct child before checking for descendants
            if not group_exists():
                break
            time.sleep(0.1)
        if group_exists():
            group_killed = signal_group(signal.SIGKILL)
        process.wait()

(evidence / "exit.txt").write_text(f"{process.returncode}\n", encoding="utf-8")
(evidence / "timed_out.txt").write_text(
    f"{'true' if timed_out else 'false'}\n", encoding="utf-8"
)
(evidence / "group_killed.txt").write_text(
    f"{'true' if group_killed else 'false'}\n", encoding="utf-8"
)
print(
    f"cargo_exit={process.returncode} timed_out={timed_out} "
    f"group_killed={group_killed}"
)
PY

python3 "$EVIDENCE_DIR/run_tests.py" "$SOURCE_DIR" "$EVIDENCE_DIR"
git -C "$SOURCE_DIR" status --short > "$EVIDENCE_DIR/source-status.txt"
```

A passing Cargo test has `exit.txt` set to `0`, `timed_out.txt` set to
`false`, and zero failed tests in `stdout.bin`. The Python supervisor exits
successfully after recording the Cargo result; use the two result files, not
the supervisor's exit status, to classify the run. The workspace includes adapter tests for the
captured Codex JSONL sequence, cancellation, descendants, bounded stdout and
stderr lines, and stopping output events after process exit. On macOS these
tests compile the Unix process-group code and exercise descendant cleanup.

If compilation or any test fails, or the command times out, preserve this
attempt unchanged. Do not edit the candidate or rerun it. Report the failure
in `result.md`, with the actual source SHA, exit code, timeout flag, and
summary of stdout/stderr; publish the captured artifacts so the coordinator
can review them and prepare a new candidate if needed.

For every attempt, create `result.md` in the output directory with the
run date, macOS version/architecture, Rust/Cargo versions, exact source SHA,
command, pass/fail result, exit code, timeout status, and any limitation.
Save the post-run `git status --short` output as `source-status.txt` with the
command above. Keep all attempt directories separate.

Use this `result.md` format:

```markdown
# M1-03 native macOS verification

- Run date:
- macOS version/build and architecture:
- Rust version:
- Cargo version:
- Source commit: `82023712efb9865e07586d6b6a3b0aa5860a725c`
- Command: `cargo test --locked --workspace`
- Result: PASS or FAIL
- Cargo exit code:
- Timed out: true or false
- Process group required SIGKILL: true or false
- Test totals and failures:
- Limitations or failure summary:
```

## Publish evidence only

After recording `result.md` and `source-status.txt`, fetch the current
`origin/main` and create a separate evidence worktree. Do not publish from
the candidate checkout and do not include source, `board.json`, or runbook
changes in the evidence commit.

```sh
git -C "$SOURCE_DIR" fetch origin
EVIDENCE_BRANCH="codex/m1-03-macos-evidence-${RUN_ID}"
EVIDENCE_WT="/private/tmp/agent-orch-m1-03-evidence-${RUN_ID}"

if [ -e "$EVIDENCE_WT" ]; then
  echo "Refusing to reuse evidence worktree path: $EVIDENCE_WT" >&2
  exit 1
fi

git -C "$SOURCE_DIR" worktree add -b "$EVIDENCE_BRANCH" \
  "$EVIDENCE_WT" origin/main
mkdir -p "$EVIDENCE_WT/docs/evidence/m1-03-macos/$RUN_ID"
cp "$EVIDENCE_DIR"/* "$EVIDENCE_WT/docs/evidence/m1-03-macos/$RUN_ID/"

git -C "$EVIDENCE_WT" add "docs/evidence/m1-03-macos/$RUN_ID"
git -C "$EVIDENCE_WT" diff --cached --check
git -C "$EVIDENCE_WT" diff --cached --name-only
git -C "$EVIDENCE_WT" commit -m "evidence: record M1-03 macOS verification"
git -C "$EVIDENCE_WT" push -u origin "$EVIDENCE_BRANCH"
```

Before pushing, verify that the staged paths are only
`docs/evidence/m1-03-macos/$RUN_ID/`. Do not change `board.json`. Report the
evidence branch and commit SHA to the coordinator. Do not force-push. If
worktree creation or push fails, preserve the attempt and report the exact
paths and Git state; do not force-remove or overwrite anything.

## Scope note

The repository-wide `cargo fmt --check` currently reports formatting diffs in
seven files untouched by this candidate. All five Rust files changed by M1-03
pass direct `rustfmt --check`. Do not reformat unrelated files as part of
this native verification.
