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
            process.poll()
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
