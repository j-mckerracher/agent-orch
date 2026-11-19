# Chat

The Orchestrator Workspace is where you talk to an agent. Each message can use a different model and permission level.

## Choose who you talk to

The target picker lists Global, project coordinators, and workstreams. Click anywhere on a target's row to load its conversation and restore its unsent draft. The header shows "Talking to:" and the target's name. A project coordinator is scoped to that repository and appears only after you add it; registering a repository alone does not create one.

Project coordinators answer through the same agent runtime as Global and workstreams, using your chosen model and permission level. Earlier coordinator notes remain visible above new exchanges. Project conversations do not propose actions; use Global or a workstream to request actionable plans. If the daemon predates project conversations, update it before sending; the app keeps your draft instead of saving an unanswered note.

## Choose an agent and model

Codex, Claude Code, and GitHub Copilot can answer chat. The model picker lists the models and efforts the daemon's catalog reports. Your choice is sent with each message and remembered in the app until you change it. If you name no agent, the daemon uses the default agent set in Settings, with the model and effort saved for it. With no default set, it uses Codex, then Claude Code.

## Choose a permission level

The permission level control is in the model picker's panel. Auto is the default. See [Permission levels](permission-levels.md) before choosing Full.

## Use GitHub Copilot

The picker offers Copilot when the daemon can launch the `copilot` CLI, and shows why when it cannot. A Copilot message uses premium requests or AI credits on your Copilot seat. The picker has a sign-in check that makes one billable Copilot call, so use it only when needed.

Copilot does not expose a free model or effort inventory to the picker. Choose Copilot as the provider to use the model configured in Copilot.

After an update to Copilot, Claude Code, or Codex, an unfamiliar version can still run. The daemon checks the installed CLI's help for the command options and permission controls the operation needs. When it cannot confirm those options, it does not start the agent. Retry the check or change your selection. MCAO does not silently replace a saved provider or weaken the requested permissions.

## Thinking

When the agent exposes its reasoning, a "Thinking" block appears above that turn's reply. It is open while the agent works and collapses when the reply starts. Click it to open it again. Copy thinking copies the full text.

The block shows everything the provider exposes. Newer Claude and GitHub Copilot models return summarized thinking. Codex returns raw reasoning only on models that support it. Turns from before this feature have no thinking.

Thinking is capped at 1 MiB per chat turn. When the cap is reached, the text ends with "[thinking truncated: limit reached]".

## Clear history or remove a coordinator

The conversation header menu has Clear history and, for a project coordinator, Remove coordinator. Both ask for confirmation first.

## Limit how long a reply may take

A chat turn times out after 180 seconds by default. Change it with `AGENT_ORCH_CONVERSATION_TIMEOUT_MS`, in milliseconds. See [Configuration](configuration.md#chat-behavior).
