# M0-02 Feasibility Evidence: Subprocess Launch, Streaming, Termination

Date: 2026-11-07T10:58-07:00 (local, America/Denver)
Platform: native Windows (see below). macOS: BLOCKED, no native macOS host available to this worker.

## Objective

Prove subprocess launch, stdout/stderr streaming, and termination for the
Rust engine (`orchestrator-runtime`) on native Windows and native macOS.
M0-02 board details: "Exercise subprocess launch, output and termination on
native macOS and Windows." Plan section on crash recovery also requires
"Correct cancellation and process cleanup on both operating systems" and
warns that "a process exit code of zero does not prove task completion" and
"an existing process ID alone does not prove that the original agent is
still running."

## Method

Self-contained std-only Rust probe at `tests/feasibility/m0-02/` (standalone
package, not a workspace member, no dependencies). The binary spawns itself
in child modes, so no external fixtures or installation are needed. Four
parent-side scenarios, each verified by parsing child-reported state, not by
exit code alone:

- `launch-stream`: one child, both pipes read concurrently on separate
  threads; asserts exit 0, all 5 expected lines, child-reported pid matches
  the pid from `Command`, and stderr stayed empty.
- `stderr-exit`: child exits 7 with a 16 KB+ stderr payload; asserts the
  exact exit code, payload size, and that stdout stayed clean.
- `fanout`: 4 concurrent children; asserts 4 distinct pids and that each
  child's self-reported pid matches the pid the parent recorded.
- `kill-tree`: two-level tree (probe -> spawn-kid child -> sleeping
  grandchild, real descendant processes). Parent kills the child's tree,
  then re-checks the grandchild pid via `tasklist` (Windows) / `kill -0`
  (Unix) until gone. Asserts both child and grandchild are dead.

`kill-tree` kills the child's tree with `taskkill /F /T` on Windows. On Unix the probe attempts to create a separate process group and send SIGKILL
to that group. That path has not been run on macOS; see Limitations. This is a
feasibility probe, not production supervision: the plan defers Job Objects
to the first production implementation and this probe only proves the
primitive works here.

## Environment (native Windows run)

- Host: X-LT-JMCKERRA, Microsoft Windows 11 Education, build 10.0.26200
- rustc 1.96.0 (ac68faa20 2026-05-25), stable-x86_64-pc-windows-msvc
- Repo: `C:\Users\jmckerra\Code\agent-orch-m0-02-priority2`, branch
  `codex/m0-02-priority2`, base `0a99421fca233399da95f7d2974fc5d9bf5a00ed`
  (M0-01 complete at HEAD)

## Commands

```
cargo build --release --manifest-path tests/feasibility/m0-02/Cargo.toml
tests/feasibility/m0-02/target/release/m0-02-probe.exe
```

## Result (native Windows)

```
TEST=launch-stream PASS stdout_len=114 exit=0 child pid=41280
TEST=stderr-exit PASS exit=7 stderr_len=16484 stdout_len=0
TEST=fanout PASS pids=[11092, 47868, 55820, 12128]
TEST=kill-tree PASS child pid=22460 grandchild pid=17648 both gone after tree kill
RESULT ALL PASS
```

Exit code 0. Pids vary per run; the lines above are from the final 2026-11-07 Windows run before the candidate commit.

## Windows findings

- `std::process::Command` spawn, piped stdout/stderr, concurrent drain on
  two threads, and `wait()` all work as expected on this host.
- Non-zero exit codes and large stderr payloads survive intact
  (`stderr_len=16484` = 4 x 4096 'x' blocks + 4 line headers).
- `taskkill /F /T /PID <n>` kills a whole two-level process tree; the
  grandchild (a real `m0-02-probe.exe` descendant, pid 29088) was
  confirmed gone by `tasklist /FI "PID eq <pid>"` polling.
- Grandchild was created with `CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP`
  via `creation_flags`; a console child with no window is the shape the
  supervisor will need for detached agent processes.
- The probe's own grandchild spawn also uses the same flags, which is why
  `taskkill /T` found the full two-level tree.

## macOS: BLOCKED

No macOS host is reachable from this worker (Windows-only workstation, no
SSH target). Nothing below has been executed; the commands are the exact
repro for a native macOS machine.

```
cd tests/feasibility/m0-02
cargo build --release
./target/release/m0-02-probe
# expected on success:
# TEST=launch-stream PASS ...
# TEST=stderr-exit PASS exit=7 ...
# TEST=fanout PASS ...
# TEST=kill-tree PASS ...
# RESULT ALL PASS   (exit 0)
```

Known macOS-specific gaps that this probe's Unix path does not yet close,
which the macOS run must confirm:

- macOS run must confirm: the child is spawned in a new process group
  (`setpgid(0, 0)` in `pre_exec`) and the kill targets that group
  (`kill -9 -<pid>`), per the plan's "process-group and session semantics".
  If `kill-tree` still fails on macOS, that is a process-group defect in
  this probe, not a spawn or streaming defect.

## Conclusion

- Windows (native): PASS for launch, stdout/stderr streaming, exit-code
  fidelity, concurrent fan-out, and two-level tree termination.
- macOS: no native evidence produced by this worker; exact repro commands
  recorded above. M0-02 is partially satisfied and the M0 exit criterion
  ("a single task runs successfully on both OSes") still requires the
  native macOS run.

## Limitations

- Feasibility probe only. No Job Objects or supervisor loop; Unix process-group
  creation has not been verified on macOS; production supervision is explicitly out of scope.
- Exit status of a killed child is not asserted (killed processes have
  platform-dependent statuses); liveness is asserted by pid re-check
  instead, per the plan's warning that process state must be verified, not
  inferred.
- Windows evidence is from one host, one build (10.0.26200). No CI or
  second machine.
- The probe is std-only Rust; the production runtime will use tokio. The
  primitives are the same (`std::process` is what tokio wraps), but
  async-specific behavior (half-closed pipes, cancellation) is not
  exercised here.
