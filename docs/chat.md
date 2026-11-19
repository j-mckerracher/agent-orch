# Chat

The Orchestrator Workspace is where you talk to an agent. Each message can use a different model and permission level.

## Choose who you talk to

The target picker lists Global, project coordinators, and workstreams. Click anywhere on a target's row to load its conversation and restore its unsent draft. The header shows "Talking to:" and the target's name. A project coordinator is scoped to that repository and appears only after you add it; registering a repository alone does not create one.

Project coordinators answer through the same agent runtime as Global and workstreams, using your chosen model and permission level. Earlier coordinator notes remain visible above new exchanges. If the daemon predates project conversations, update it before sending; the app keeps your draft instead of saving an unanswered note.

## Work with a project coordinator

A project coordinator is an orchestrator for one project. It proposes actions like Global does, but only inside its project: it proposes that project's tasks, retries or messages them, asks you questions about them, and reports to Global. It cannot create workstreams, remove the project, or hand work to another coordinator. Every action still waits for your approval.

Give each coordinator a scope sentence, one line that says what the project covers. Set it in the coordinator panel above the message box. The coordinator is told its scope, and Global uses it to decide which work to hand over.

When you ask Global for work that a coordinator's scope covers, Global proposes to route it there. After you approve, the coordinator receives the request with an id, proposes the tasks it needs, and owns them. Global stops watching that work. It learns about it only from the coordinator's reports: the app reports when the coordinator's tasks are ready, finish, fail or wait, when a question is raised or answered, and when work merges. The coordinator adds its own answer, a caveat, or a blocker. A routed request stays open until the coordinator answers it.

A coordinator does nothing on its own. With no routed request and no message from you, it stays idle and gets no periodic check. A message you send in its conversation is answered there.

The coordinator panel shows the scope, how many of its tasks are not finished, its open requests and its newest reports. Remove coordinator is refused while the coordinator owns unfinished tasks or an unanswered request.

## Choose an agent and model

Codex, Claude Code, and GitHub Copilot can answer chat. The model picker lists the models and efforts the daemon's catalog reports. Your choice is sent with each message and remembered in the app until you change it. If you name no agent, the daemon uses the default agent set in Settings, with the model and effort saved for it. With no default set, it uses Codex, then Claude Code.

## Choose a permission level

The permission level control is in the model picker's panel. Auto is the default. See [Permission levels](permission-levels.md) before choosing Full.

## Use GitHub Copilot

The picker offers Copilot when the daemon can launch the `copilot` CLI, and shows why when it cannot. A Copilot message uses premium requests or AI credits on your Copilot seat. The picker has a sign-in check that makes one billable Copilot call, so use it only when needed.

Copilot does not expose a free model or effort inventory to the picker. Choose Copilot as the provider to use the model configured in Copilot.

After an update to Copilot, Claude Code, or Codex, an unfamiliar version can still run. The daemon checks the installed CLI's help for the command options and permission controls the operation needs. When it cannot confirm those options, it does not start the agent. Retry the check or change your selection. MCAO does not silently replace a saved provider or weaken the requested permissions.

## How the orchestrator writes to you

Every reply, including automatic updates, follows the same rules. It states outcomes, what they mean for you and the decision you have next, without internal terms or task ids. Its last paragraph restates every outcome and every decision still waiting on you, so you can read only that. A failure is reported with its evidence. Worker output and reports are summarized, never passed on word for word. An automatic update about running work posts nothing when there is nothing to tell you.

When a reply acted on or reported tasks that have a pull request MCAO recorded, the links are listed under the reply as `Pull request 12: https://github.com/<owner>/<repo>/pull/12`. The app builds each link from its own records, not from the reply text. The app does not open links, so use the copy button next to each one.

## Clear history or remove a coordinator

The conversation header menu has Clear history and, for a project coordinator, Remove coordinator. Both ask for confirmation first.

## Limit how long a reply may take

A chat turn times out after 180 seconds by default. Change it with `AGENT_ORCH_CONVERSATION_TIMEOUT_MS`, in milliseconds. See [Configuration](configuration.md#chat-behavior).
