# Manual UI tests: Global Orchestrator

These test cases check documented behaviors of the Global Orchestrator through the desktop app UI. An AI agent with computer use runs them. A human can run them too.

| # | Behavior | File |
| --- | ---------- | ------ |
| 1 | The Global Orchestrator dispatches agents to do work | [01-dispatch.md](01-dispatch.md) |
| 2 | The Global Orchestrator wakes to check on its agents | [02-wake.md](02-wake.md) |
| 3 | The Global Orchestrator starts sub-orchestrators | [03-sub-orchestrators.md](03-sub-orchestrators.md) |
| 4 | The Global Orchestrator is the main intermediary between the user and any sub-orchestrator or worker | [04-intermediary.md](04-intermediary.md) |
| 5 | Questions for the user are held until the user answers | [05-decisions.md](05-decisions.md) |
| 6 | Workers start from a brief, wait for a reply and resume | [06-workers.md](06-workers.md) |
| 7 | The orchestrator acts on its own within bounds; some actions still need the user's word | [07-authority.md](07-authority.md) |
| 8 | Preferences and learnings | [08-preferences.md](08-preferences.md) |
| 9 | Project coordinators take routed work and report back | [09-coordinators.md](09-coordinators.md) |
| 10 | Reply etiquette, pull request links and wake caps | [10-etiquette-and-caps.md](10-etiquette-and-caps.md) |
| 11 | An older database upgrades without losing data | [11-upgrade.md](11-upgrade.md) |
| 12 | Every operation in Settings | [12-settings.md](12-settings.md) |

Start with [00-setup.md](00-setup.md). Every test file starts from that setup. To run the tests, paste the prompt in [RUNNER.md](RUNNER.md) into a computer-use agent; a human tester follows the same rules.

The tests always run on the real app, built from the tip of `main`, with real agents: Claude Code, model "Sonnet (alias)" at effort "medium". There is no fake agent. Real agents cost money: each file states its cost class, and a run stops at its spend limit.

## How the docs define these terms

The docs describe these behaviors differently from the plain-language names above. The tests follow the docs.

- **Dispatch.** The agent ends a reply with one structured action block. The daemon turns each action into a card (`docs/design/orchestrator-actions.md` N1; `docs/m7-08-conversation-service.md` "Task proposals (DPES-1105, phase 2)"). With **Let the orchestrator act on its own** on (the default), a task from a reply to the user's message starts at once and its card reads "Done on its own". With the switch off, the task runs only after the user approves the card (`docs/security.md` section 6; **Settings** > **Autonomy**).
- **Wake.** A wake turn is an automatic turn started by an event, such as a failed task, a worker that waits, or a coordinator report, or by a timer while work is active. It is not a user message (`docs/design/orchestrator-actions.md` "Wake-up loop: the daemon is the watcher"). A wake turn does not start new work on its own initiative. It may retry, message or ask about existing work. A project coordinator may also start tasks for a request routed to it (`09-coordinators.md`).
- **Sub-orchestrator.** A sub-orchestrator is a workstream conversation, made by a `workstream.create` action: on its own when the switch is on, after the user's approval when it is off. It has its own entry in the conversation target picker. It cannot create workstreams itself (TC-S-07). A project coordinator is the other kind: one orchestrator per project, which receives work routed from Global (`09-coordinators.md`).
- **Intermediary.** The Global conversation reaches the user through cards, wake notices, relayed scout reports, the **Needs you** tab of **Review**, coordinator reports, and the "Routed here by the global orchestrator" strip. The user talks to a workstream or a coordinator directly by choosing it in the target picker. Workers never address the user. The docs do not describe the Global Orchestrator forwarding chat text to workers. A test that expects that is out of scope.

## Rules for the runner

[RUNNER.md](RUNNER.md) owns the rules: real app and real agents only, verbatim prompts, fixed settings and waits, retries for model-dependent steps, evidence names, and the layout of `RESULTS.md`. This README does not repeat them.

## Test case format

Each test case has:

- **ID.** `TC-D-nn` (dispatch), `TC-W-nn` (wake), `TC-S-nn` (sub-orchestrator), `TC-I-nn` (intermediary), `TC-Q-nn` (questions), `TC-B-nn` (worker brief and status), `TC-A-nn` (authority), `TC-P-nn` (preferences), `TC-C-nn` (coordinators), `TC-E-nn` (etiquette and caps), `TC-M-nn` (upgrade), `TC-ST-nn` (settings).
- **Source.** The doc and the line the expected behavior comes from.
- **Preconditions.** State that must be true before step 1, and the worker objectives or triggers it needs (`00-setup.md` section 8).
- **Steps.** Numbered. Each step names the control to use and gives every message to send, word for word.
- **Expected.** What must be visible after the steps.
- **Real agent.** The model and effort, and the cost class.

