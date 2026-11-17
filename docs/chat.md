# Chat

The Orchestrator Workspace is where you talk to an agent. Each message can use a different model and permission level.

## Choose who you talk to

The target picker lists Global, project coordinators, and workstreams. Click anywhere on a target's row to load its conversation and restore its unsent draft. The header shows "Talking to:" and the target's name. A project coordinator is scoped to that repository and appears only after you add it; registering a repository alone does not create one.

Project coordinators answer through the same agent runtime as Global and workstreams, using your chosen model and permission level. Earlier coordinator notes remain visible above new exchanges. Project conversations do not propose actions; use Global or a workstream to request actionable plans. If the daemon predates project conversations, update it before sending; the app keeps your draft instead of saving an unanswered note.

## Choose an agent and model

Codex, Claude Code, and GitHub Copilot can answer chat. The model picker lists the models and efforts the daemon's catalog reports. Your choice is sent with each message and remembered in the app until you change it. If you name no agent, the daemon uses Codex, then Claude Code.

## Choose a permission level

The permission level control is in the model picker's panel. Auto is the default. See [Permission levels](permission-levels.md) before choosing Full.

## Use GitHub Copilot

The picker offers Copilot when the daemon can launch the `copilot` CLI, and shows why when it cannot. A Copilot message uses premium requests or AI credits on your Copilot seat. The picker has a sign-in check that makes one billable Copilot call, so use it only when needed.

## Clear history or remove a coordinator

The conversation header menu has Clear history and, for a project coordinator, Remove coordinator. Both ask for confirmation first.

## Limit how long a reply may take

A chat turn times out after 180 seconds by default. Change it with `AGENT_ORCH_CONVERSATION_TIMEOUT_MS`, in milliseconds. See [Configuration](configuration.md#chat-behavior).
