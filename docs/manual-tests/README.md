# Manual UI tests: Global Orchestrator

These test cases check four documented behaviors of the Global Orchestrator through the desktop app UI. An AI agent with computer use runs them. A human can run them too.

| # | Behavior | File |
|---|----------|------|
| 1 | The Global Orchestrator dispatches agents to do work | [01-dispatch.md](01-dispatch.md) |
| 2 | The Global Orchestrator wakes to check on its agents | [02-wake.md](02-wake.md) |
| 3 | The Global Orchestrator starts sub-orchestrators | [03-sub-orchestrators.md](03-sub-orchestrators.md) |
| 4 | The Global Orchestrator is the main intermediary between the user and any sub-orchestrator or worker | [04-intermediary.md](04-intermediary.md) |

Start with [00-setup.md](00-setup.md). Every test file assumes that setup is complete.

## How the docs define these terms

The docs describe these behaviors differently from the plain-language names above. The tests follow the docs.

- **Dispatch.** The agent ends a reply with one structured action block. The daemon turns it into a proposal and shows an approval card. The task runs only after the user approves it (`docs/design/orchestrator-actions.md` N1; `docs/m7-08-conversation-service.md` "task proposals").
- **Wake.** A wake turn is an automatic Global turn started by an event, such as a failed task, or by a timer while work is active. It is not a user message (`docs/design/orchestrator-actions.md` "Wake").
- **Sub-orchestrator.** A sub-orchestrator is a workstream conversation. The user approves a `workstream.create` proposal to make one. It has its own entry in the conversation target picker. It cannot create workstreams itself.
- **Intermediary.** The Global conversation reaches the user through approval cards, wake notices, relayed scout reports, and the "Routed here by the global orchestrator" strip. The user talks to a workstream directly by choosing it in the target picker. The docs do not describe the Global Orchestrator forwarding chat text to workers. A test that expects that is out of scope.

## Rules for the AI runner

1. Run tests in file order. Run each test in step order. Do not skip steps.
2. Take a screenshot at every step marked **Screenshot**. Save it as `docs/evidence/manual-tests/<run-UTC>/<TC-ID>-<step>.png`. `<run-UTC>` looks like `20261117T180000Z`.
3. Compare UI text with the quoted expected strings. Quoted strings are exact. Treat a difference in wording as a FAIL and quote what you saw.
4. Record one result per test case: `PASS`, `FAIL`, or `BLOCKED`.
   - `FAIL`: a step ran and the result differed from the expected result.
   - `BLOCKED`: a precondition could not be met. Say which one.
5. After the last test, write `docs/evidence/manual-tests/<run-UTC>/RESULTS.md`. Put one row per test case: ID, result, and one line of notes.
6. Do not run a paid agent. Use the fake agent from `00-setup.md`. Stop and report if a step would call a real provider.
7. Do not use the keyboard to change application state when a UI control exists. Use the UI control.
8. If the app shows "Fixture data: nothing was saved", the app is not connected to a real daemon. Stop and fix setup. Results from fixture mode do not count.

## Test case format

Each test case has:

- **ID.** `TC-D-nn` (dispatch), `TC-W-nn` (wake), `TC-S-nn` (sub-orchestrator), `TC-I-nn` (intermediary).
- **Source.** The doc and the line the expected behavior comes from.
- **Preconditions.** State that must be true before step 1.
- **Steps.** Numbered. Each step names the control to use.
- **Expected.** What must be visible after the steps.

## Coverage matrix

| Documented behavior | Source | Test cases |
|---------------------|--------|------------|
| Dispatch defaults and precedence | `docs/settings.md` "Agent dispatch defaults" | TC-D-01, TC-D-02, TC-D-03 |
| Task proposal card, approve, reject | `docs/design/orchestrator-actions.md` N1 | TC-D-04, TC-D-05, TC-D-06 |
| Dependent card order | `actionCardModel.ts` | TC-D-07 |
| Home directory when project is unclear | `docs/settings.md` "Home directory" | TC-D-08, TC-D-09 |
| Load gate | `docs/design/orchestrator-actions.md` "Load gate" | TC-D-10, TC-D-11 |
| Actions that always need approval | `docs/settings.md` "Actions that always need approval" | TC-D-12 |
| Standing approvals | `docs/settings.md` "Standing approvals" | TC-D-13 |
| Proposal only at Restricted; project chats do not propose | `docs/chat.md`; `orchestrator-actions.md` | TC-D-14, TC-D-15 |
| Default agent order and unavailable agent | `docs/chat.md`; `docs/m7-08-conversation-service.md` | TC-D-02, TC-D-16 |
| Monitoring chip and modes | `docs/design/orchestrator-actions.md` "Wake" | TC-W-01, TC-W-02, TC-W-03 |
| Wake on failed task | same | TC-W-04 |
| Notice-only wakes (stalled, delivery waiting) | same | TC-W-05 |
| Quiet reply | same | TC-W-06 |
| User turn cancels wake | same | TC-W-07 |
| Limits and "Monitoring paused: limit reached" | same; `docs/settings.md` "Automatic replies" | TC-W-08 |
| Three failures pause automatic updates | same | TC-W-09, TC-W-10 |
| Away mode and return brief | `docs/m7-08-conversation-service.md` | TC-W-11 |
| Check interval settings | `docs/settings.md` "Checks on running work" | TC-W-12 |
| Wake badge and label | `monitoringModel.ts` | TC-W-04, TC-W-13 |
| `workstream.create` proposal | `orchestrator-actions.md` "Secondmates" | TC-S-01, TC-S-02, TC-S-03 |
| Workstream in picker and "Talking to:" | `docs/chat.md` | TC-S-04 |
| Routing a task to a workstream | `orchestrator-actions.md` Phase 6a | TC-S-05, TC-S-06 |
| Workstream scope limits | `docs/m7-08-conversation-service.md` | TC-S-07, TC-S-08 |
| Workstream counts as work unit | `orchestrator-actions.md` | TC-S-09 |
| Escalate to Global | `docs/ui/wireframe.md` | TC-S-10 |
| Target picker lists all targets | `docs/chat.md` | TC-I-01 |
| User talks to a workstream or coordinator | `docs/chat.md` | TC-I-02, TC-I-03 |
| Scout report relay, automatic and manual | `docs/m7-08-conversation-service.md` | TC-I-04, TC-I-05, TC-I-06, TC-I-07 |
| Approvals surface to the user | `orchestrator-actions.md` | TC-I-08 |
| Task events reported up to Global | same | TC-I-09 |
| End-to-end flow | all | TC-I-10 |
