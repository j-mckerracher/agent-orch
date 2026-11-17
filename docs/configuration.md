# Configuration

MCAO is configured with environment variables, a data directory, and the in-app [Settings](settings.md). Nothing needs configuring for a default install.

## Data directory

MCAO keeps its state in one folder. The default is `~/.local/share/agent-orch` on macOS and `%LOCALAPPDATA%\agent-orch` on Windows.

| Path | Contents |
| --- | --- |
| `store.sqlite` | Tasks and conversations |
| `worktrees/` | One Git worktree per task |
| `daemon.log` | Daemon output, rotated to `daemon.log.1` at 4 MiB |
| `daemon.token` | Local authentication token. Never share it. |
| `ipc/daemon.sock` | The daemon's local socket (a named pipe on Windows) |
| `config/branch-prefix` | Optional machine-wide task branch prefix |

### Change the data directory

Set `AGENT_ORCH_DATA_DIR` for the desktop app. Pass `--data-dir` to the [command-line client](cli.md) and to `orchestrator-daemon`.

## How variables reach the daemon

The desktop app starts the daemon, and the daemon inherits the app's environment. Set a variable in the environment that starts the app. The daemon exits after about a minute of inactivity, so quit the app and wait before relaunching to apply a change.

## Agent programs

MCAO finds `codex`, `claude`, and `copilot` on `PATH`. Set a variable to use another binary.

| Variable | Overrides |
| --- | --- |
| `AGENT_ORCH_CODEX_PROGRAM` | The Codex CLI |
| `AGENT_ORCH_CLAUDE_PROGRAM` | The Claude Code CLI |
| `AGENT_ORCH_COPILOT_PROGRAM` | The GitHub Copilot CLI |
| `AGENT_ORCH_PI_PROGRAM` | The Pi agent CLI |
| `AGENT_ORCH_GH_PROGRAM` | The `gh` CLI used for pull requests |

## Variables passed to agents

An agent process gets a short allowlist of your environment, not all of it. The allowlist covers `PATH`, `HOME`, locale, temp, proxy, and CA settings, plus the provider's own key variables.

### Forward more variables

List the names, comma separated:

```sh
AGENT_ORCH_ENV_PASSTHROUGH=MY_TOOLCHAIN_VAR,ANOTHER_VAR
```

Names that start with `AGENT_ORCH_` are never forwarded. Only names are logged, never values.

## Chat behavior

| Variable | Effect |
| --- | --- |
| `AGENT_ORCH_CONVERSATION_AGENT` | Restrict chat to `codex`, `claude-code`, or `github-copilot` |
| `AGENT_ORCH_CONVERSATION_MODEL` | Default model for chat |
| `AGENT_ORCH_CONVERSATION_EFFORT` | Default reasoning effort for chat |
| `AGENT_ORCH_CONVERSATION_PINS` | Extra trusted CLI versions, such as `codex@0.200.0,claude-code@2.2.0` |
| `AGENT_ORCH_CONVERSATION_TIMEOUT_MS` | Chat turn timeout in milliseconds. Default 180000, range 100 to 600000. |
| `AGENT_ORCH_COPILOT_CREDIT_CAP` | Set to `1` to pass `--max-ai-credits 30` to Copilot |

A CLI version that is not pinned still works, but the chat shows an unverified-version warning.

## Diagnostics

Set `AGENT_ORCH_TRACE_REQUESTS` to any value to log one line per daemon request to `daemon.log`. It records the method, request ID, duration, and outcome, never parameters or results.

## Development only

`AGENT_ORCH_DAEMON_BIN` names the daemon for a debug desktop build. `AGENT_ORCH_CODEBURN_BIN` names the usage reader in debug builds. Release builds ignore both. See [Build from source](build-from-source.md).
