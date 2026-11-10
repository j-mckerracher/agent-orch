# M5-05 native desktop lifecycle runbook

Scope: build, install, launch and lifecycle checks for the Tauri desktop app and
its daemon. Anything not marked **MEASURED** was not run by the author. Windows
steps are **UNPROVEN**: the author had only a Mac, and neither the daemon nor
`src-tauri` can be compiled for Windows from it.

## 1. What the app is

- `agent-orch-desktop` (Tauri host + webview) and `orchestrator-daemon` (sidecar)
  are separate processes. The host relays authenticated frames over the daemon's
  local transport (Unix socket on macOS, named pipe on Windows). The webview never
  sees the token, and the Tauri capability grants only `daemon_open`,
  `daemon_send` and `daemon_close` (no shell, fs or `core:default`).
- The host starts the daemon only when no daemon answers, and never stops it.
  The host refuses `shutdown` and `handshake`. Closing a window or quitting the
  app ends only that client's connection.
- The daemon exits by itself after 60 s of true idleness: no request, no open
  event subscription, no pending executor pass and no active task. A live UI
  event subscription counts as activity, so a connected window keeps its daemon
  alive; the 60 s clock restarts when the last subscriber disconnects. After that
  the daemon idles out and the host starts a fresh one on the next open or
  reconnect. Running agents also keep the daemon alive. **MEASURED (Mac, tests):**
  `orchestrator-ipc` `idle_subscription_tests` (live subscription outlives the
  idle timeout; the server idles out after the last subscriber leaves; a
  non-subscribed connection does not hold it). **NOT re-measured through the GUI**
  after this fix; the earlier GUI observation (idle exit with a connected UI)
  describes the pre-fix behavior.
