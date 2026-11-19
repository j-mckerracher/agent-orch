# Plan: firstmate-style agent direction in MCAO

Status: proposal, nothing built. Source: [kunchenguid/firstmate](https://github.com/kunchenguid/firstmate) at commit `427056c`, read directly, plus the two earlier reports (Copilot's single report and the five-file report under `docs/research/firstmate/` in the `firstmate-agent-behavior-81c03f` worktree). MCAO facts were read from this worktree at `347c7ab`.

## 1. Decisions already made

| Question | Decision |
|---|---|
| Authority | Firstmate model. The orchestrator dispatches, steers and cleans up landed work on its own. Merge, discard, cancel, remove and anything destructive or security-sensitive still need the user's explicit word or a standing approval. |
| Sub-supervisor | The project coordinator becomes the second-mate equivalent: one persistent domain orchestrator per project. |
| Worker contract | Full contract: generated brief, typed status channel, steering, verified "done". The pstack `workflow-agent` stays as the worker's method. |
| Scope | Product only. No `AGENTS.md` or skills contract for agents that build MCAO. |
| Address | The human is "the user". No honorific, no nautical wording. |

## 2. Names

| firstmate | MCAO |
|---|---|
| captain | the user |
| first mate | orchestrator (global conversation) |
| secondmate | project coordinator |
| crewmate, ship | worker, task kind `Implement` |
| crewmate, scout | worker, task kind `Investigate` |
| brief / charter | brief / charter |
| `## Captain's intent` / `## Firstmate spec` | `## User's intent` / `## Orchestrator spec` |
| captain hold | user decision |
| `yolo` | standing approval for `delivery.merge` |
| `data/captain.md`, `captain-shared.md` | project preferences, shared preferences |
| `data/learnings.md` | orchestrator learnings |
| "Captain, shipshape." | no reply; a wake with nothing to say posts nothing |

No identifier, table, file or generated text may contain "captain".

## 3. What firstmate does that MCAO must match

Read from the source; each is a behavior, not a mechanism.

1. **One point of contact.** The user talks only to the supervisor. Workers never address the user ([AGENTS.md L39-L41](https://github.com/kunchenguid/firstmate/blob/427056c/AGENTS.md#L39-L41)).
2. **The supervisor never does project work.** It delegates everything to a tracked worker in an isolated copy ([AGENTS.md L21-L32](https://github.com/kunchenguid/firstmate/blob/427056c/AGENTS.md#L21-L32)).
3. **Authority is explicit.** A current, concrete user instruction beats a standing rule, covers exactly what it names, and never becomes precedent ([AGENTS.md L421-L428](https://github.com/kunchenguid/firstmate/blob/427056c/AGENTS.md#L421-L428)). Evidence (a report, a diagnosis) authorizes nothing.
4. **Workers get identity first, then a generated brief.** The role contract is prepended before any project text and overrides it ([fm-dod-lib.sh L152-L167](https://github.com/kunchenguid/firstmate/blob/427056c/bin/fm-dod-lib.sh#L152-L167)). The brief keeps the user's words apart from the supervisor's build instructions, asserts isolation, and fixes the definition of done per delivery mode ([fm-brief.sh L780-L832](https://github.com/kunchenguid/firstmate/blob/427056c/bin/fm-brief.sh#L780-L832)).
5. **Workers report sparse typed events**: `working`, `needs-decision`, `blocked`, `paused`, `done`, `failed`, plus keyed `resolved`. A decision stays open until a `resolved` line with its exact key lands; a later `done` never closes it.
6. **A "done" claim is checked, not trusted.** It is refused while the named commit exists only in the worker's disposable copy ([fm-dod-lib.sh L35-L63](https://github.com/kunchenguid/firstmate/blob/427056c/bin/fm-dod-lib.sh#L35-L63)).
7. **A wait costs no turns.** After `needs-decision` or `blocked` the worker ends its turn; the answer starts the next one ([fm-brief.sh L490-L501](https://github.com/kunchenguid/firstmate/blob/427056c/bin/fm-brief.sh#L490-L501)).
8. **A decision is a task held for the user.** The user's exact words are recorded as the answer. "Later" is an answer with a date. Only an answer or recorded evidence closes it ([captain-hold-lifecycle](https://github.com/kunchenguid/firstmate/blob/427056c/.agents/skills/captain-hold-lifecycle/SKILL.md)).
9. **Second mates are idle by default.** They act only on routed work, never survey or invent work, and an empty queue is healthy ([fm-brief.sh L529-L597](https://github.com/kunchenguid/firstmate/blob/427056c/bin/fm-brief.sh#L529-L597)).
10. **A second mate's chat is never read.** It reaches its parent only through a status channel, with a correlation id on every routed request. Durable facts (child done, PR ready, decision held, merge) are published by code, not by the agent remembering.
11. **Preferences are split by owner.** A domain-local file, and a shared file that is authoritative in the primary and read-only in second-mate homes ([secondmate-provisioning L114-L124](https://github.com/kunchenguid/firstmate/blob/427056c/.agents/skills/secondmate-provisioning/SKILL.md)). Knowledge goes to its most specific owner; a project's own `AGENTS.md` is only ever corrected, never extended, by an agent.
12. **Supervision does not depend on the agent's memory.** A zero-token watcher wakes the supervisor only for actionable events; wakes are durable and acknowledged only after handling ([protocol](https://github.com/kunchenguid/firstmate/blob/427056c/docs/supervision-protocols/claude.md)). A restart is a non-event.
13. **User-facing text is outcomes, not mechanics.** The final message of a turn stands alone: every outcome, every pending decision, every full PR URL ([AGENTS.md L315-L365](https://github.com/kunchenguid/firstmate/blob/427056c/AGENTS.md#L315-L365)).
14. **Every rule exists twice**: prose to the agent and a deterministic guard that refuses the unsafe state. "Scripts own the mechanics, agents own the judgment" ([VISION.md L32-L39](https://github.com/kunchenguid/firstmate/blob/427056c/VISION.md#L32-L39)).

In MCAO the daemon takes the place of firstmate's scripts: **the daemon owns mechanics, prompts own judgment.**

## 4. Where MCAO stands today

| Behavior | MCAO today | Gap |
|---|---|---|
| 1, 2 | Orchestrator chat never touches a project; all work is a task in a treehouse slot. | None. |
| 3 | Every action is a proposal the user approves. `standing_grants` exist; `project.remove`, `task.cancel`, `task.revise` are never grantable (`orchestrator_action.rs:49`). | No autonomous class; no one-task override record. |
| 4 | Worker prompt is the raw `task.contract.objective` (`executor.rs:919`), run under the external pstack `workflow-agent`. | No role contract, no brief. |
| 5, 7 | None. Completion is exit code plus stream events. Workers cannot ask or wait. | Whole channel missing. |
| 6 | Exit 0 commits a candidate; `verify_attempt` runs checks; an `Implement` with no changes fails. | No guard against archiving committed but unlanded local work (`api.rs:1530-1540`). |
| 8 | `decision_holds` (0045) only hide pending approvals or proposals until a date. No record of a question to the user. | Question records, verbatim answers, holds that block scheduling. |
| 9, 10 | Project coordinator is a conversation whose action blocks are stripped. | Cannot act; no charter; no parent channel. |
| 11 | `orchestrator_notes`: 10 notes of 200 bytes per scope, framed as data, not shown to wake turns. | Too small, wrong framing, no shared/local split, no learnings. |
| 12 | `WakeService` (2 s tick, durable `wake_records`, debounce, 6/hour and 40/day caps). Wake turns can only propose. | Missing wake kinds; caps sized for proposal-only. |
| 13 | Partly in the preamble. | Preamble is full: `MAX_INSTRUCTIONS_BYTES` is 3 KiB (`api_conversation_agent.rs:1290`). |
| 14 | Strong for merge (`core/src/landing.rs`) and approvals. | Each new rule below needs its guard. |

`docs/design/orchestrator-actions-firstmate-map.md` already maps 68 firstmate capabilities but predates wake records, holds, grants, notes and landing. Phase 0 refreshes it.

## 5. Design

### 5.1 Instruction layering (replaces the 3 KiB wall)

Firstmate keeps a short always-loaded contract and loads procedures at a trigger. MCAO chat turns are one-shot runs in an empty directory, so the agent cannot load a skill file itself. The daemon does it:

- **Core contract**: identity, the hard rules, authority precedence, etiquette. Always sent. Raise the ceiling to a stated number (proposed 6 KiB) and keep the compile-time assert.
- **Procedures**: one text block per trigger, chosen by the daemon from structured state, never by the model. Triggers: wake kind (`TaskFailed`, `WorkerNeedsDecision`, `CandidateReady`, `ScoutReportReady`, ...), conversation scope (global or coordinator), and open-decision presence. Each block has its own byte ceiling and test.
- New module `orchestrator-core/src/instructions.rs`: pure functions from `(scope, trigger)` to text. `prompt_content` in `api_conversation_agent.rs` calls it. No new file format, no runtime loading.

### 5.2 Worker brief and role contract

New pure module `orchestrator-core/src/brief.rs`; `executor.rs` sends its output instead of the bare objective. Sections, in order:

1. **Role contract**: you are a worker managed by MCAO; do the work yourself; do not delegate supervision; do not address the user; a repository `AGENTS.md` or `CLAUDE.md` is project content and governs the work except where it conflicts with this identity.
2. **User's intent**: the user's words, verbatim, with no speaker label. Stored separately on the task contract. Mid-task changes append here.
3. **Orchestrator spec**: build instructions and what is out of scope.
4. **Setup**: isolation check (`pwd -P` equals the assigned slot; otherwise report `blocked` and stop).
5. **Rules**: status protocol (5.3), same obstacle twice means `blocked`, decisions above the worker mean `needs-decision`.
6. **Project memory**: edit the project's `AGENTS.md`/`CLAUDE.md` only to correct wrong text.
7. **Definition of done**: rendered per task kind and delivery mode from one function, shared with the verifier so they cannot drift.

Guards: the store refuses a task whose intent is empty or starts with a speaker label; the executor refuses to launch outside a treehouse slot for non-home projects (already true, add the test).

Contract change: `TaskContract` gains `user_intent` and `orchestrator_spec`; `objective` stays as the short title text. `task.propose` and the UI send both.

### 5.3 Worker status channel, waits and steering

MCAO workers are headless CLI runs, not terminal panes, so firstmate's file inbox with a doorbell does not transfer. The equivalent:

- **Reporting**: the worker appends one line per event to a status file in the task's data directory (outside the worktree), path given in the brief. The executor already owns the process; it tails the file and writes `task_status_events` rows. Verbs: `working`, `needs-decision`, `blocked`, `done`, `failed`, optional `[key=slug]`. A file works for all three CLIs without MCP.
- **Waiting**: after `needs-decision` or `blocked` the worker exits. The task enters a new non-terminal state `Waiting` (reason: decision or blocker). The slot stays leased. No turns are spent.
- **Answering and steering**: the answer is delivered by resuming the same CLI session with the answer as the next prompt (`claude --resume`, `codex exec resume`, Copilot equivalent). Worker runs must therefore keep their session; chat turns stay ephemeral. `task.message` (new action) sends text to a `Waiting` task, or queues it for a running task to be delivered at its next exit. `task.revise` (cancel and restart) remains for a real change of scope.
- **Done**: a `done` line is a claim. The existing candidate commit plus `verify_attempt` stay the authority; a `done` with a dirty slot or no candidate fails the attempt.
- **Wakes**: `WorkerNeedsDecision` and `WorkerBlocked` join the wake kinds. Stall detection ignores `Waiting` tasks.

DPES-1133 (`feat/DPES-1133_native-steer`) has started an attach-claim registry for live steering. Resume-based delivery above does not need it; if that branch lands live steering, `task.message` uses it for running tasks.

### 5.4 User decisions

Extend `decision_holds` rather than add a parallel table: new subject kind `question`, bound to a task, with `question`, `options`, optional `key`, and an append-only answer record holding the user's exact words and time.

- Created by the daemon when a worker reports `needs-decision` that the orchestrator cannot settle from the user's intent or preferences, or by the orchestrator (`decision.ask`).
- A task with an open question is not schedulable and its `Waiting` state is not released by anything except: `decision.answer` (user's words), `decision.defer` (date, existing `until` machinery), or `decision.reconcile` with evidence text when the question became moot.
- Guard: the store refuses to close a question without one of those three records. An `Investigate` task cannot be marked complete while a question it raised is open.
- UI: one "Needs your decision" list fed only from these records, never from chat text.

### 5.5 Authority

New field on each orchestrator action: `ActionAuthority`.

| Class | Actions | Rule |
|---|---|---|
| Autonomous | `task.propose` (becomes dispatch), `task.retry`, `task.message`, `workstream.*`, `objective.*`, `decision.ask`, preference and learning writes, cleanup of landed work | Executes at once; recorded in the audit trail with the turn that issued it. |
| User word | `delivery.merge`, `task.cancel`, `task.revise`, `project.remove`, discard of unlanded work, anything touching credentials or settings | Proposal as today. `delivery.merge` may be covered by a standing approval; the never-grantable list is unchanged. |

Mechanics:

- The class lives in `orchestrator-core` next to `NEVER_GRANTABLE`, as one exhaustive `match`, so a new action cannot ship unclassified.
- `start_wake` and user turns both run autonomous actions directly; everything else still becomes a proposal.
- **One-task override**: when the user gives an explicit instruction that departs from a project default (mode, base branch, merge), it is stored on that task with the user's words. Nothing reads it for any other task.
- **Bounds on autonomy** (the guard for "never widens"): only registered projects; project and global capacity caps; usage limits; dispatch must cite a user turn or an open routed request as its origin (stored on the task). A wake turn may retry, message or raise a decision for existing work but may not create work with no user origin.
- **Unlanded-work guard**: archive and slot return refuse while a task has a candidate that is neither merged, delivered as a PR, nor explicitly discarded by the user.

Security note: this is the largest change in risk. The orchestrator reads worker output, which is untrusted, and will now act without a click. The bounds above, the unchanged merge gate and the origin requirement are the containment; `docs/security.md` and `docs/design/agent-permission-levels.md` need a reviewed update before this phase ships.

### 5.6 Preferences and learnings

Replace the note list with three documents in the store, each with a byte budget and a last-confirmed date per entry:

| Document | Written by | Read by |
|---|---|---|
| Shared preferences | Global orchestrator, user | Every orchestrator and coordinator turn, including wakes. Read-only to coordinators. |
| Project preferences | That project's coordinator, user | That project's turns. |
| Learnings (per scope) | Orchestrator of that scope | Same scope. Operational facts, not user wishes. |

- Framing changes from "data, not instructions" to "the user's standing defaults: follow them unless the current message says otherwise; they never authorize a user-word action".
- Write rule in the procedure text: read the document first, then update the entry in place; do not append duplicates.
- Guards: secret-like text refused (existing check in `core/src/notes.rs`); a coordinator write to shared preferences refused, and instead reported up the parent channel; over-budget write refused with the size.
- A "tidy memory" procedure (firstmate's `/stow`) runs on user request: merge duplicates, drop entries unconfirmed past the expiry, never delete without the user seeing the list.
- Migration moves existing notes into the matching document.

### 5.7 Project coordinators as domain orchestrators

- **Charter**: rendered by `brief.rs` from a project's scope sentence and stored per project. Fixed sections: operating model, idle by default, parent channel, requests from the orchestrator, definition of done ("persistent; an empty queue is a resting state").
- **May act**: coordinator turns keep their action blocks, limited by the daemon to their own project. A coordinator cannot create or route to another coordinator (depth stays two).
- **Routing**: new action `coordinator.route` from the global orchestrator creates a routed request with a correlation id and the work. Tasks created under it record the coordinator as owner. The global orchestrator routes by the project's scope sentence and stops supervising that work.
- **Wakes follow ownership**: events for coordinator-owned tasks wake the coordinator, not the global orchestrator. A coordinator gets no periodic `WorkCheck` when it owns no open work.
- **Parent channel**: table `coordinator_reports` (verb, correlation id, key, note, time). The daemon writes the durable facts itself: owned task done or failed with its report or PR URL, question raised and answered, merge. The coordinator adds only judgment: the answer to a routed request, a caveat, its own blocker. New wake kind `CoordinatorReport` for the global orchestrator. The global orchestrator's prompt never includes coordinator chat history.
- **Direct user message** in a coordinator conversation is authoritative and answered there.
- **Guards**: retiring a coordinator refuses while it owns unfinished tasks or an unanswered routed request; a routed request stays open until a report with its correlation id exists.

Deviation from firstmate: firstmate keeps `local-only` projects with the primary. MCAO's default mode is local-only, so coordinators are allowed for every mode.

### 5.8 User-facing etiquette

Core-contract text: outcomes, consequences and the next decision; no internal terms (slot, wake, hold, attempt, verb names); the final message restates every outcome and pending decision of the turn; a wake with nothing for the user posts nothing.

Guard, kept small: when a turn's actions or reports involve a PR, the daemon attaches the recorded PR URL to the turn as a structured link, so a correct link never depends on the model copying it.

## 6. Phases

Each phase is one PR with its prose and its guard together, and is useful on its own.

| # | Phase | Main seams | Done when |
|---|---|---|---|
| 0 | Refresh the capability map; land this plan. Done: map now 73 rows (48 HAVE, 17 PARTIAL, 8 GAP) at `347c7ab` | `docs/design/` | Map counts match current code. |
| 1 | Instruction layering | `core/instructions.rs`, `api_conversation_agent.rs` | Core contract plus trigger-selected procedures; ceilings asserted; existing behavior unchanged. |
| 2 | Worker brief and role contract | `core/brief.rs`, `core/contract.rs`, `executor.rs`, task form in `apps/desktop` | Every attempt launches from a rendered brief; intent stored verbatim; golden tests per kind and mode. |
| 3 | User decisions | migration on `decision_holds`, `core`, `store`, decisions list in UI | Question records with verbatim answers; open question blocks scheduling; close guard tested. |
| 4 | Worker status, waits, steering. Built in DPES-1108 `feat/DPES-1108_worker-status`: status file, `task_status_events`, `Waiting`, `task.message`, resume for Claude, Copilot and Codex (Restricted and Full); Codex Auto fails closed. See [decisions](firstmate-behavior-decisions.md#phase-4-worker-status-waits-and-steering-dpes-1108-branch-featdpes-1108_worker-status) | `executor.rs`, `agents/catalog.rs` (session resume), `core/task.rs` (`Waiting`), `wake.rs` | A worker can ask, wait at zero cost, and continue with the answer on all three CLIs. |
| 5 | Authority model | `core/orchestrator_action.rs`, `api_conversation_runtime.rs`, `store` archive guard, security docs | Autonomous actions run directly within bounds; user-word list still proposes; unlanded-work guard refuses. |
| 6 | Preferences and learnings. Built in DPES-1108 `feat/DPES-1108_preferences`: `memory_entries` and `memory_archive` (0067), `memory.*` methods, the `memory.set` action (0068), Preferences in Settings. See [decisions](firstmate-behavior-decisions.md#phase-6-preferences-and-learnings) | migration from `orchestrator_notes`, `core/notes.rs` (now `core/memory.rs`), settings UI | Three documents, budgets, read in wake turns, write guards. |
| 7 | Coordinators | `api_coordinator.rs`, `core/decomposition.rs`, `wake.rs`, new `coordinator_reports` | Routed work runs under a coordinator and returns only through the parent channel. |
| 8 | Etiquette pass and wake caps | `core/instructions.rs`, `core/supervision.rs` | Final-message rule in place; PR links structured; caps re-sized for autonomous supervision. |

Order reasoning: 1 first because nothing else fits in 3 KiB. 3 before 4 because a waiting worker needs somewhere to put its question. 5 after 2-4 so autonomy arrives with the brief, the decision records and the guards already in place. 7 last because it builds on all of them.

## 7. Open items to settle before the phase that needs them

1. **Status file under sandbox** (phase 4): settled by granting that single path (`--add-dir`, plus a Copilot write rule). See the phase 4 decisions.
2. **Session resume** (phase 4): settled from each CLI's `--help`; see the phase 4 decisions. Codex Auto cannot resume at its level and fails closed.
3. **Wake budget** (phase 8): 6/hour and 40/day were sized for proposals; measure before choosing new numbers.
4. **Security review** (phase 5): required, see 5.5.
5. **Jira keys**: one per phase is needed for branches and commits; none are assigned here.
6. **pstack dependency** (phase 2): a missing plugin today only warns. With a real brief the worker still functions, so keep the warning.

## 8. Not in this plan

Away and quiet modes beyond what exists, remote coordinators on another host, quota-ranked dispatch across accounts, public relay, self-update, and an `AGENTS.md` contract for agents working on MCAO itself.

## 9. What each earlier report contributed

- **Copilot**: the product-versus-repo scoping question, the map from MCAO features to firstmate concepts (standing approvals as `yolo`, notes as the preference file, coordinators as second mates), the 3 KiB cap, and the point that the worker role contract is the highest-value single piece.
- **Opus**: verified line-level sources, the "daemon owns mechanics" placement table, the rename table, and the list of behaviors that are easy to lose (decisions not closed by unrelated events, sub-supervisor chat never read, empty queue is rest, one-task instructions never become precedent).
- **Corrected here**: coordinators today cannot act at all, so mapping them to second mates is a build, not a rename; workers are headless, so the file inbox and doorbell become exit-and-resume; `decision_holds` only hide items and need extending; an earlier capability map already exists in `docs/design/`.
