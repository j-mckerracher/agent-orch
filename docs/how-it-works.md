# How it works

You only need this page to debug a problem or to understand what MCAO touches on your machine.

## The pieces

| Piece | Role |
| --- | --- |
| Desktop app | A Tauri window with a React frontend. It starts the daemon on demand and never stops it. |
| Daemon (`orchestrator-daemon`) | Runs agents, schedules tasks, and stores state in SQLite. |
| Agent CLIs | `codex`, `claude`, and `copilot`, started as child processes. |
| CLI client (`orchestrator-cli`) | Runs one Codex task without the daemon. See [Command-line client](cli.md). |

## How the app talks to the daemon

The app uses JSON-RPC 2.0 over a local socket (`ipc/daemon.sock`) or a named pipe on Windows. Each connection starts with a handshake that checks the token in `daemon.token`. Closing the window does not stop agent work.

## What a task touches

Each task gets its own worktree and its own branch. The worktrees come from a pool that the bundled [treehouse](third-party/treehouse.md) tool keeps under `worktrees/`. A pool slot goes back to the pool when its task is landed or archived, and the next task reuses it at the commit it needs. Your primary checkout is read-only to MCAO. A candidate is only delivered after the steps its delivery mode requires. A folder that is not a Git repository has no worktree and stays report-only.

## Provider access

MCAO makes no direct calls to a model provider and has no credential store. The agent CLIs talk to their own providers with your own sign-in.

## What agents can access

Agents run as your OS account, so a permission level hardens what they do but is not a sandbox. See [Permission levels](permission-levels.md). The threat model is in the [security document](security.md).

## Usage numbers

The AI usage display shows estimates from local Claude Code, Codex, and Copilot data, read by a pinned `codeburn` reader. See [Usage ledger](usage-ledger.md) and [codeburn](third-party/codeburn.md).

## Source layout

The engine is a Cargo workspace under `crates/`. The desktop app is in `apps/desktop`. The architecture and crate roles are in the [technical implementation plan](technical-implementation-plan.md).
