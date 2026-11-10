# Initial desktop wireframe and data boundaries

Planning artifact only. No UI, fixture, IPC, or backend implementation is added by this change.

Reference: [technical implementation plan](/Users/mckerracher.joshua/Code/agent-orch-worktrees/mckerracher-joshua-mclm-friendly-guide/docs/technical-implementation-plan.md), §§2, 9, 10, 12; §11 explains coordinator authority and §14 limits the initial desktop scope. Execution is governed by `orch-impl-m.md`.

## Scope and evidence

Screens S1–S6 below are the entire initial UI scope. C1 is a shared connection/error state inside those screens, not an extra destination. M5-03 owns projects, tasks, coordinator and client connection presentation; M5-04 owns review; M5-05 owns packaged window lifecycle and native connection behavior.

- REAL means an existing or in-flight M5-01/M5-01b/M5-02 contract, **not** a claim that it works in this checkout or has passed final native acceptance.
- DUMMY means an unavailable service or read model. Every such component uses the named interface and fixture in the registry below. Fixture paths are planned artifacts to be created by the implementing UI item, not files provided by this documentation change.
- Local navigation, selection, drafts and filtering are UI state associated with the indicated REAL records; they do not require extra daemon methods.
- This checkout already has a minimal React/Vite frontend (`apps/desktop/src/App.tsx` only renders a heading) and Tauri scaffold. Its daemon/IPC files are placeholders. Board history records separate, unverified UI candidates. Do not overwrite or accept those candidates based on this wireframe.
- REAL contracts were checked read-only against the board-pinned M5-02 candidate `b160e48398db1b1aab93fa851ca4b2c4d27c3bc1`: `crates/orchestrator-daemon/src/api.rs`, `crates/orchestrator-ipc/src/{client,lib,session}.rs`, and the serialized task/approval records. Integrators must recheck against the eventual accepted M5-02 revision.
- Do not reuse any earlier HTML design from a different checkout as an existing wireframe for this repo. This document is the first wireframe here.

## Navigation

```text
S1 Projects -> S2 Project detail -> S3 Task list -> S4 Task detail
                   |                    ^              |
                   +-> S5 Coordinator --+              +-> S6 Review
                   +-------------------------------> S6 Review

Every screen: project breadcrumb / back navigation + shared connection strip.
S6 Review returns to its task or project. S5 may link proposed tasks only after
an explicit engine submission returns canonical task IDs.
```

## Shared connection strip and error states — C1 (M5-03; native lifecycle M5-05)

```text
[Projects / selected project / task]  [Connected | Reconnecting... | Offline]
[Last durable sequence: n] [Refresh / reconnect]
+-----------------------------------------------------------------------+
| When disconnected: cached data, last received time, actions unavailable |
| When replay fails: history incomplete; refresh required                |
| Inline error: reason, affected action, safe next step                  |
+-----------------------------------------------------------------------+
```

| Component | Source | Data and daemon dependency | Navigation / behavior |
| --- | --- | --- | --- |
| Breadcrumb and selection | REAL | IDs from `projects.list`, `tasks.list`, `tasks.get`; local route state | Back to S1/S2/S3; no additional RPC |
| Connection strip and reconnect control | REAL | M5-01 authenticated transport/version and negotiated IPC capabilities; M5-01b Windows named pipe; M5-02 client reconnect and `events.subscribe` | Connecting, connected, reconnecting, disconnected, authentication failed, incompatible version/capability; never show tokens |
| Last durable event / timeline freshness | REAL | `events.subscribe` with `after_sequence`; `events.event` notification fields `sequence`, `task_id`, optional `session_id`, `kind`, `payload`, `created_at`; `events.error` | Apply each sequence once; update freshness only on receipt, not on heartbeat assumptions |
| Cross-restart/reset journal identity indicator | DUMMY D6 | `JournalIdentitySource`; no store epoch/reset identity in M5-02 | Label placeholder. Never assert a resumed history is complete solely because the sequence fits |
| Data empty/loading/error region | REAL | Loading/request outcome of the screen's methods; protocol authentication, unsupported capability and timeout failures; M5-02 `NotFound`, `InvalidParams`, `Conflict`, `Forbidden`, `IdempotencyConflict`, `IdempotencyInProgress`, `ReplayGap`, `SlowConsumer`, `ResultTooLarge`, `Internal` | Empty is distinct from disconnected/error. Keep cached rows visibly stale; explicit retry/refresh |

