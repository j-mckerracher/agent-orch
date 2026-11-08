import subprocess
import sys
from pathlib import Path

source = Path(sys.argv[1])
evidence = Path(sys.argv[2])
command = ["cargo", "test", "--locked", "--workspace"]
timed_out = False
process_tree_killed = False
kill_output = b""
with (evidence / "stdout.bin").open("wb") as stdout, (evidence / "stderr.bin").open("wb") as stderr:
    process = subprocess.Popen(
        command,
        cwd=source,
        stdout=stdout,
        stderr=stderr,
        creationflags=subprocess.CREATE_NEW_PROCESS_GROUP,
    )
    try:
        process.wait(timeout=900)
    except subprocess.TimeoutExpired:
        timed_out = True
        killer = subprocess.run(
            ["taskkill", "/PID", str(process.pid), "/T", "/F"],
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            timeout=30,
            check=False,
        )
        kill_output = killer.stdout
        process_tree_killed = killer.returncode == 0
        process.wait(timeout=30)
(evidence / "exit.txt").write_text(f"{process.returncode}\n", encoding="utf-8")
(evidence / "timed_out.txt").write_text(f"{'true' if timed_out else 'false'}\n", encoding="utf-8")
(evidence / "group_killed.txt").write_text(f"{'true' if process_tree_killed else 'false'}\n", encoding="utf-8")
(evidence / "taskkill-output.bin").write_bytes(kill_output)
print(f"cargo_exit={process.returncode} timed_out={timed_out} process_tree_killed={process_tree_killed}")
