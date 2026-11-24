# Troubleshooting

Find your symptom below. The daemon log is `daemon.log` in the [data directory](configuration.md#data-directory), and it has secrets redacted.

## The app does not connect

The connection strip shows the state: Offline, Connecting..., Disconnected, Daemon unavailable, Authentication failed, or Incompatible version or capability. Click Refresh / reconnect first. If that fails, read the end of `daemon.log`.

### Daemon unavailable

The app could not start the daemon. A release build runs the `orchestrator-daemon` binary bundled beside the app. For a development build, set `AGENT_ORCH_DAEMON_BIN` ([Build from source](build-from-source.md#run-the-desktop-app-in-development)).

### Authentication failed or incompatible version

The app and the running daemon disagree on the token or the version. Try quitting the app, waiting about a minute for an idle daemon to exit, and reopening it.

## A chat gets no reply

The message shows a failure reason. These are the common ones:

| Message | Fix |
| --- | --- |
| Orchestrator runtime unavailable | No agent CLI could be launched. Install one, or set `AGENT_ORCH_*_PROGRAM` ([Configuration](configuration.md#agent-programs)). |
| The chosen agent is not signed in | Sign in to the CLI. For Copilot, run `copilot login`. |
| The orchestrator took too long to answer | Raise `AGENT_ORCH_CONVERSATION_TIMEOUT_MS`. |
| The orchestrator runtime is not pinned to a version | Add the version to `AGENT_ORCH_CONVERSATION_PINS`. |
| This permission level is not available for chat | Choose another level. Extended is not available for chat. |

## An agent is missing from the picker

The picker lists agents the daemon can launch. Check that the CLI (`codex`, `claude`, or `copilot`) is on `PATH` for the process that started the app. Or point MCAO at it with `AGENT_ORCH_CODEX_PROGRAM`, `AGENT_ORCH_CLAUDE_PROGRAM`, or `AGENT_ORCH_COPILOT_PROGRAM`.

On Windows, background catalog probes run without opening a console window.

## A task stays queued

The scheduler runs at most two tasks at once, and two per project. New tasks also wait while the machine is over a [load limit](settings.md#machine-load-limits). Raise a limit to 100 to turn that check off.

## Settings or approvals are locked

The running daemon was not started by this app, so the app cannot approve for it. Close the app and reopen it about a minute after running work finishes.

## A CLI command says the data lock is busy

Another process, such as the daemon, holds the data directory. The CLI waits about 2 seconds. Quit the app or use a different `--data-dir`.

## The CLI refuses to write under the checkout

The data directory must be outside the repository you run against. Pass a `--data-dir` elsewhere.

## macOS or Windows blocks the first launch

The installers are unsigned. Follow [Install](install.md#first-launch-of-an-unsigned-build).
