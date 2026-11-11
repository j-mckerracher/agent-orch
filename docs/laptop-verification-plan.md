# Laptop verification plan

Run on the native macOS laptop on 2026-11-07.

1. Inspect toolchains and existing Git remotes, then add `motif-mcao` pointing to https://github.com/mc-innersource/motif-mcao without changing `origin`.
2. Build and test the engine workspace with locked dependencies.
3. Install frontend dependencies from its lockfile, build the frontend, and build the standalone Tauri host with locked dependencies.
4. Launch the desktop app and inspect its window.
5. Exercise native subprocess launch, stdout/stderr capture, and termination with a disposable process group.
6. Inspect the installed Codex CLI and probe one-shot JSON output in a disposable directory with a response-only request.
7. Record actual results and limitations in this document, README.md, and board.json. Keep unfinished milestones open.

## Results

Completed on 2026-11-07 against checkout `0a99421fca233399da95f7d2974fc5d9bf5a00ed`.
No application code or dependency versions were changed for these checks.

Environment: macOS 26.7.1, build 25G241, Darwin arm64; Rust 1.95.0;
Node.js 26.13.0; npm 11.20.0; codex-cli 0.167.0. Xcode command-line tools
are selected at `/Library/Developer/CommandLineTools`.

| Check | Command or procedure | Result |
| --- | --- | --- |
| Engine build | `cargo build --locked --workspace` | Passed. Eight crates compiled. |
| Engine tests | `cargo test --locked --workspace` | Passed. Zero tests across 14 suites, so this establishes compilation rather than runtime coverage. |
| Scaffold CLI | `cargo run --locked -p orchestrator-cli` | Expected exit 1 and explicit message that task execution is not implemented. |
| Frontend dependencies | `npm --prefix apps/desktop ci` | Passed. Installed 28 packages, audited 29, zero known vulnerabilities reported. npm warned that the optional fsevents install script was not approved; no script approval was added. |
| Frontend build | `npm --prefix apps/desktop run build` | Passed TypeScript and Vite production build. |
| Native Tauri build | `cargo build --locked --manifest-path apps/desktop/src-tauri/Cargo.toml` | Passed. 219 crates compiled. |
| Development launch | `npm --prefix apps/desktop run tauri dev` | Vite started at localhost:1420 and Cargo launched the native executable. The UI tool could not locate the unbundled process, so the bundled app was inspected next. The development session was then stopped. |
| Debug app bundle | `npm --prefix apps/desktop run tauri build -- --debug --bundles app` | Passed. Created `apps/desktop/src-tauri/target/debug/bundle/macos/Agent Orchestrator.app`. |
| GUI inspection | Opened the debug bundle using the native UI tool | Passed. Accessibility tree showed window "Agent Orchestrator", HTML content at `tauri://localhost`, and heading "Agent Orchestrator". The app remains open for inspection. |
| Native subprocess | Launched `python3 -u -c` with `start_new_session=True`, captured stdout/stderr, sent SIGTERM to its process group, and waited with a five-second timeout | Passed. stdout `stdout-ready`, stderr `stderr-ready`, return code -15. The disposable child was reaped. |
| Codex one-shot JSON | Command below, followed by parsing every stdout line as JSON | Passed. Exit 0, five JSON events, final response `AGENT_ORCH_PROTOCOL_OK`, and `turn.completed` observed. |
| Additional Git remote | `git remote add motif-mcao https://github.com/mc-innersource/motif-mcao` | Added and verified with `git remote get-url motif-mcao`. `origin` still points to `git@github.com:j-mckerracher/agent-orch.git`. No fetch or push to motif-mcao was performed. |

Codex protocol command:

```sh
codex exec --ephemeral --sandbox read-only --skip-git-repo-check \
  -C /private/tmp/agent-orch-laptop-verification --json --color never \
  -o /private/tmp/agent-orch-laptop-verification/codex-last-message.txt \
  'This is a CLI protocol smoke test, not an implementation assignment. Do not use tools, read files, or change files. Reply with exactly AGENT_ORCH_PROTOCOL_OK.' \
  < /dev/null \
  > /private/tmp/agent-orch-laptop-verification/codex-events.jsonl \
  2> /private/tmp/agent-orch-laptop-verification/codex-stderr.log
```