Connection sequence: connect/authenticate/negotiate -> subscribe and hydrate records without losing intervening events -> connected. On disconnect disable mutations, retain drafts, reconnect/authenticate, replay from the last applied sequence, deduplicate and refresh affected snapshots. On replay gap/slow-consumer, mark history incomplete and resubscribe/refetch without showing invented continuity. Because M5-02 accepts some stale cursors after store replacement (known D3), start a fresh baseline after reopening or suspected restart/reset until D6 has a real source; do not persist a cursor as proof of cross-process continuity. Do not automatically replay unresolved approval/delivery mutations with a new key. Reconcile state first; if the same logical request is retried, retain its idempotency key.

## S1 — Project list (M5-03)

```text
[C1 connection strip]
Projects                              [Register repository: absolute path]
+-----------------------------------------------------------------------+
| Repository identity / canonical path                 [Open]            |
| ...                                                                   |
+-----------------------------------------------------------------------+
[No repositories registered | loading | registration/read error]
```

| Component | Source | Data / method or event | Navigation |
| --- | --- | --- | --- |
| Project rows and open action | REAL | `projects.list` returns `{id}` rows; current candidate uses canonical repository path as ID. Derive display basename locally; no project settings/details API assumed | Row -> S2 |
| Repository registration | REAL | `projects.register` with absolute `path`; response `id`; refresh `projects.list` | Open registered project in S2; invalid/non-repository path stays inline |
| Empty/loading/error | REAL | C1 applied to the two project methods | Retry in place |

## S2 — Project detail and submission (M5-03)

```text
[C1] Projects > selected repository
Repository: canonical project ID/path
[Tasks] [Coordinator] [Pending review]
[State counts derived from this project's returned tasks]
+-----------------------------------------------------------------------+
| New task: objective; kind; agent; dependencies; verification; delivery  |
| autonomy; retry/repair limits                            [Submit]      |
| Agent detection: PLACEHOLDER DATA (D1)                                 |
| Execution availability: PLACEHOLDER DATA (D2)                           |
+-----------------------------------------------------------------------+
```

| Component | Source | Data / method or event | Navigation |
| --- | --- | --- | --- |
| Project identity and counts | REAL | Selected `projects.list` ID; `tasks.list` filtered locally by `contract.project_id` and grouped by returned state, with explicit denominator | Counts -> S3 with local filter |
| Task contract form and submission receipt | REAL | `tasks.submit` with `idempotency_key` and validated `contract` (project, parent, objective, kind, agent, dependencies, verification, delivery, autonomy, max_retries, max_repairs); returned `TaskRecord` | Success -> S4; admission/queueing is not agent launch |
| Agent detection / support selector | DUMMY D1 | `AgentCapabilitiesSource`; `agents.capabilities` belongs to M5-02b, not M5-01 IPC capability negotiation | Label fixture options; a fixture must never pick an installed agent for a live submission automatically |
| Scheduler / launch availability preview | DUMMY D2 | `TaskExecutionSource`; M5-02 only admits/queues tasks | Fixture-only launch demo -> fixture S4; never claim real execution |
| Task / coordinator / review navigation | REAL | Project ID, `tasks.list`, `approvals.list` with task-to-project association | S3, S5, S6 |

## S3 — Task list (M5-03)

```text
[C1] Project > Tasks                         [Back to project / New task]
[All | queued | running | blocked | awaiting review | terminal]
+-----------------------------------------------------------------------+
| Task ID | objective | returned state | agent | current recorded session |
| ...                                                      [Open]       |
+-----------------------------------------------------------------------+
[Execution preview: PLACEHOLDER DATA (D2), in a separate fixture dataset]
```

| Component | Source | Data / method or event | Navigation |
| --- | --- | --- | --- |
| Task rows, state filters, counters | REAL | `tasks.list`, filtered locally by project; contract ID/objective/agent/dependencies, returned state, recorded sessions; refresh via `events.event` after `events.subscribe` | Row -> S4; no invented server filter parameters |
| Execution/scheduling preview rows | DUMMY D2 | `TaskExecutionSource`; queued/running/waiting/failed fixture scenarios | Fixture-only S4; visibly separate from REAL IDs |
| List empty/loading/error and new task link | REAL | C1 and task-list outcome; selected project ID | Retry or S2 |