Cost classes: **low** means chat turns only, no task run. **Medium** means one or more task runs; a failed task can run twice, because the daemon retries it once on its own. A test that starts a task run is **medium**, even when it only holds the run.

Tests from `05` on also have a one-line purpose under the heading, a **Reset** line when the test leaves state behind, and a **Known limitations** line when part of the behavior has no UI to check. A step marked **Model-dependent** needs the model to take an action; RUNNER.md rule 8 says how to retry it. Checks marked **Prompt log** read what the daemon sent to an agent, which the app does not show (`00-setup.md` section 7). A test marked `BLOCKED` in its preconditions has no reliable way to cause its situation with a real agent; the reason is stated, and the runner records it as `BLOCKED` without running it.

## Coverage matrix

| Documented behavior | Source | Test cases |
| --------------------- | -------- | ------------ |
| Dispatch defaults and precedence | `docs/settings.md` "Agent dispatch defaults"; **Settings** > **Agents** | TC-D-01, TC-D-02, TC-D-03 |
| Task proposal card, approve, reject | `docs/design/orchestrator-actions.md` N1 | TC-D-04, TC-D-05, TC-D-06 |
| Dependent card order | `apps/desktop/src/ui/views/workspace/actionCardModel.ts` | TC-D-07 |
| Home directory when project is unclear | `docs/settings.md` "Home directory" | TC-D-08, TC-D-09 |
| Load gate | `docs/design/orchestrator-actions.md` "Machine load gate: respect free resources" | TC-D-11 |
| Actions that always need approval | `docs/settings.md` "Actions that always need approval" | TC-D-12 |
| Standing approvals | `docs/settings.md` "Standing approvals" | TC-D-13 |
| Proposal states | `docs/design/orchestrator-actions.md` "Standing approvals" | TC-D-14 |
| A project coordinator proposes actions in its project | `docs/chat.md` "Work with a project coordinator" | TC-D-15 |
| Default agent order and unavailable agent | `docs/chat.md`; `docs/m7-08-conversation-service.md` | TC-D-02, TC-D-16 |
| Monitoring chip and modes | `docs/design/orchestrator-actions.md` "Wake-up loop: the daemon is the watcher" | TC-W-01, TC-W-02, TC-W-03 |
| Wake on failed task | same | TC-W-04 |
| Notice-only wakes (stalled, delivery waiting) | same | TC-W-05 |
| Quiet reply only folds a work check | `docs/design/orchestrator-actions.md` "Wake-up loop: the daemon is the watcher" (bullet **Output**) | TC-W-06 |
| Quiet reply does not hide a failure | same | TC-W-17 |
| Empty change set fails with a reason | same | TC-W-15 |
| Candidate ready update | `docs/design/orchestrator-actions.md` "Wake-up loop: the daemon is the watcher" | TC-W-16 |
| Report relay retried, then a deterministic notice | `docs/design/orchestrator-actions.md` "Wake-up loop: the daemon is the watcher" (bullet **Output**) | TC-W-18 |
| Open task link on a task notice | `docs/design/orchestrator-actions.md` "Wake-up loop: the daemon is the watcher" (bullet **What the user sees**) | TC-W-15, TC-W-16, TC-W-17 |
| User turn cancels wake | same | TC-W-07 |
| Limits and "Monitoring paused: limit reached" | same; `docs/settings.md` "Automatic replies" | TC-W-08 |
| Three failures pause automatic updates | same | TC-W-09, TC-W-10 |
| Away mode and return brief | `docs/m7-08-conversation-service.md` | TC-W-11 |
| Check interval settings | `docs/settings.md` "Checks on running work" | TC-W-12 |
| Mode Off sends notices only | `docs/design/orchestrator-actions.md` "Wake-up loop: the daemon is the watcher" | TC-W-14 |
| Wake badge and label | `apps/desktop/src/ui/views/workspace/monitoringModel.ts` | TC-W-04, TC-W-13 |
| `workstream.create` proposal | `docs/design/orchestrator-actions.md` "Secondmates and the "sub-orchestrator" request" | TC-S-01, TC-S-02, TC-S-03 |
| Routing a task to a workstream | `docs/design/orchestrator-actions.md` "Secondmates and the "sub-orchestrator" request" | TC-S-05, TC-S-06 |
| Workstream scope limits | `docs/m7-08-conversation-service.md` | TC-S-07, TC-S-08 |
| Workstream counts as work unit | `docs/design/orchestrator-actions.md` "Supervision checks: stuck work and scope creep" | TC-S-09 |
| Archived workstream refuses an update | `docs/m7-08-conversation-service.md` "Action approval and execution (DPES-1105, PR 1c)" | TC-S-11 |
| Target picker lists all targets, with a search box | `docs/chat.md` "Choose who you talk to" | TC-I-01 |
| User talks to a workstream or coordinator | `docs/chat.md` | TC-I-02, TC-I-03 |
| Scout report relay, automatic | `docs/m7-08-conversation-service.md` | TC-I-04, TC-I-05 |
| Approvals surface to the user | `docs/design/orchestrator-actions.md` "Wake-up loop: the daemon is the watcher" | TC-I-08 |
| Task events reported up to Global | same | TC-I-09 |
| End-to-end flow | all | TC-I-10 |
| Wake turns run at Restricted, whatever the user chose | `docs/design/orchestrator-actions.md` "Wake-up loop: the daemon is the watcher" | TC-I-11 |
| Questions under **Needs you** in **Review**, one per key | `docs/design/firstmate-behavior-decisions.md` Phase 3 | TC-Q-01, TC-Q-02 |
| Verbatim answer, deferral, cancelled task keeps its question | same | TC-Q-03, TC-Q-04, TC-Q-06 |
| Open question blocks a queued task | `firstmate-behavior-plan.md` 5.4 | TC-Q-05 |
| Questions are user-only | Phase 3 items 10, 17 | TC-Q-07 |
| Acting on its own: switch, Done on its own, off fallback | `docs/security.md` section 6 | TC-A-01, TC-A-02, TC-A-03 |
| User-word actions and pull-request tasks still wait | same | TC-A-04, TC-A-05 |
| Automatic turns act only on existing work; 5 per reply | same | TC-A-06, TC-A-07 |
| One-task override | Phase 5 items 14, 15 | TC-A-08 |
| Unlanded work and Discard changes | Phase 5 items 16 to 18 | TC-A-09 |
| Preferences: keyed entries, archive, refusals, budgets | `docs/settings.md` "Preferences and learnings" | TC-P-01, TC-P-02, TC-P-03 |
| Learning on its own, preference proposed; shared read-only outside Global | same; Integration I2, I8 | TC-P-04, TC-P-05 |
| Wake turns read preferences; staleness; history clear keeps them | Phase 6 items 5, 7, 14 | TC-P-06, TC-P-07, TC-P-08 |
| Worker brief per task kind; workers never address the user | `firstmate-behavior-plan.md` 5.2 | TC-B-01, TC-B-02 |
| Waiting for a decision or help; no spend, no slot; no stall notice | `docs/tasks.md` "Task states"; Phase 4 | TC-B-03, TC-B-06, TC-B-07 |
| Answer and `task.message` resume the same session | Phase 4 items 13 to 20 | TC-B-04, TC-B-05, TC-B-08 |
| Scope sentence and routing from Global | `docs/chat.md` "Work with a project coordinator" | TC-C-01, TC-C-02, TC-C-08 |
| Coordinator owns its tasks and acts only in its project | same; Phase 7 items 5, 14 | TC-C-03, TC-C-04 |
| Parent channel: reports, never coordinator chat | Phase 7 items 18 to 22 | TC-C-05 |
| Idle by default; removal refused with open work | `docs/chat.md` | TC-C-06, TC-C-07 |
| Etiquette block on every turn | `docs/chat.md` "How the orchestrator writes to you" | TC-E-01 |
| Pull request links from records only | same; Phase 8 items 4 to 7 | TC-E-02, TC-E-03 |
| Wake caps 20 and 160, half for work checks; quiet updates | `docs/settings.md` "Automatic replies" | TC-E-04, TC-E-05 |
| Upgrade keeps notes, tasks, cards; migrations 0064 to 0074 | Integration I0, I1 | TC-M-01 |
| Every Settings page, section and operation | `apps/desktop/src/ui/views/settings/settingsPages.tsx`; `docs/settings.md` | TC-ST-01 to TC-ST-22 (the table in `12-settings.md` lists each one; `sh docs/manual-tests/manual-test.sh settings-coverage` checks it) |
