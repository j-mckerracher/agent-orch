# MCAO documentation

Start with [Install](install.md) and [Getting started](getting-started.md). The project overview is in the [repository README](../README.md).

## Use MCAO

- [Install](install.md): download an installer and get past the unsigned-build prompts.
- [Getting started](getting-started.md): register a repository, chat, and run a first task.
- [Chat](chat.md): talk to Codex, Claude Code, or Copilot in the workspace.
- [Permission levels](permission-levels.md): what Restricted, Extended, Auto, and Full allow.
- [Tasks](tasks.md): submit work that runs in an isolated worktree, then review it.
- [Project defaults](project-defaults.md): delivery mode, base branch, branch prefix, checks, and merge method.
- [Settings](settings.md): load limits, standing approvals, preferences and learnings, and monitoring.

## Reference

- [Command-line client](cli.md): run and inspect a Codex task from a terminal.
- [Configuration](configuration.md): data directory and every supported environment variable.
- [Troubleshooting](troubleshooting.md): fixes for connection, chat, and task problems.
- [How it works](how-it-works.md): the pieces and what MCAO touches on your machine.

## Build and contribute

- [Build from source](build-from-source.md): requirements, build, and development run.
- [Contributing](contributing.md): tests, security scan, branch and PR rules.
- [Release process](RELEASE.md): versioning and how a release is cut.
- [Release CI](release-ci.md): the CI workflows and gates, including unsigned releases.
- [Changelog](changelog.md): what changed in each release.

## Design and engineering records

These documents describe design and implementation. They are written for maintainers, not as user guides.

- [Technical implementation plan](technical-implementation-plan.md): architecture and milestones.
- [Security and threat model](security.md): trust boundaries and controls.
- [Security gate](security/dpes-1100-security-gate.md): the CI security gate.
- [Dependency review](security/m6-01-threat-model-and-dependency-review.md): dependencies, environment allowlists, and delivery boundaries.
- [Agent permission levels design](design/agent-permission-levels.md): per-agent flags and open decisions.
- [Orchestrator actions](design/orchestrator-actions.md): design for the Global orchestrator and its crew of agents.
- [Orchestrator actions on remote hosts](design/orchestrator-actions-remote-hosts.md): threat model for remote crew hosts.
- [Orchestrator actions map](design/orchestrator-actions-firstmate-map.md): each firstmate capability mapped to MCAO.
- [Conversation service](m7-08-conversation-service.md): how chat turns run in the daemon.
- [Usage ledger](usage-ledger.md): how AI usage is recorded and summarized.
- [codeburn reader](third-party/codeburn.md): the pinned local usage reader.
- [treehouse](third-party/treehouse.md): the pinned worktree pool tool.
- [Host metrics](host-metrics.md): the `host.metrics` daemon method.
- [Queue snapshot](queue-snapshot.md): the `queue.snapshot` daemon method.
- [Assignment profile policy](assignment-profile-policy-v1.md): how work is classified.
- [Audit envelope v1](audit-envelope/mcao.audit-envelope.v1.md): the audit record format.
- [Desktop wireframe](ui/wireframe.md): screens and data boundaries.
- [Deferred features](future/mcao-next-version-features.md): planned work that is not started.
- [Laptop verification plan](laptop-verification-plan.md): a recorded native macOS verification run.

Test evidence, runbooks, and screenshots are under [`evidence/`](evidence/) and [`ui/`](ui/). They are records, not guides, so they are not indexed here.