Do not invent a reliable progress percentage or synthesize Running from task submission. A REAL row may legitimately have no session before M5-02b is integrated.

## S4 — Task detail, session and timeline (M5-03)

```text
[C1] Project > Tasks > Task ID                      [Review candidate]
Objective / contract / state / version / dependencies
Attempts: recorded IDs + states | Sessions: [select recorded session]
+-----------------------------------------------------------------------+
| Timeline: sequence/time | reported kind | recorded payload              |
| older session events [Load more]                                       |
+-----------------------------------------------------------------------+
[Cancel] [Retry: PLACEHOLDER DATA D4]
[Execution outcome: PLACEHOLDER DATA D2]
[Recovery classification: PLACEHOLDER DATA D3]
```

| Component | Source | Data / method or event | Navigation |
| --- | --- | --- | --- |
| Task summary and attempts | REAL | `tasks.get({task_id})` -> contract, state, attempts, sessions, audit events, version | Back to S3; dependencies -> another S4 |
| Recorded session selector/detail | REAL | `sessions.list({task_id})`, `sessions.get({task_id,session_id})`; display only returned identity/adapter/state fields | Select recorded session on S4; no session-created event assumed |
| Event timeline and pagination | REAL | Task audit events from `tasks.get`; session replay through `sessions.events({task_id,session_id,after_sequence})` with `more` / `next_after_sequence`; live `events.subscribe` notifications filtered locally | Load more in place; unknown event kinds remain raw, labelled records |
| Cancellation request and acknowledgement | REAL | `tasks.cancel({task_id,idempotency_key})`, then `tasks.get`/events; only expose supported state/capability, surface Conflict/Forbidden | Stay S4; request acceptance is not proof of process termination |
| Running process launch / scheduling / final cancellation effects | DUMMY D2 | `TaskExecutionSource`; no daemon executor owner yet | Fixture-only controls; cannot invoke live agents or mutate REAL records |
| Retry a new attempt | DUMMY D4 | `TaskRetrySource`; `tasks.retry` is out of M5-02 scope | Fixture receipt/new attempt within fixture S4 |
| Restart recovery classification | DUMMY D3 | `RecoveryStatusSource`; actual process identity, workspace and partial-delivery reconciliation not exposed | Placeholder recoverable/blocked/completed/inspection state; no live Resume promise |
| Candidate-review navigation | REAL | `approvals.list` filtered by task; available approval ID, task and candidate identity | S6; no pending approval -> honest empty state |

## S5 — Coordinator view (M5-03)

```text
[C1] Project > Coordinator
[PLACEHOLDER DATA — coordinator conversation service unavailable (D5)]
User objective / coordinator response / structured proposal / validation
[Draft objective] [Preview reply] [Preview proposed tasks]
[Open real task or approval, when backed by a returned ID]
```

| Component | Source | Data / method or event | Navigation |
| --- | --- | --- | --- |
| Conversation, objective composer and proposal preview | DUMMY D5 | `CoordinatorSource`; no coordinator message/start/reply RPC exists in M5-02. Session events alone are not a writable coordinator conversation | Preview fixture-only replies and proposals, not live sends |
| Structured validation and suggested retries/delivery | DUMMY D5 | Same interface; proposed tasks, dependencies, validation errors, outstanding decisions | Fixture preview; no autonomous approval or policy changes |
| Related live tasks / pending decisions | REAL | `tasks.list` and `approvals.list` joined locally by project/task IDs | S4/S6; never link a fixture ID into a REAL mutation |

The future coordinator remains a proposer. Engine validation, canonical IDs, lifecycle and approval enforcement stay in the daemon, as required by [the technical implementation plan](/Users/mckerracher.joshua/Code/agent-orch-worktrees/mckerracher-joshua-mclm-friendly-guide/docs/technical-implementation-plan.md), §§2, 11–12. An internal M3-01 implementation is not a frontend IPC contract.

## S6 — Diff review and approval (M5-04)

```text
[C1] Project / Task > Review                  [Pending decision selector]
Task / approval / candidate commit / action / status
+-------------------------+---------------------------------------------+
| File list from diff     | Scoped unified diff                         |
| [choose relative path]  | Empty / too large / unavailable state       |
+-------------------------+---------------------------------------------+
[Exact action parameters + verification evidence: PLACEHOLDER DATA D7]
[Accept] [Reject] [Request changes: PLACEHOLDER DATA D8]
[Delivery: explicit separate request / confirmed result / error]
```