Observed event types, in order: `thread.started`, `item.completed`,
`turn.started`, `item.completed`, `turn.completed`. No implementation worker
was delegated. Local raw probe artifacts are in
`/private/tmp/agent-orch-laptop-verification`, including `subprocess-result.json`;
that temporary directory is not committed.

Initial attempts under the outer execution sandbox could not resolve the
npm/crates.io registries, update Git config, or initialize Codex's in-process
app-server client. Approved execution outside that outer sandbox succeeded.
The Codex probe itself retained its read-only execution sandbox.
The Tauri CLI rewrote two dependency declarations to equivalent empty-feature
table syntax; those generated formatting changes were reverted after checking.

## Remaining coverage

- The initial smoke checks did not close M0-02 or M0-03. The corrected M0-02
  Rust probe now passes on native macOS; the implementation coordinator owns
  reviewing the evidence and updating its board status. The first compile
  failure is preserved below. M0-03 remains `todo` on main;
  the Windows task-branch evidence has not been integrated.
- The GUI check covers launch and rendering only. The `ping` invoke bridge
  remains unexercised; agent execution, persistence, and isolation are absent
  from the scaffold.
- The debug app bundle is local and unsigned. Release installation, signing,
  upgrades, and clean-machine smoke tests were not exercised.
- M0-05 and the M0 milestone remain incomplete. A successful end-to-end task
  requires implementation beyond this scaffold and evidence on both platforms.

## Follow-up from remote task instructions

On 2026-11-07, fetched `origin` and read the new native macOS instructions on
`origin/codex/m0-02-priority2` and `origin/codex/m0-03-priority2`.
The M0-02 runbook explicitly requires its Rust probe, rather than the earlier
Python OS-primitive check, and says the Codex check need not be repeated for
this follow-up.

The initial follow-up used a separate worktree of the M0-02 task branch at
`8e213e4` and ran the unchanged Rust probe with
`cargo run --release --locked --offline`. It preserved the environment and
complete output in the evidence commit described below; the probe was not
modified or rerun after that failure.

The first Rust probe run failed to compile with exit 101: `src/main.rs:306`
called unsafe `CommandExt::pre_exec` without an enclosing unsafe block. No
scenarios ran. The task branch now fixes this with safe
`CommandExt::process_group(0)`; its Windows rerun passed all four scenarios.
That Windows run did not verify the Unix-only code, so M0-02 remained blocked
pending the native macOS retry recorded below. The first failure and its complete raw artifacts
remain preserved in [the Rust probe report](evidence/m0-02-macos.md).

## Updated retry instructions

The Mac verification agent was instructed to fetch the latest
`codex/m0-02-priority2` branch and follow the agent-facing
[M0-02 macOS runbook](https://github.com/j-mckerracher/agent-orch/blob/codex/m0-02-priority2/docs/evidence/m0-02-macos-runbook.md).
The runbook identifies the corrected probe, capture paths, and evidence-only
publish procedure. Do not run the earlier source revision or edit the probe.

## Corrected Rust probe retry completed

Followed the updated runbook on 2026-11-07. Created a fresh probe worktree
from `origin/codex/m0-02-priority2` at
`1b006b5053617eb8a7fa961dc4d88b870cc4c76c` and a separate evidence worktree
from `origin/main` at `1c2189470c879ff97d1242a47eceb7881c2c33f4`.

The unchanged `cargo run --release --locked --offline` command passed
`launch-stream`, `stderr-exit`, `fanout`, and `kill-tree`, printed
`RESULT ALL PASS`, and exited 0. No timeout occurred. Complete retry output
and environment are preserved under
[`docs/evidence/m0-02-macos/retry-1/`](evidence/m0-02-macos/retry-1/result.md).
The initial failure remains preserved. Only evidence and documentation were
changed; the implementation coordinator owns probe fixes and board status.