- Release gate: the shipped app always uses the live daemon. Fixture data is
  reachable only in a plain browser with `?fixture=1` (global banner "FIXTURE DEMO
  MODE") or on a review route `#/project/<id>/review/<taskId>?fixture=1` in that
  same browser (panel labelled "FIXTURE DAEMON - PLACEHOLDER DATA"). The native app
  (any mode with a bound daemon bridge) ignores `fixture=1` everywhere. A failed live request is never replaced by fixture data. D3
  recovery status is a labelled placeholder that reports "Unavailable": for real
  tasks it shows no per-task process, workspace or delivery facts, only that the
  daemon exposes no recovery status yet ("PLACEHOLDER DATA (D3)").

## 2. Build

Prerequisites: Rust (stable, via rustup), Node 22+, `npm ci` in `apps/desktop`.

macOS (**MEASURED**: `Agent Orchestrator.app`, 15.6 MiB, ad-hoc signed, arm64):

```sh
cd apps/desktop
npm ci
CARGO_TARGET_DIR=<dir with >= 2 GiB> npm run package:mac
# app: <target>/release/bundle/macos/Agent Orchestrator.app
```

Windows (**UNPROVEN**; needs MSVC build tools, the WebView2 runtime and NSIS
via the Tauri CLI):

```powershell
cd apps\desktop
npm ci
npm run package:win
# installer: <target>\release\bundle\nsis\Agent Orchestrator_0.1.0_x64-setup.exe
```

`prepare:sidecar` builds `orchestrator-daemon` in release mode, then copies it to
`src-tauri/binaries/orchestrator-daemon-<host-triple>[.exe]` (git-ignored).
`src-tauri/tauri.bundle.conf.json` adds that file as `externalBin`; the bundler
places it beside the main executable. It is kept out of the base
`tauri.conf.json` so `cargo check` works without a staged binary.

Not packaged yet: code signing and notarization, auto-update, a `.dmg`
(`--bundles dmg` is untested), arm64/x64 universal builds.

## 3. Install and launch

macOS: copy the `.app` to Applications (or run it in place) and open it. The
ad-hoc signature may trigger a Gatekeeper prompt on another machine:
right-click, Open.

Windows (UNPROVEN): run the NSIS installer (per-user, no admin), then start
"Agent Orchestrator" from the Start menu.

Environment overrides (set before launch):

| Variable | Effect |
| --- | --- |
| `AGENT_ORCH_DATA_DIR` | Daemon data directory. Default `~/.local/share/agent-orch` (macOS), `%LOCALAPPDATA%\agent-orch` (Windows). |
| `AGENT_ORCH_DAEMON_BIN` | Debug builds only. A release app always uses the sidecar next to its executable. |

Data directory contents: `daemon.log` (daemon stdout/stderr; rotated once at host launch to `daemon.log.1` when over 4 MiB), `store.sqlite`,
`daemon.token` (owner-only), `ipc/`. **Keep the data-dir path short on macOS**:
the Unix socket path (`<dir>/ipc/daemon.sock`) must stay under about 100 bytes.
**MEASURED:** with a 190-character data dir the daemon exits at once with
"daemon IPC failed: local IPC operation failed" in `daemon.log`, and the app shows
"Daemon unavailable".

Launch proof (macOS, **MEASURED**): start the app with `AGENT_ORCH_DATA_DIR`
set to a short path. Within seconds `orchestrator-daemon --data-dir <dir>`
appears as a child process, `daemon.sock` exists, the app holds a connection to
it (`lsof -U`), and the window shows "Connected", "Recovery: unknown" with a
PLACEHOLDER (D3) badge, and the Projects screen with "No repositories
registered." from the real daemon.

## 4. Error presentation (top strip)

| Situation | Strip label | Meaning and next step |
| --- | --- | --- |
| Starting | Connecting... | Wait. |
| Client connection lost, retrying | Reconnecting... | Work is not known to have stopped. |
| Retries exhausted, daemon answered nothing useful | Disconnected (daemon may still be running) | Use Refresh / reconnect. Never read this as "agents stopped". |
| No daemon and the host could not start one | Daemon unavailable | Read `daemon.log` in the data directory. Nothing is known about running work. |
| Wrong or stale token | Authentication failed | Restart the app so it reads the current token. |
| Protocol version or capability mismatch | Incompatible version or capability | Update app and daemon together. |
| Connected | Connected + note "A connection does not prove any agent is running" | Per-task process, workspace and delivery state come from each task. |
| Always | Recovery: unknown (D3 placeholder) | The daemon exposes no recovery status yet. No process is assumed safe from a PID or a successful connection. Task detail shows the same state as "Unavailable" for real tasks, never canned process/workspace/delivery facts. |

Windows connect timing (**derived from code, UNMEASURED**): the host relay now
fails fast when a stale `daemon.token` exists but the pipe is gone (about 0.3 s
per attempt instead of about 5 s, so roughly 0.6 s before the host relaunches the
daemon; `launch_and_open` then retries every 200 ms for 15 s). Relay handshake and
write timeouts are 10 s (were 2 s). A plain `Client::connect` (CLI, tests) still
waits up to 5 s for a pipe that has not appeared yet, because the daemon writes
the token before it creates the pipe.

## 5. Lifecycle verification

### 5.1 Automated, headless (Windows or Mac)

```sh
cargo test --locked -p orchestrator-daemon --test lifecycle -- --nocapture
```

The test has no libtest harness (it re-executes itself as the fake agent), so
cargo reports `0 tests` for it; the result is the printed `STEP` lines and
`lifecycle: PASS` (non-zero exit code on failure). It proves, with a real daemon
and a real agent process, that:

1. the agent process itself wrote its own pid and the OS reports that pid alive;
2. a separate fresh connection reads the task as `Running`;
3. after the UI-side relay (the same transport code the Tauri host uses) closes,
   the daemon and the agent are still alive and the task is still `Running`;
4. a new relay authenticates again, reports the same journal epoch, and resumes the
   event stream at the saved cursor with no gap;
5. a cancel from another client appears on the resumed stream, the task becomes
   `Cancelled`, and the agent process exits;
6. a daemon shutdown reaches the open relay as a remote close.

It does NOT prove anything about the Tauri window, webview, or capability
configuration. A passing fixture test, an open socket, or "Connected" in the
UI is not execution proof; only steps 1-3 are.

**MEASURED (Mac):** passes (3 consecutive runs). **UNPROVEN on Windows:** the
test avoids Unix-only APIs and `canonicalize`, but has never been compiled or run
there. A Windows failure is a finding, not a flake: capture the full output.

### 5.2 Native window lifecycle (manual)

Use a short `AGENT_ORCH_DATA_DIR`.

1. Launch the app. Expect "Connected" within a few seconds.
2. Register a repository in the Projects screen and start a long-running task
   from the UI (the agent CLI named in the task, such as `codex`, must be installed).
   Note the agent pid.
3. Type text into a draft field (for example a task objective) without
   submitting.
4. Close the window.
   - macOS: the app and daemon keep running (the app stays in the Dock). Check
     the daemon and agent pids are unchanged.
   - Windows: closing the window exits the app. The daemon is detached and must
     still be running (Task Manager or `Get-Process orchestrator-daemon`).
5. Reopen (macOS: click the Dock icon; Windows: start the app again).
   Expect "Connected" again, the task list reloaded from the daemon, and the same
   task `Running` with the same agent pid.
6. Kill the daemon pid only (`kill <pid>` / `Stop-Process -Id <pid>`). Expect the
   strip to leave "Connected"; the host starts a new daemon on the next
   reconnect. The previous agent is NOT assumed alive or safe: check the task
   state and recovery handling, not the connection.
7. Make `orchestrator-daemon` fail to start (rename the sidecar or use an
   over-long data dir). Expect "Daemon unavailable" and no sample data.

Record result per step: PASS, FAIL or UNPROVEN, with the pids observed.

**MEASURED (Mac, this build, ad-hoc signed `.app`, empty store):**

- step 1: the app started the daemon as its child; the window showed
  "Connected" and the Projects screen from the daemon (see section 3);
- step 4: closing the window left the app process and its daemon running and the
  app's connection to the daemon gone;
- step 5: reopening via LaunchServices re-established an authenticated connection
  to the same daemon process (same pid; matching `lsof -U` peer sockets) and the
  window rendered "Connected" again;
- the pre-fix daemon idle-exit with a connected UI followed by a host relaunch was
  observed once, as a side effect (this no longer happens with a live
  subscription; see section 1).

Additional native steps to run (**UNPROVEN**, never driven through the GUI):

8. macOS only: press Cmd+Q (or the app menu Quit). The app must exit and the
   daemon and agent pids must be unchanged (the `prevent_exit` handling is only
   read from code).
9. Leave a window connected for more than 60 s with no task. The daemon pid must
   not change (live subscription keeps it alive). Then quit the app and wait
   about 60 s: the daemon may exit; a later launch starts a new one.

**UNPROVEN:** steps 2, 3, 6, 7 in the native app (no task, no draft typed, no
daemon kill, no sidecar failure was driven through the GUI); every step on
Windows; the first-launch window painted blank in one screenshot, then rendered
normally after reopen. This was not investigated and may be a capture artifact.

### 5.3 Where each guarantee is tested

| Guarantee | Test | Layer |
| --- | --- | --- |
| Frames relay, reauth, resume | `orchestrator-ipc` relay tests (5) | Rust, in-process server |
| Host bridge errors, limits, no shutdown/handshake | `src-tauri` bridge tests (6) | Rust, real ipc server |
| Close/reopen reconnect, drafts survive, states distinct, fixture mode gated (incl. native `?fixture=1` review route, D3 unavailable) | `native.test.tsx` (18) | vitest, fake host |
| S6 (D5/D7/D8) against the real daemon methods | `e2e/daemon.e2e.test.ts` | Node, real daemon (Unix socket) |
| Daemon-owned task survives client close | `tests/lifecycle.rs` | Rust, real daemon + agent process |
| Live subscription keeps an idle daemon alive; idle exit after the last subscriber leaves | `orchestrator-ipc` `idle_subscription_tests` (3, seconds-scale timeouts) | Rust, in-process server |

The e2e suite needs the daemon binary: build it first
(`cargo build --locked -p orchestrator-daemon`) and run from `apps/desktop`:

```sh
ORCH_DAEMON_BIN=<target>/debug/orchestrator-daemon npm run test:e2e
```

Without `ORCH_DAEMON_BIN` the suite fails at start ("set ORCH_DAEMON_BIN to the
orchestrator-daemon binary"). It is Unix-socket based and has not been run on
Windows.