| Component | Source | Data / method or event | Navigation |
| --- | --- | --- | --- |
| Pending decisions and basic candidate identity | REAL | `approvals.list`; task ID, approval ID, status and action's candidate/kind/digest as actually returned; `tasks.get` for task context | Select decision; back to S4/S2 |
| File selection and diff content | REAL | `workspaces.diff({task_id,workspace_id?,path?})` returns task ID, workspace ID and diff; derive file list from diff, narrow by safe relative path | Local file navigation; no editor or fabricated immutable-diff guarantee |
| Complete verification/action preview and pinned candidate snapshot | DUMMY D7 | `ReviewEvidenceSource`; current diff response is not a commit-pinned snapshot, approval digest is not readable action parameters, and `tasks.get` does not expose verification/workspace/artifact collections | Fixture-only complete preview; absent live evidence disables Accept and delivery, rather than treating a fixture as approval evidence |
| Accept / reject decision transport | REAL | `approvals.resolve({approval_id,decision,idempotency_key})`; Accept maps to `approve`, Reject to `reject` (the only supported decisions) | Refresh pending decisions/context after receipt; wait for daemon result, no optimistic success |
| Request-changes action and feedback | DUMMY D8 | `ChangeRequestSource`; no third decision or change-request endpoint exists | Label as preview, operate only within fixture dataset; do not send an invented `request_changes` decision |
| Authorized delivery request and result | REAL | `delivery.request` with task_id, optional workspace_id, approval_id, verification_run_id, action (`report_only`/`local_review`), optional target_ref, idempotency_key | Only enabled with trusted real evidence/IDs and authorization; show actual result; approval alone is not delivery |
| Conflicts, stale review and failed decision/diff/delivery | REAL | C1 outcomes; `Conflict`, `Forbidden`, `ResultTooLarge`, not-found, timeout, idempotency errors; reload approvals/task/diff | Keep prior preview stale; changed candidate/action requires fresh evidence and approval; never silently reuse previous acceptance |

D7 is a deliberate boundary: the UI can render a working fixture review and exercise decision transport against prepared M5-02 fixtures without claiming a safe end-to-end live approval flow. No fixture-supplied verification_run_id, workspace or digest authorizes a real delivery. M3-04 provides engine delivery, not the missing read-side API. PR automation remains deferred.

## DUMMY interface and fixture registry

All names here are **frontend boundary interfaces**, not new RPC methods. Implement one fixture-backed adapter per interface in the owning UI item; inject it at the screen/container boundary. A future daemon-backed adapter returns the same view model and replaces the fixture without a component/route change. Every view model carries source provenance, shown as **PLACEHOLDER DATA** beside the component (including loading, errors and mutation receipts). DUMMY buttons perform isolated fixture actions only. Tests must prove fixture IDs cannot reach REAL mutations; changing from REAL to fixture mode must be explicit, never an automatic fallback after an IPC failure.

Planned fixture root: `apps/desktop/src/fixtures/ui/`. Interface names are contracts to implement later, not source files added now. Each fixture includes success, empty, loading/error and unavailable scenarios where relevant.

| ID / component | Interface and minimum view-model contract | Fixture | Replaced by |
| --- | --- | --- | --- |
| D1 Agent detection/capabilities | `AgentCapabilitiesSource`: agent IDs, installed/available/unsupported/unknown, supported features, discovery time, source; no optimistic support inference | `agent-capabilities.json` | M5-02b: `agents.capabilities` and actual discovery |
| D2 Scheduler, launch and live execution effects | `TaskExecutionSource`: execution availability, task/attempt/session IDs, state, launch/cancel outcome and source; fixture launch/cancel stays isolated | `task-execution.json` | M5-02b: scheduler/daemon executor and transition production |
| D3 Restart recovery | `RecoveryStatusSource`: task/attempt, reconciliation phase, process/workspace/delivery checks, classification, reason, source | `recovery-status.json` | M5-02b: restart reconciliation before scheduling |
| D4 Retry | `TaskRetrySource`: retry availability/reason, previous and new attempt IDs, outcome, source | `task-retry.json` | M5-02b: `tasks.retry` |
| D5 Coordinator conversation/proposals | `CoordinatorSource`: project, messages, proposed tasks/dependencies, validation outcome, related real IDs only when returned, source; fixture reply/proposal actions | `coordinator.json` | New M5-02c: coordinator daemon bridge to M3-01 |
| D6 Journal reset/restart identity | `JournalIdentitySource`: store identity/epoch, cursor relationship (same/reset/unknown), source; unknown never means replay complete | `journal-identity.json` | M5-02b: known M5-02 D3 store-epoch/resume gap; agree contract before binding |
| D7 Complete review evidence | `ReviewEvidenceSource`: candidate/base identity, scoped snapshot, workspace identity, verification run/evidence, readable action/parameters/target and digest relationship, stale/missing state, source | `review-evidence.json` | New M5-02c: authorized read-side evidence contract and candidate binding |
| D8 Request changes | `ChangeRequestSource`: task/candidate, feedback, request outcome, resulting review state, source | `change-request.json` | New M5-02c: engine-backed request-changes contract |

