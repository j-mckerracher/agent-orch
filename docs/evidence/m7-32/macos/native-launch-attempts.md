# M7-32 native launch and cleanup observations

- Source SHA: `92f56c3af9cd34741a09667f499164edcafec696`
- Run ID: `6C93441BC5514685A1B1DB93A0B55F0E`
- Host: macOS 26.7.1 (25G241), arm64; WKWebKit framework 21624.
- Original system appearance was Dark; no `defaults write`, `defaults delete`, System Settings, or desktop appearance change occurred.

## One permitted development WebView attempt

- Working directory: `apps/desktop` in the task worktree.
- Command: `npm run tauri dev`, with task-isolated `CARGO_TARGET_DIR`, `AGENT_ORCH_DATA_DIR=/private/tmp/m7-32-macos-6C93441BC5514685A1B1DB93A0B55F0E/data-dev`, daemon binary from the task target, and Vite port 1420 (confirmed free before launch).
- Result: Tauri compiled and started; Vite listened on port 1420; task daemon log records PID 69790. CUA could not acquire a native window and returned `cgWindowNotFound` for the Agent Orchestrator window. The app was stopped with Ctrl-C. Full Tauri output is `tauri-dev.log`; daemon startup output is `daemon-dev.log`.
- Per the orchestrator direction, no further `tauri dev`, browser-window, or native-window attempts were made.

## Packaged bundle check

- Command: `npm run package:mac` from `apps/desktop`; exit 0. It built an arm64 `.app` (17.69 MiB) and staged the daemon sidecar. Full output: `package-mac.log`.
- The bundle was copied under the isolated run directory. LaunchServices open returned `kLSNoExecutableErr`; CUA also did not expose an inspectable native window (`cgWindowNotFound`). The isolated package daemon log records PID 47021 (`daemon-package.log`). Launcher PID capture is retained in `packaged-app.pid`; the attempted app logs are empty because no UI rendered.
- Task-owned app sessions were interrupted/stopped, and the task-owned daemons (PIDs 69790 and 47021) were terminated. No broad process matching or killing was used. `ps` inspection was denied by the sandbox (`operation not permitted`); no other processes were touched.

The task-created run directory, build targets, desktop `node_modules`, `dist`, and generated ignored sidecar are removed after evidence is committed. This note and the task-owned logs/screenshots remain as evidence.
