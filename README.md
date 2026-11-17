# MCAO: Mayo Clinic Agent Orchestrator

MCAO is a desktop app and a local daemon that run coding agents on your machine. You chat with Codex, Claude Code, or GitHub Copilot, queue tasks that run in isolated Git worktrees, and review the results.

MCAO makes no direct calls to a model provider. It starts the agent CLIs you already installed and signed in to, and it keeps no credentials of its own.

## Unsigned alpha builds

The macOS and Windows installers are not code-signed, so the OS warns on first launch. See [Install](docs/install.md#first-launch-of-an-unsigned-build) for the steps.

## Quick start

1. Install and sign in to at least one agent CLI: `codex`, `claude`, or `copilot`.
2. Download the installer from the [releases page](https://github.com/mc-innersource/motif-mcao/releases) and install it ([details](docs/install.md)).
3. Open MCAO and talk to an agent in the Orchestrator Workspace ([first run](docs/getting-started.md)).

Supported platforms: macOS (Apple Silicon) and Windows (x64).

## What you can do

| I want to... | Read |
| --- | --- |
| Install MCAO and get past the unsigned-build prompts | [Install](docs/install.md) |
| Register a repository and run my first task | [Getting started](docs/getting-started.md) |
| Chat with an agent and choose its permission level | [Chat](docs/chat.md), [Permission levels](docs/permission-levels.md) |
| Run background tasks and review their work | [Tasks](docs/tasks.md), [Project defaults](docs/project-defaults.md) |
| Run a task from the terminal | [Command-line client](docs/cli.md) |
| Point MCAO at a different agent binary or data folder | [Configuration](docs/configuration.md) |
| Fix a problem | [Troubleshooting](docs/troubleshooting.md) |
| Build from source or contribute | [Build from source](docs/build-from-source.md), [Contributing](docs/contributing.md) |

The full list of pages is in the [documentation index](docs/README.md).

## Status

- Releases are manual, unsigned, and marked as prereleases.
- Windows behavior of Copilot and permission levels is not verified.
- The Workstreams and Review views show placeholder data in places.

## License

MIT, as declared in [`Cargo.toml`](Cargo.toml).
