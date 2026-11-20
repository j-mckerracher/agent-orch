# Chat

The Orchestrator Workspace is where you talk to an agent. Each message can use a different model and permission level.

## Choose who you talk to

The target picker lists Global, project coordinators, and workstreams. Click anywhere on a target's row to load its conversation and restore its unsent draft. The header shows "Talking to:" and the target's name. A project coordinator is scoped to that repository and appears only after you add it; registering a repository alone does not create one.

Project coordinators answer through the same agent runtime as Global and workstreams, using your chosen model and permission level. Earlier coordinator notes remain visible above new exchanges. If the daemon predates project conversations, update it before sending; the app keeps your draft instead of saving an unanswered note.

## Work with a project coordinator

A project coordinator is an orchestrator for one project. It proposes actions like Global does, but only inside its project: it proposes that project's tasks, retries, messages, revises or cancels them, asks you questions about them, proposes the project's preferences and learnings, and reports to Global. It cannot create workstreams, remove the project, or hand work to another coordinator.

Its actions follow the same rules as Global's. With "Let the orchestrator act on its own" on in Settings, the actions it may take on its own run at once after your message, at most 5 a turn, and the card says "Done on its own". These are starting a task, retrying a task, messaging a waiting task, asking you a question, recording a learning, and reporting to Global. Cancelling or revising a task, merging, holding a decision, saving a preference, and starting a task that ends in a pull request always wait for you to click **Approve**. An automatic turn, such as an update about its running tasks, or a turn that read a worker's report, acts on its own only on existing work: it may retry, message, ask or report. The one exception is a task for an open routed request, described below. With the switch off, every action waits for your approval, unless a standing approval covers it.

Give each coordinator a scope sentence, one line that says what the project covers. Set it in the coordinator panel above the message box. The coordinator is told its scope, and Global uses it to decide which work to hand over.

When you ask Global for work that a coordinator's scope covers, Global routes it there. With "Acting on its own" on in Settings, a route from your message happens at once; otherwise it is a card you approve. The coordinator receives the request with an id, starts the tasks it needs, and owns them. A task for an open request starts on its own (at most 5 per request) while the switch is on, even from an automatic turn; past that, it waits for you. Global stops watching that work. It learns about it only from the coordinator's reports: the app reports when the coordinator's tasks are ready, finish, fail or wait, when a question is raised or answered, and when work merges. The coordinator adds its own answer, a caveat, or a blocker. A routed request stays open until the coordinator answers it.

A coordinator does not look for work. With no routed request and no message from you, it stays idle and gets no periodic check. A message you send in its conversation is answered there.

The coordinator panel shows the scope, how many of its tasks are not finished, its open requests and its newest reports. Remove coordinator is refused while the coordinator owns unfinished tasks or an unanswered request.

## Choose an agent and model

Codex, Claude Code, and GitHub Copilot can answer chat. The model picker lists the models and efforts the daemon's catalog reports. Your choice is sent with each message and remembered in the app until you change it. If you name no agent, the daemon uses the default agent set in Settings, with the model and effort saved for it. With no default set, it uses Codex, then Claude Code.

## Choose a permission level

The permission level control is in the model picker's panel. Auto is the default. See [Permission levels](permission-levels.md) before choosing Full.

## Use GitHub Copilot

The picker offers Copilot when the daemon can launch the `copilot` CLI, and shows why when it cannot. A Copilot message uses premium requests or AI credits on your Copilot seat. The picker has a sign-in check that makes one billable Copilot call, so use it only when needed.

The picker lists the models your Copilot account can use, and the effort levels each model supports. The daemon reads them from the Copilot CLI's `--acp` server, which does not make a billable call. The daemon keeps the list for 30 minutes. A model you choose is sent to Copilot with your message. If the daemon cannot read the list, the model picker stays empty and shows why, and Copilot uses the model configured in Copilot.

After an update to Copilot, Claude Code, or Codex, an unfamiliar version can still run. The daemon checks the installed CLI's help for the command options and permission controls the operation needs. When it cannot confirm those options, it does not start the agent. Retry the check or change your selection. MCAO does not silently replace a saved provider or weaken the requested permissions.

## How the orchestrator writes to you

Every reply, including automatic updates, follows the same rules. It states outcomes, what they mean for you and the decision you have next, without internal terms or task ids. Its last paragraph restates every outcome and every decision still waiting on you, so you can read only that. A failure is reported with its evidence. Worker output and reports are summarized, never passed on word for word. An automatic update about running work posts nothing when there is nothing to tell you.

When a reply acted on or reported tasks that have a pull request MCAO recorded, the links are listed under the reply as `Pull request 12: https://github.com/<owner>/<repo>/pull/12`. The app builds each link from its own records, not from the reply text. The app does not open links, so use the copy button next to each one.

## Live activity

While the agent works on a turn, an activity block appears above the reply. It shows the agent's thinking and a timeline of its tool calls in the order they happened. The status line under it says what the agent is doing, for example "Thinking · 4 s" or "Running Read · 12 s". When the agent has sent nothing for 20 seconds the line reads "No activity for 20 s", so a hung agent looks different from a busy one. Cancel reply stays available the whole time.

Each tool call shows its name and a one-line summary, such as the file path or the command. A spinner means the call is running, a check means it finished, a cross means it failed, and a stop mark means the turn ended before the call did (canceled, timed out, failed or interrupted by a restart). Select a call, or focus it and press Enter or Space, to open a short preview of its result. When you scroll up in a long log the view stops following new lines, and Jump to latest takes you back.

The thinking text is open while the agent works and collapses when the reply starts. Click it to open it again. Copy thinking copies the full text. It shows everything the provider exposes. Newer Claude and GitHub Copilot models return summarized thinking. Codex returns raw reasoning only on models that support it. Turns from before this feature have no activity.

Limits and redaction. Thinking is capped at 1 MiB per chat turn. When the cap is reached, the text ends with "[thinking truncated: limit reached]". Thinking is stored as the provider sent it and is not redacted. Tool input and results can hold secrets, so the daemon redacts credentials in them before it stores anything. A tool summary keeps 160 characters and a result preview keeps 400, and a longer text ends with "...[truncated N chars]". The full file contents and the full command output are never stored. A turn keeps at most 400 tool calls, then one line says later calls are not shown.

An older daemon does not record tool calls. The block then shows thinking only, with the time since you sent the message.

## Message times

Each message shows the time it was recorded, in your device's timezone with its short name, such as 11:41 PM MST. The name follows daylight saving, so it reads MDT in summer. The daemon stores and sends every time as UTC with a Z. An older daemon sent some times with no zone mark, and the app reads those as UTC too, so old and new messages show the same clock. A time the app cannot read shows as "Unrecognized time" instead of a guess.

## Clear history or remove a coordinator

The conversation header menu has Clear history and, for a project coordinator, Remove coordinator. Both ask for confirmation first.

## Limit how long a reply may take

A chat turn times out after 180 seconds by default. Change it with `AGENT_ORCH_CONVERSATION_TIMEOUT_MS`, in milliseconds. See [Configuration](configuration.md#chat-behavior).
