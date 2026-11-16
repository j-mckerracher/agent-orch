# MCAO: Mayo Clinic Agent Orchestrator

MCAO is a desktop app and a local daemon that run coding agents on your machine. You chat with agents in a workspace, choose the agent, model, and permission level for each message, and watch what the agents are doing.

The daemon launches the vendor CLIs you have installed (Codex, Claude Code, and GitHub Copilot) as child processes. MCAO makes no direct calls to any provider API and keeps no credential store of its own. It passes a short list of provider environment variables, such as `OPENAI_API_KEY`, from its own environment to the CLI it starts.

## Install and first launch (unsigned build)

> Important: these builds are unsigned.
> The macOS and Windows installers are not code-signed or notarized, so the operating system warns or blocks the first launch. The first release, [v0.1.0](https://github.com/mc-innersource/motif-mcao/releases/tag/v0.1.0), is a prerelease.
>
> - macOS (Apple Silicon, arm64 only): Gatekeeper blocks the first launch. Try to open MCAO once, then open System Settings, Privacy & Security, and click Open Anyway. Or move `MCAO.app` to `/Applications`, then run `xattr -dr com.apple.quarantine /Applications/MCAO.app` in Terminal.
> - Windows: SmartScreen shows "Windows protected your PC". Choose More info, then Run anyway.
> - To check a download, run `shasum -a 256 <file>` on macOS or `Get-FileHash <file>` on Windows, and compare the digest with its line in `SHA256SUMS`.
>
> Downloads are on the [releases page](https://github.com/mc-innersource/motif-mcao/releases). Details are in [docs/RELEASE.md](docs/RELEASE.md).
>
> Nobody has launched the installed app on a clean machine yet, so these first-launch prompts are not yet checked on one.

## Features

- **Workspace chat.** Chat with an agent in the Global workspace or in a project. The composer has a target picker, a model picker, and a permission level control. Each message can use a different model. Codex, Claude Code and GitHub Copilot answer chat. The picker offers Copilot when the daemon can launch its CLI and shows why when it cannot. A Copilot message uses premium requests or AI credits on your Copilot seat.
- **Permission levels.** There are four levels: Restricted, Extended, Auto, and Full. Auto is the default for chats. Extended is not yet available for chat. Full gives the agent the same access as your account, with no MCAO sandbox or approval checks, and has no automatic review step. Only Restricted is kept across a reload. The other levels reset to Auto. Auto relies on each CLI's own automatic review, which is not a guarantee.
- **Agents view.** Lists the configured agent adapters and their sessions. Without a running daemon, it shows fixture data.
- **AI usage.** Shows estimated AI usage across the machine, read from local Claude Code, Codex, and GitHub Copilot data by a pinned `codeburn` reader (see `crates/orchestrator-local-usage/codeburn-pin.json`). The figures are estimates.
- **Conversation management.** The conversation header menu has Clear history and Remove coordinator. Both ask for confirmation first.
- **Ambient background.** An animated background, on by default. A switch turns it off.
- **GitHub Copilot tasks.** Copilot runs tasks through its CLI. The model picker shows a Copilot sign-in check.
- **Command-line client.** The `agent-orch` command runs one Codex task in an isolated Git worktree (`run`). It also lists, inspects, cancels, and cleans up tasks.

## Status

- MCAO is alpha. Releases are manual, unsigned, and marked as prereleases. See [Install and first launch](#install-and-first-launch-unsigned-build).
- Windows behavior of Copilot and permission levels is not verified.
- Workstreams and Review views show placeholder data in places. The move of the agent harness to Oh My Pi is deferred. See `docs/future/mcao-next-version-features.md`.

## Architecture

The Rust engine is a Cargo workspace of 13 crates:

| Crate | Role |
| --- | --- |
| `orchestrator-core` | Domain contracts: task state, transitions, and permission levels. No I/O dependencies. |
| `orchestrator-agents` | Adapters for the vendor CLIs: Codex, Claude Code, GitHub Copilot, and Pi. |
| `orchestrator-git` | Repository validation, task worktree creation, and the task branch prefix. Keeps the primary checkout read-only. |
| `orchestrator-store` | Local persistence of tasks and conversations in SQLite (WAL). |
| `orchestrator-runtime` | Process supervision, cancellation, stdio streaming, and process-tree cleanup for agent subprocesses. |
| `orchestrator-engine` | Execution logic shared by the CLI and the daemon, including the restart reconciliation gate. |
| `orchestrator-ipc` | JSON-RPC protocol, and the authenticated local server and client. |
| `orchestrator-daemon` | The long-lived daemon. Composes the crates behind the local IPC server. |
| `orchestrator-cli` | The `agent-orch` command-line client. Runs the engine in process. |
| `orchestrator-local-usage` | Machine-wide AI usage estimates from the pinned codeburn reader. |
| `orchestrator-host-metrics` | Read-only host telemetry (CPU, memory, disk, GPU) for the daemon. |
| `orchestrator-private-fs` | Owner-only directories and files in the data directory. |
| `orchestrator-diagnostics` | Credential redaction and one-line JSON diagnostics. |

Dependencies point inward. `orchestrator-core` defines the contracts. The agents, git, store, and runtime crates implement them. The IPC and daemon crates add process boundaries. The CLI and the desktop app are clients. Direct dependencies are pinned with `=`.

The desktop app is in `apps/desktop`. Its frontend is React with Vite, inside a Tauri 2 host at `apps/desktop/src-tauri`. The host is a standalone Cargo package with its own lockfile, not a workspace member, so the desktop toolchain and the engine build separately. The host starts the daemon on demand, detached, and never stops it. Closing the window does not stop agent work.

The desktop app talks to the daemon over JSON-RPC 2.0 on `ipc/daemon.sock` in the data directory, or on a named pipe on Windows. Each connection starts with a handshake that checks a token in `daemon.token`.

The data directory defaults to `~/.local/share/agent-orch` on macOS and `%LOCALAPPDATA%\agent-orch` on Windows. Set `AGENT_ORCH_DATA_DIR` for the desktop app, or `--data-dir` for the daemon and the CLI. It holds `store.sqlite`, `daemon.token`, `daemon.log`, and the task worktrees in `worktrees/`.

## Requirements

- Rust 1.96.0 and Node.js 22.20.0. These are the versions CI pins.
- On Windows, the Visual Studio C++ build tools and the WebView2 runtime.
- Supported platforms are macOS and Windows.
- To run real agents, install and sign in to the vendor CLIs you want: `codex`, `claude`, and `copilot`. MCAO finds them on `PATH`. Set `AGENT_ORCH_CODEX_PROGRAM`, `AGENT_ORCH_CLAUDE_PROGRAM`, or `AGENT_ORCH_COPILOT_PROGRAM` to use another path.

## Install

Download the installers from the [releases page](https://github.com/mc-innersource/motif-mcao/releases). Read [Install and first launch](#install-and-first-launch-unsigned-build) before you open them, because the builds are unsigned.

- macOS: unzip `MCAO-0.1.0-macos-arm64-unsigned.zip` and move `MCAO.app` to `/Applications`.
- Windows: run `MCAO-0.1.0-windows-x64-unsigned-setup.exe` and follow the installer.

## Development

Run these from the repository root:

```sh
# Engine workspace: builds every crate, including the daemon
cargo build --workspace

# Desktop frontend dependencies and production build
npm --prefix apps/desktop install
npm --prefix apps/desktop run build

# Tauri host, a standalone package
cargo build --manifest-path apps/desktop/src-tauri/Cargo.toml

# Desktop dev loop: serves the Vite app and opens the Tauri window
AGENT_ORCH_DAEMON_BIN="$PWD/target/debug/orchestrator-daemon" \
AGENT_ORCH_DATA_DIR="$HOME/agent-orch-dev" \
npm --prefix apps/desktop run tauri dev

# CLI against the same development data directory
cargo run -p orchestrator-cli -- --data-dir "$HOME/agent-orch-dev" tasks
```

A debug desktop build launches the daemon named by `AGENT_ORCH_DAEMON_BIN`. Release builds ignore that variable and run the daemon bundled beside the app. `AGENT_ORCH_DATA_DIR` keeps development runs away from your real data.

On Windows, use `npm.cmd` in place of `npm`, and set the two variables in PowerShell with `$env:AGENT_ORCH_DAEMON_BIN = "..."` and `$env:AGENT_ORCH_DATA_DIR = "..."`.

To package the app, run `npm --prefix apps/desktop run package:mac` or `package:win`. The script builds the release daemon first.

## Testing and CI

Run the tests from the repository root:

```sh
cargo test --locked --workspace
cargo test --locked --manifest-path apps/desktop/src-tauri/Cargo.toml
npm --prefix apps/desktop test
npm --prefix apps/desktop run test:e2e
npm --prefix apps/desktop run test:browser
node --test "scripts/**/*.test.mjs"
```

`npm run test:e2e` runs against a real daemon over a Unix socket, so it is a macOS gate in CI. `test:browser` runs Playwright layout checks.

The workflows are in `.github/workflows`:

| Workflow | Runs on | Purpose |
| --- | --- | --- |
| `ci.yml` | pull requests, pushes to `main`, nightly, manual | Native gates on macOS (`macos-15`) and Windows (`windows-2025`). |
| `security.yml` | pull requests | Trivy and TruffleHog through `scripts/security/gate.mjs`, on macOS and Windows. |
| `audit.yml` | manifest and lockfile changes, `main`, daily | `cargo deny`, `cargo audit`, and `npm audit` at high severity. |
| `lint.yml` | pull requests, pushes to `main` | Super-linter: Markdown, YAML, GitHub Actions, and gitleaks. |
| `policy.yml` | pull requests, pushes to `main` | Repository policy checks. |
| `pr-validate.yml` | pull requests | PR title, Jira key, and labels. |
| `catalog-validate.yml` | pull requests, pushes to `main` | Validates the Port service catalog file. |
| `release.yml` | `v*` tags, manual | Builds the installers. A publish job creates the GitHub release after approval. |

A gate passes only when its command exits 0 and its output has the expected shape: test counts and names. The expected values are in `scripts/ci/expected.mjs`, and the matching name lists are in `scripts/ci/*-test-names.json`. When a change adds or removes tests, update those values in the same pull request, taking them from real CI logs. Do not estimate them.

`node scripts/ci/run-gates.mjs --list` prints the gates for your platform. A pull request that changes only `docs/**` or the root Markdown files runs only the Windows harness job.

Some Windows count values are still derived rather than measured. See the comments in `scripts/ci/expected.mjs`.

## Security

Before you open or update a pull request, run the Trivy scan from the repository root. It must exit 0:

```sh
trivy fs --scanners vuln,secret,misconfig --severity HIGH,CRITICAL --exit-code 1 .
```

Fix the root cause of a finding: upgrade the dependency, remove the secret, or fix the misconfiguration. Do not suppress findings with ignore files or by lowering the severity without agreement from the maintainers. Treat any secret the scan finds as compromised, and rotate it, even after you remove it from the code.

CI runs Trivy with the same scanners and severities, plus TruffleHog. The threat model is in `docs/security.md`.

## Contributing

- Name branches `type/DPES-NNNN_slug`, for example `feat/DPES-NNNN_short-slug`.
- Write commit messages and PR titles as Conventional Commits with the Jira key: `type(scope): DPES-NNNN - Description`. The description starts with a capital letter. The allowed types are in the PR template.
- Use the [organization PR template](https://github.com/mc-innersource/.github) for every pull request. Fill in the Jira work-item block, and list every CI run under Workflow Runs.
- Open pull requests as ready for review, not as drafts. Do not merge while a check is red.
- Record user-visible changes under `## Unreleased` in `docs/changelog.md`.
- Maintainers cut releases as described in `docs/RELEASE.md`.

## Links

- [Technical implementation plan](docs/technical-implementation-plan.md)
- [Release process](docs/RELEASE.md) and [release CI, including unsigned releases](docs/release-ci.md)
- [Security and threat model](docs/security.md) and [security gate](docs/security/dpes-1100-security-gate.md)
- [Agent permission levels](docs/design/agent-permission-levels.md)
- [Conversation service](docs/m7-08-conversation-service.md)
- [Usage ledger](docs/usage-ledger.md) and [codeburn reader](docs/third-party/codeburn.md)
- [Desktop wireframe](docs/ui/wireframe.md)
- [Changelog](docs/changelog.md)
- [Deferred features](docs/future/mcao-next-version-features.md)
