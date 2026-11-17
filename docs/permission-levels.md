# Permission levels

A permission level sets how much an agent may do without asking you. In the desktop app you choose it for each chat message, in the model picker's panel.

## The four levels

| Level | Meaning |
| --- | --- |
| Restricted | Read-only chat: agent write, shell, and web access is denied. |
| Extended | Not yet available for chat. |
| Auto | The default. The agent CLI's own automatic review decides risky actions. |
| Full | No MCAO sandbox and no approval checks. |

## What Auto does and does not guarantee

Auto relies on each CLI's own automatic review. That review is not a guarantee. For tasks, Claude Code's Bash and Copilot's shell are not confined to the task worktree.

## What Full does

Full gives the agent the same access as your account: any command, any file you can edit. MCAO adds no sandbox, no approval checks, and no review step. Codex follows your own `~/.codex` settings, and Claude Code and Copilot follow your own settings and hooks.

## What is remembered

Only Restricted is kept across a reload. Auto, Extended, and Full reset to Auto. Choosing Full opens a confirmation first.

## Older daemons

A daemon that predates permission levels runs chats as Restricted and shows a notice. Update the daemon, or choose Restricted, to clear the notice.

## Optional Copilot spend cap

Set `AGENT_ORCH_COPILOT_CREDIT_CAP=1` to pass `--max-ai-credits 30` to every Copilot run. The cap is soft, so one response can exceed it. See [Configuration](configuration.md#chat-behavior).

## Design details

The per-agent flags are in the [design record](design/agent-permission-levels.md). It is a design document, and it marks some items as open.
