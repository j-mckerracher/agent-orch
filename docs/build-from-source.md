# Build from source

Build MCAO yourself to run the desktop app in development or to get the [command-line client](cli.md).

## Requirements

- Rust 1.96.0 and Node.js 22.20.0. These are the versions CI pins.
- Windows only: the Visual Studio C++ build tools and the WebView2 runtime.
- Agent CLIs you want to use, installed and signed in. See [Install](install.md#before-you-install).

## Build the engine

```sh
cargo build --workspace
```

This builds every crate in the root workspace, including the daemon (`target/debug/orchestrator-daemon`) and the CLI.

## Build the desktop frontend

```sh
npm --prefix apps/desktop install
npm --prefix apps/desktop run build
```

## Build the desktop host

The Tauri host is a standalone Cargo package with its own lockfile, so it builds separately from the engine.

```sh
cargo build --manifest-path apps/desktop/src-tauri/Cargo.toml
```

## Run the desktop app in development

```sh
AGENT_ORCH_DAEMON_BIN="$PWD/target/debug/orchestrator-daemon" \
AGENT_ORCH_DATA_DIR="$HOME/agent-orch-dev" \
npm --prefix apps/desktop run tauri dev
```

A debug build launches the daemon named by `AGENT_ORCH_DAEMON_BIN`. `AGENT_ORCH_DATA_DIR` keeps development runs away from your real data. Release builds ignore `AGENT_ORCH_DAEMON_BIN`.

### On Windows

Use `npm.cmd` in place of `npm`. Set the variables in PowerShell:

```powershell
$env:AGENT_ORCH_DAEMON_BIN = "C:\path\to\orchestrator-daemon.exe"
$env:AGENT_ORCH_DATA_DIR = "C:\path\to\agent-orch-dev"
```

## Use the CLI against the development data

```sh
cargo run -p orchestrator-cli -- --data-dir "$HOME/agent-orch-dev" tasks
```

## Package the installers

```sh
npm --prefix apps/desktop run package:mac   # macOS .app
npm --prefix apps/desktop run package:win   # Windows NSIS installer
```

The script builds the release daemon first. Release publishing is described in [Release process](RELEASE.md).