Replacement acceptance for every row: source is visibly labelled while fixture-backed; real adapter can replace fixture with no UI change; fixture errors/empty states are reviewable; reconnects never promote fixture data into trusted state. M5-02b replaces D1–D4 and D6, not the missing D5/D7/D8 contracts. M5-05 reuses D2/D3/D6 interfaces for lifecycle previews; it does not define duplicate mocks or count them as native runtime acceptance.

## Ownership and acceptance boundaries

- M5-03: S1–S5 and C1 presentation; D1–D6. Accept UI rendering/navigation, real M5-02 reads/admission/cancel request/replay against the accepted contract, and isolated labelled fixture demos. It does not wait for M5-02b/M5-02c implementation or assert agents were launched/recovered.
- M5-04: S6 with C1 integration; D7–D8. Accept real diff/approve/reject request contracts plus labelled complete fixture review. Missing live evidence disables protected operations. Request-changes transport remains a fixture until M5-02c.
- M5-05: C1 native launch/reopen, window close/quit semantics and packaging of S1–S6. Build/package work uses the same adapters and need not wait for unavailable services. **Final completion still requires its existing M5-01b and M5-02b dependencies**, native Windows transport, and a real daemon task surviving client close/reopen; fixture survival cannot satisfy the milestone exit criterion. No new daemon methods for window lifecycle.
- Closing a window disconnects the client; explicit daemon shutdown is distinct. Authenticated reconnect must not imply task reconciliation completed. M5-05 must not advertise production coordinator/request-changes/evidence support while M5-02c is incomplete.

## Conflicts and dependency recommendations

1. The plan lists `tasks.retry` and `agents.capabilities` in §10, but the board assigns them to M5-02b. D1/D4 remain DUMMY even if an internal library can perform them.
2. M5-03 asks for detection, conversation and recovery without M5-02b dependencies. Preserve its dependency on M5-02; use D1–D6 for missing services. M5-04 likewise proceeds without M5-02c through D7/D8.
3. M5-02 `approvals.resolve` accepts only approve/reject. The requested Request changes UI needs D8 and a new owner. Complete candidate/action/evidence preview also has a read-side gap (D7), and coordinator RPC is unassigned (D5). New M5-02c tracks these gaps; it does not make up method names.
4. M5-02 known D3 lacks store identity/epoch for reset-safe persisted resume. Track D6 replacement in M5-02b; until then use explicit unknown/fresh baseline behavior. Do not claim data loss-free cross-store replay.
5. M5-05 has no existing details/acceptance fields and depends on M5-02b. Its plan milestone requires real tasks surviving UI reopen. Do not remove that edge or allow dummy lifecycle demos to pass it. Add details/acceptance for preview versus native completion.
6. Context claimed no frontend, but this checkout has a minimal heading-only frontend; current daemon crates are stubs despite board foundation acceptance on other revisions. Preserve existing candidate/evidence history. No acceptance criteria existed on the three UI items; this change adds `acceptance` arrays.
7. No existing status, priority or depends_on is changed. M5-02c is a newly proposed task at priority 10, depending on M5-02, M3-01 and M3-04 (which transitively includes approval and verification prerequisites). Recommend adding M5-02c as a **release gate** to M5-05 only if full live coordinator/request-changes/evidence support is required for the first package; that edge would raise M5-05 to priority 11, already its value. Keep M5-03/M5-04 free to implement labelled previews. Existing earlier-priority/runtime blocks in `important_context` still apply; this board edit is not worker dispatch or status acceptance.
