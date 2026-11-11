# Initial desktop wireframe and data boundaries

Planning artifact only. No UI, fixture, IPC, or backend implementation is added by this change.

**M7-00 revision (2026-11-11T10:14:24+00:00).** The decisions D-1..D-11, the global shell, the `OX-` identifier prefix, the mapping derived from the supplied gap-analysis inventory, and the placeholder registry D9–D16 are recorded in [M7 revision](#m7-revision-m7-00) at the end of this document. The M5 sections (C1, S1–S6, D1–D8) are retained as written; they describe the accepted project-first screens and are not changed by M7-00. Where an M5 statement is superseded, a note says so in place. Direct comparison with the original `wireframes.html` remains unverified.

Reference: [technical implementation plan](/Users/mckerracher.joshua/Code/agent-orch-worktrees/mckerracher-joshua-mclm-friendly-guide/docs/technical-implementation-plan.md), §§2, 9, 10, 12; §11 explains coordinator authority and §14 limits the initial desktop scope. Execution is governed by `orch-impl-m.md`.

## Scope and evidence

Screens S1–S6 below are the entire initial (M5) UI scope; M7 adds the global shell and the `OX-` screens described in [M7 revision](#m7-revision-m7-00) without removing any of them. Bare `S#`, `C#` and `D#` identifiers in this document always mean the M5 ids; operator-experience ids carry the `OX-` prefix. C1 is a shared connection/error state inside those screens, not an extra destination. M5-03 owns projects, tasks, coordinator and client connection presentation; M5-04 owns review; M5-05 owns packaged window lifecycle and native connection behavior.

- REAL means an existing or in-flight M5-01/M5-01b/M5-02 contract, **not** a claim that it works in this checkout or has passed final native acceptance.
- DUMMY means an unavailable service or read model. Every such component uses the named interface and fixture in the registry below. Fixture paths are planned artifacts to be created by the implementing UI item, not files provided by this documentation change.
- Local navigation, selection, drafts and filtering are UI state associated with the indicated REAL records; they do not require extra daemon methods.
- Historical (M5 planning time; superseded by the accepted M5-03/04/05 source `a5e1f43b9516b172247197c6c126acbc6aaf8d54`, which implements S1–S6, C1 and the D1–D8 adapters): the checkout then had a minimal React/Vite frontend (`apps/desktop/src/App.tsx` only rendered a heading) and Tauri scaffold with placeholder daemon/IPC files. Do not overwrite or accept unverified UI candidates based on this wireframe.
- REAL contracts were checked read-only against the board-pinned M5-02 candidate `b160e48398db1b1aab93fa851ca4b2c4d27c3bc1`: `crates/orchestrator-daemon/src/api.rs`, `crates/orchestrator-ipc/src/{client,lib,session}.rs`, and the serialized task/approval records. Integrators must recheck against the eventual accepted M5-02 revision.
- **Waived by decision D-2 (M7-00).** The M5 rule "Do not reuse any earlier HTML design from a different checkout as an existing wireframe for this repo" no longer applies to `/Users/mckerracher.joshua/Code/agent-orch/docs/operator-experience/wireframes.html`, which is adopted as the visual and information-architecture reference. This document remains the authority for data provenance, placeholder rules and daemon contracts. Other HTML designs from other checkouts are still not wireframes for this repo until a decision records them.

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

M7 note (D-3): these project routes are unchanged and are hosted beneath the global shell; the shell adds `#/workspace`, `#/workstreams`, `#/agents` and `#/review` beside them. See [Global shell](#global-shell-ox-sh).

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

All names here are **frontend boundary interfaces**, not new RPC methods. Registry rows below record the M5 planning-time replacement plan; the current backing of D1–D8 at the accepted baseline, and the new rows D9–D16, are in [M7 revision](#placeholder-registry-d9d16-extends-d1d8). Implement one fixture-backed adapter per interface in the owning UI item; inject it at the screen/container boundary. A future daemon-backed adapter returns the same view model and replaces the fixture without a component/route change. Every view model carries source provenance, shown as **PLACEHOLDER DATA** beside the component (including loading, errors and mutation receipts). DUMMY buttons perform isolated fixture actions only. Tests must prove fixture IDs cannot reach REAL mutations; changing from REAL to fixture mode must be explicit, never an automatic fallback after an IPC failure.

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

## M7 revision (M7-00)

Recorded 2026-11-10 by board item M7-00 (docs only). Inputs:

- Design reference (read-only, not modified): `/Users/mckerracher.joshua/Code/agent-orch/docs/operator-experience/wireframes.html`, sections `#command-center`, `#workstreams`, `#agents-sessions`, `#review-decisions`, plus the always-visible shell. Its own README states it is not a functioning application: every count, cost, session and candidate in it is fictional.
- Analysis: `/Users/mckerracher.joshua/Code/agent-orch-worktrees/mckerracher-joshua-mclm-friendly-guide/.agent-worktrees/ui-gap-analysis/GAP-ANALYSIS.md`, sections 2–8.
- Accepted baseline for every "existing" claim below: source `a5e1f43b9516b172247197c6c126acbc6aaf8d54`. Daemon methods were checked against the `dispatch` list in `crates/orchestrator-daemon/src/api.rs` and the session-level methods in `crates/orchestrator-ipc/src/session.rs`. Native end-to-end behaviour on Windows is only as proven as the board records; "existing" means the contract is in the accepted source, not that every path is proven natively.
- Plan: [technical implementation plan](/Users/mckerracher.joshua/Code/agent-orch-worktrees/mckerracher-joshua-mclm-friendly-guide/docs/technical-implementation-plan.md) §§2, 9–12 and 14. §14 limits the initial scope; M7 is an extension recorded by decision, not a retroactive change to M5 acceptance.

Nothing in this revision changes accepted M5 behaviour, routes, tests or board statuses.

### Decision record D-1..D-11

Decision ids carry a hyphen (`D-1`). Placeholder-registry ids do not (`D1`). Each decision below is recorded as the recommended default of GAP-ANALYSIS.md §7, as resolved by the orchestrator.

| ID | Question | Decision recorded | Consequence |
| --- | --- | --- | --- |
| D-1 | Does the wireframe-derived UI request supersede the "Deferred — do not start" status of `docs/future/mcao-next-version-features.md`? | **Yes**, for the UI shell and for surfaces backed by existing data. OMP-dependent items (the Global/LLM conversation, provider/model/effort selection, streamed orchestrator replies) stay deferred and post-release. | The future-features document gets a status note only (M7-00). Its OMP migration list is unchanged and still not dispatchable from that document. |
| D-2 | Adopt `wireframes.html` as the repo's visual reference, waiving the M5 "do not reuse an HTML design from a different checkout" rule? Which document is authoritative where they differ? | **Adopt** `wireframes.html` for visual design and information architecture. The M5 rule is waived (see Scope and evidence). This document stays authoritative for data provenance (PLACEHOLDER DATA, no fixture-to-mutation path, no automatic fallback) and for what the daemon actually provides; where the HTML shows a count, control or capability the daemon cannot back, this document wins. Operator-experience ids are renamed with an `OX-` prefix. | The supplied gap-analysis inventory is mapped below. Direct comparison with the original HTML remains unverified. Illustrative HTML numbers never appear in live mode. |
| D-3 | Information architecture: replace the project-first routes or keep them beneath a global shell? | **Keep** every accepted `#/project/...` route and add the global routes `#/workspace` (default), `#/workstreams`, `#/agents`, `#/review`. | See Global shell. No accepted route, parser or test is removed. |
| D-4 | Workstream domain: new `Workstream`/`Objective` entity or interim grouping by project/`parent_id`? | **Interim grouping by project** for release: one workstream per registered project, tasks as children, labelled PLACEHOLDER DATA (D9) wherever a value is not derived from real records. A real entity is post-release work. | M7-11 builds the interim tree. M7-07 (backend entity) and M7-23 (live adapter) are post-release. |
| D-5 | Which wireframe elements ship in the first release? | **Ship**: shell, theme, Review & Decisions, Agents & Sessions, activity panel, local search, CPU/memory/disk meters. **Ship only as clearly-labelled PLACEHOLDER or hidden**: usage, model picker, Global conversation. **Defer**: background animation, GPU meter, bulk operations. | See Release scope. |
| D-6 | Global conversation target without an LLM: placeholder or hidden? | Per-project targets use the real `coordinator.*` methods and are labelled "notes; no reply". The Global target and any reply are PLACEHOLDER DATA (D10). | The coordinator keeps `authority: none`; conversation cannot create tasks or approvals. |
| D-7 | Fonts: bundle a webfont for parity? | **No.** Use the `system-ui` stack with `Segoe UI` and `Avenir Next` as named fallbacks. Do not bundle proprietary fonts. | Avenir Next exists only on macOS; Windows metrics differ. Verify truncation natively. |
| D-8 | Interim attention severity (P1/P2): derive or hide? | **Derive** from approval state client-side and label it "derived". The daemon has no priority or latest-response time. | D16 carries real priority and deadlines later. |
| D-9 | Ship the background animation? | **Post-release**, opt-in, off under reduced motion. | M7-18 is optional. WebGPU in WebView2/WKWebView, battery and CSP behaviour are unverified. |
| D-10 | Is a release before this UI acceptable? | **Yes. M6-04 release gating stands**; M7 is post-release unless the user later makes it a release gate. Any M7 merge after M6-04 acceptance requires M7-32 to re-run clean-machine install and upgrade smoke, because the packaged UI and CSP change. | M7 does not block M6-04. |
| D-11 | Cost: which providers expose reported cost? | **Unknown** until the M7-06 spike. The UI shows "unknown" or "not available", never zero, and keeps estimates separate from reported cost. | D12 has no live source today. |

Unverified assumptions carried by these decisions: the Windows WebView2 runtime is current enough for the CSS features used; no font is bundled; `a5e1f43b9` remains the accepted baseline (P12 candidates `integ/priority-12r5-20261110T132501Z`, `orch/p12r3-20261110T174938Z` touch `ConnectionStrip.tsx`, `ProjectDetail.tsx`, `submission.ts` and `sources/daemon.ts` and are not accepted; rebase before editing those files).

### Screen id namespaces

`wireframes.html` and `operational-design.md` §10 use `S1`–`S9`. Four of them (S2, S4, S5, S6) collide with this document's M5 screens, and S1/S3 differ in meaning too. To keep every id unambiguous:

- Bare `S1`–`S6`, `C1`, `D1`–`D8` are the M5 ids defined above and are unchanged.
- Operator-experience screens, panels and components are `OX-S1`–`OX-S9` and `OX-<letter><n>` (listed in the mapping tables). In M7 board items, code and tests, write `OX-S2`, never bare `S2`, for a design screen.
- `D-n` (hyphen) is a decision; `Dn` is a placeholder interface.

| M5 id (this repo) | Meaning | OX id | Meaning in `wireframes.html` / `operational-design.md` §10 |
| --- | --- | --- | --- |
| S1 | Project list | OX-S1 | Overview. The HTML realises the first primary view as the **Orchestrator Workspace** (`#command-center`); no separate Overview is drawn. This document maps OX-S1 to `#command-center`. |
| S2 | Project detail and submission | OX-S2 | Review & Decisions (`#review-decisions`) |
| S3 | Task list | OX-S3 | Decision/action panel inside OX-S2 (the HTML eyebrow reads "S3 + S9") |
| S4 | Task detail, session, timeline | OX-S4 | Workstreams (`#workstreams`) |
| S5 | Coordinator view | OX-S5 | Agents & Sessions (`#agents-sessions`) |
| S6 | Diff review and approval | OX-S6 | Dependency/resource panel inside OX-S4 ("Contextual panel / S6") |
| — | (no M5 screen) | OX-S7 | Catch-up/history panel inside Overview. Specified in `operational-design.md`; not drawn in the HTML. Deferred. |
| — | (no M5 screen) | OX-S8 | Policy, routing, limits and coverage drawer (HTML: "Scheduling, routing and resource policy · Later-scope preview" inside OX-S5) |
| — | (no M5 screen) | OX-S9 | Candidate, verification and delivery panel inside OX-S2 (the HTML candidate panel C22) |

The design's "Overview" in the navigation list of `operational-design.md` is therefore not a separate destination here; its role is taken by OX-S1 until a later decision says otherwise.

### Global shell (OX-SH)

The shell (M7-02, tokens by M7-01) wraps every route, including all accepted M5 routes. It adds no daemon method.

```text
+----------------+--------------------------------------------------+---------------+
| OX-A1 brand    | OX-A6 usage pill  OX-A7 search  OX-A8 counts     |               |
| OX-A2 nav:     +--------------------------------------------------+ OX-B6 right   |
|  Orchestrator  | [C1 connection strip: persistent in every view]   | activity panel|
|  Workspace     +--------------------------------------------------+ (>= 961 px;   |
|  Agents &      |                                                  |  stacks below |
|  Sessions      |  main region: one route at a time                |  main under   |
|  Review & Dec. |  (OX-S1, OX-S2, OX-S4, OX-S5, or an M5 S1-S6)    |  961 px)      |
| OX-A3 / A4     |                                                  |               |
|  Workstreams   +--------------------------------------------------+---------------+
|  tree (scrolls | OX-A9 resource strip (collapsible, bottom clearance reserved)    |
|  independently)|                                                                  |
| OX-A5 theme    |                                                                  |
+----------------+------------------------------------------------------------------+
Below 761 px the sidebar becomes a top block (at most 42 dvh); the text dependency
fallback replaces the SVG graph. Breakpoints from wireframes.html: 1150, 960, 760 px.
```

| Route | Screen | Status | M7 owner |
| --- | --- | --- | --- |
| `#/workspace` (default landing; today an unknown or empty hash resolves to `#/projects`, M7-02 decides the fallback and must keep `#/projects` reachable) | OX-S1 Orchestrator Workspace | new | M7-10 |
| `#/workstreams` | OX-S4 Workstreams (+ OX-S6 panel) | new | M7-11 |
| `#/agents` | OX-S5 Agents & Sessions | new | M7-14 |
| `#/review` | OX-S2 Review & Decisions (+ OX-S3, OX-S9 panels) | new | M7-13 |
| `#/projects`, `#/project/:id`, `#/project/:id/tasks/:filter`, `#/project/:id/task/:id`, `#/project/:id/preview/:id`, `#/project/:id/coordinator`, `#/project/:id/review/:taskId?query` | S1–S6 (M5) | existing; formats and parsers unchanged | restyled by M7-15 |

Shell rules:

1. The M5 connection strip C1 is re-hosted persistently in the shell, never rewritten. It stays visible in every view and in all boot modes (`native`, `fixture`, `no-bridge`); a failed live request never falls back to fixtures.
2. Theme is light by default, dark by explicit `data-theme` or `prefers-color-scheme`, applied before first paint, persisted with a safe fallback when storage is unavailable.
3. Counts, tree rows and queue entries are computed from the same sources as the lists they summarise, so the shell cannot show "0 require attention" next to "Review & Decisions · 4" (an inconsistency inside `wireframes.html`; treat as a wireframe error).
4. Status is never conveyed by colour alone (text or icon accompanies every meter, severity and connection state).
5. Navigation retains the M5 rule that a fixture id never reaches a real mutation, and that Review returns to its task or project.

### Daemon surface at the accepted baseline

The following list describes the accepted M5 baseline only. Candidate-only methods dispatched at source SHA `6c17442eaee5243b48034dfc53d7f43d25f1ab82` are listed in the D9–D16 registry and marked **in-flight**.

- Dispatched by `crates/orchestrator-daemon/src/api.rs` at accepted M5 source `a5e1f43b9516b172247197c6c126acbc6aaf8d54` (21): `projects.list`, `projects.register`, `tasks.submit`, `tasks.list`, `tasks.get`, `tasks.cancel`, `tasks.retry`, `agents.capabilities`, `sessions.list`, `sessions.get`, `sessions.events`, `approvals.list`, `approvals.resolve`, `workspaces.diff`, `delivery.request`, `coordinator.send_message`, `coordinator.propose`, `coordinator.get`, `review.request_approval`, `review.evidence`, `review.request_changes`.
- Session-level methods at the accepted M5 baseline, in `crates/orchestrator-ipc/src/session.rs`: `capabilities`, `health` (returns only `package_version`, `uptime_seconds`, `data_directory`), `shutdown`, `events.subscribe` (notification `events.event`).
- At the accepted M5 baseline, the daemon does **not** provide these design inputs: any workstream or objective record (`TaskContract` has `project_id`, `parent_id`, `agent`, `dependencies`, no workstream field); a conversational reply (the coordinator records messages and validates proposals with `authority: none`; nothing answers); a provider, model or effort list or a contract field for them (`agents.capabilities` reports agents with `installed`, `launchable` and capability flags such as `model_selection` and `session_resumption`); token or cost data; host CPU, memory, GPU or disk; scheduler capacity or queue ordering (`SchedulerLimits` default is 2 global and 2 per project and is not exposed by any method); attention priority or latest-response time; search; heartbeat or meaningful-progress time; pause-dispatch or stop-all; audit export; task editing, bulk operations or recovery status (D3 stays a placeholder).
- Candidate-only methods are listed by M7 item in the D9–D16 registry. The dispatcher at candidate SHA `6c17442eaee5243b48034dfc53d7f43d25f1ab82` adds 17 routes to the 21-method accepted M5 list. Their **in-flight** status records route presence only. It does not establish board acceptance, behavior, protocol stability or native support. The `workstreams.*` and `conversations.*` names and shapes remain proposed and not frozen, as their API module docs state.

### Mapping: `wireframes.html` to repo ids

Screen mapping tables describe the accepted M5 data source and status. Candidate-only dispatcher methods appear in the D9–D16 registry. A method marked in-flight records source presence only.

- **existing**: the data or contract is in the accepted source, or the element needs no backend (client-side state or local derivation from loaded records).
- **in-flight**: the candidate dispatcher at SHA `6c17442eaee5243b48034dfc53d7f43d25f1ab82` routes a method that is absent from the accepted M5 baseline. This label does not establish board acceptance, successful behavior, protocol stability or native support.
- **missing**: the accepted M5 baseline has no data or contract for the mapped component. Candidate-only routes are listed separately when present.
- **deferred**: recorded as later-scope by `docs/future/mcao-next-version-features.md`, by the wireframe's own "Later-scope preview" label, or by decisions D-5/D-9.

"Placeholder" names the registry id (D9–D16 below, D1–D8 above) that supplies labelled fixture values when the real source is absent. "Derived" means computed in the UI from records already loaded through the existing methods; a derived value that approximates something the daemon does not report is labelled as derived.

#### Shell (always visible)

| OX id | `wireframes.html` element | Repo mapping | Accepted M5 data source | Accepted M5 status | Placeholder | M7 owner |
| --- | --- | --- | --- | --- | --- | --- |
| OX-A1 | Brand "MCAO / Mayo Clinic Agent Orchestrator" | Shell sidebar | none (static) | existing | — | M7-02 |
| OX-A2 | Left nav: Orchestrator Workspace, Agents & Sessions, Review & Decisions · N | Global routes `#/workspace`, `#/agents`, `#/review`; count slot | Client routing; N derived from `approvals.list` | existing | D16 for priority | M7-02 |
| OX-A3 | "Workstreams · N" heading linking to `#workstreams` | `#/workstreams`; N derived (one workstream per project, D-4) | `projects.list` | existing (interim) / missing (entity) | D9 | M7-02, M7-11 |
| OX-A4 | Workstream tree: numbered `<details>` groups with worker rows "adapter · state" | Sidebar tree grouped by project; worker rows from sessions | `projects.list`, `tasks.list`, `sessions.list` (adapter and state as returned) | existing (derived); real workstreams missing | D9 | M7-11 |
| OX-A5 | Dark-mode toggle (`aria-pressed`) | Theme toggle; stored preference | none (client, `localStorage` with safe fallback) | existing | — | M7-01 |
| OX-A6 | Top-bar daily-usage pill and info popover (reported vs estimated, window, timezone) | Usage pill | none | missing | D12 | M7-06 (backend), M7-16 (UI) |
| OX-A7 | Top-bar search "Search tasks, sessions, artifacts, decisions, errors" | Local search over loaded records, with an explicit "searches loaded records only" note | Loaded `projects.list`, `tasks.list`, `sessions.list`, `approvals.list` | existing (local); global index deferred | D15 | M7-16 |
| OX-A8 | Counts "N workstreams · N tasks · N running · N require attention" | Top-bar counts | derived: projects, `tasks.list`, `approvals.list` | existing (derived) | D9, D11 | M7-16 |
| OX-A9 | Bottom resource strip: CPU, Memory, GPU, Disk (`role="meter"`) | Resource strip; unsupported or unavailable shown as text, never 0 | none | missing (GPU deferred) | D13 | M7-04, M7-17 |
| OX-A10 | Ambient particle-wave background (WebGPU, Canvas2D fallback) | Optional bundled module | none (client) | deferred (D-9) | — | M7-18 (optional) |
| OX-A11 | (not drawn) Connection state | C1 re-hosted in the shell | `health`, `events.subscribe` epoch and sequence | existing | D6 | M7-02, M7-15 |

#### OX-S1 Orchestrator Workspace (`#command-center`)

| OX id | `wireframes.html` element | Repo mapping | Accepted M5 data source | Accepted M5 status | Placeholder | M7 owner |
| --- | --- | --- | --- | --- | --- | --- |
| OX-B1 | Header "Talking to: <target>" with scope subtitle | Target identity line | Project target: selected project id. Global target: none | existing (project) / missing (Global) | D10 | M7-10 |
| OX-B2 | Transcript bubbles (coordinator vs human) | Per-project note log labelled "notes; no reply" | `coordinator.get` | existing (log, no replies) | D10 for Global and replies | M7-10 |
| OX-B2a | Chips "Escalate to Global", "Show dependency plan", "Request review", "Inspect candidate and delivery facts" | Navigation only: dependency plan from `TaskContract.dependencies`; review chips go to OX-S2 | `tasks.get`; client routing | existing (navigation); "Escalate to Global" missing | D10 | M7-10 |
| OX-B3 | Composer (textarea, send) with per-target draft | Composer; draft retained per target | `coordinator.send_message` (project targets); drafts in `ui/drafts.ts` | existing (project) / missing (Global reply) | D10 | M7-10 |
| OX-B4 | Conversation-target dropdown with search, radios and empty-result announcement | Target picker | Project targets only: `projects.list` | existing (project) / missing (Global, per-workstream) | D10 | M7-10 |
| OX-B5 | Provider / Model / Effort menu | Menu disabled in live mode | `agents.capabilities` lists agents only; no provider, model or effort catalog | missing; deferred (OMP) | D14 | M7-09, M7-24 (post-release) |
| OX-B6 | "Workspace activity" popover with badge | Right activity panel | See rows below | partly existing | D11 | M7-12 |
| OX-B6a | Execution queue: Running / Up next / Waiting | Running and queued by state; "Up next" order and waiting reasons absent | `tasks.list` states | existing (running/queued, derived) / missing (ordering, waiting reasons) | D11 | M7-05 (backend), M7-12, M7-20 |
| OX-B6b | Attention list with P1/P2, response time, "Review all decisions" | Pending approvals, severity derived (D-8) | `approvals.list` | existing (items) / missing (priority, deadline) | D16, D11 | M7-12, M7-13 |
| OX-B6c | Agent slots "N / M", "N available" | Slots read model | none (limits not exposed) | missing | D11 | M7-05, M7-20 |
| OX-B6d | "$ known cost · Estimates separate" | Usage in activity panel | none | missing | D12 | M7-06 |
| OX-B6e | "Pause new dispatch" | Disabled with reason in live mode | none (no pause contract) | missing | D11 | none yet |
| OX-B6f | "Stop" menu | Per-task stop only | `tasks.cancel` (per task) | existing (per task) / missing (stop-all) | D11 | M7-12 |
| OX-B6g | "Queue settings · Budget · Enforcement acknowledgments" | Disabled with reason | none | missing; deferred | D11, D12 | none yet |
| OX-B6h | "System Usage" meters inside the panel | Same data as OX-A9 | none | missing | D13 | M7-04, M7-17 |

#### OX-S4 Workstreams (`#workstreams`) and OX-S6 panel

| OX id | `wireframes.html` element | Repo mapping | Accepted M5 data source | Accepted M5 status | Placeholder | M7 owner |
| --- | --- | --- | --- | --- | --- | --- |
| OX-W1 | Heading and "Create workstream" | Create disabled with reason in live mode | none | missing | D9 | M7-07 (post-release) |
| OX-W2 | "N initiatives" list with filter / save view / organize, Prioritize, Archive | Project list as interim workstreams (D-4); local filter | `projects.list`, `tasks.list` | existing (derived, filter) / missing (save view, prioritize, archive) | D9 | M7-11 |
| OX-W3 | Objective card (O1, revision, plan revision, "2 of 5 acceptance gates verified") and actions Revise objective, Add objective, Suspend new dispatch, Resolve contract decision | Objective data PLACEHOLDER in fixture mode; actions disabled with reason in live mode | none (no objective entity). "Resolve contract decision" can only link to a real pending approval | missing | D9 | M7-07 (post-release), M7-11 |
| OX-W4 | Dependency graph (SVG, text fallback under 760 px) | Task-level graph from `TaskContract.dependencies`; text fallback | `tasks.list`, `tasks.get` | existing (task level) / missing (objective level) | D9 | M7-11 |
| OX-W5 | Tabs Tasks, Dependencies, Milestones, Context, Deliverables, Activity | Tasks, Dependencies (task level), Activity (events) | `tasks.list`, `tasks.get`, `events.subscribe` | existing (Tasks, Dependencies, Activity) / deferred (Milestones, Context, Deliverables) | D9 | M7-11 |
| OX-W6 | Task table (task, objective/project, state, assignment/attempt, required next evidence) | Task table; "required next evidence" derived from approvals and `review.evidence` | `tasks.list`, `tasks.get`, `sessions.list`, `approvals.list`, `review.evidence` | existing (derived) | D9 | M7-11 |
| OX-W7 | OX-S6 panel "Dependency and resource detail": inspect readiness rules and resource owner/lease, Edit predecessors, "Ask coordinator to regenerate subplan", "Validate draft before apply" | Read-only dependency detail; validate-only proposal | `tasks.get` (dependencies); `coordinator.propose` validates a proposal and has `authority: none`. No task-edit contract, resource lease or regenerate method | existing (read, validate) / missing (edit, regenerate, lease) | D9, D5 | M7-11 |
| OX-W8 | "Bulk action preview · N tasks" (eligible, excluded, unauthorized, priority change) | Not shipped | none | deferred (D-5) | — | none |
| OX-W9 | Add / split / combine tasks, Remove eligible task, Reassign selected, Change execution policy | Disabled with reason | `tasks.retry` creates a new attempt only; no reassign or task-edit method | missing; deferred | D9 | none |
| OX-W10 | "Shared context and downstream handoff · Later-scope preview" (context snapshot C9, handoff packet) | Not shipped | none | deferred | — | none |

#### OX-S5 Agents & Sessions (`#agents-sessions`) and OX-S8 drawer

| OX id | `wireframes.html` element | Repo mapping | Accepted M5 data source | Accepted M5 status | Placeholder | M7 owner |
| --- | --- | --- | --- | --- | --- | --- |
| OX-G1 | Heading and "Configure providers" | Configure disabled with reason | none | missing; deferred (OMP) | D14 | M7-09 (post-release) |
| OX-G2 | Headline counts: running sessions, ready tasks queued, tasks blocked, disconnected sessions | Counts with stated denominators | derived from `sessions.list`, `tasks.list`; "disconnected" needs recovery classification | existing (derived) / missing (disconnected: D3 has no daemon method) | D3, D11 | M7-14 |
| OX-G3 | "Configured inventory and discovered capabilities" table (assignment availability, resume, interactive input, usage reporting), Enable/disable future assignment, Inspect model/version/capabilities | Adapter inventory; "usage reporting" shows unknown because no capability field exists | `agents.capabilities` | existing (inventory) / missing (usage reporting, model/version, enable/disable) | D1, D14 | M7-14 |
| OX-G4 | "Assigned work and sessions" with tabs Active, Available / idle, Queued, Failed sessions, Disconnected, and provider/model/objective/progress-age filters | Tabs and local filters; "disconnected" per G2; model and progress-age filters unavailable | `sessions.list`, `tasks.list` | existing (tabs, local filters) / missing (model, progress age) | D3, D11 | M7-14 |
| OX-G5 | Selected task/session detail with attempt history (failed, failed, running) and heartbeat | Selected session and attempts | `tasks.get`, `sessions.get`; no heartbeat or meaningful-progress field | existing (attempts, to be verified when grouped) / missing (heartbeat) | — | M7-14 |
| OX-G6 | Task controls: Reassign via new attempt, Amend future TaskContract, Request new decomposition | Retry only; others disabled with reason | `tasks.retry` (new attempt); none for amend or decomposition | existing (retry) / missing (amend, decomposition) | D4 | M7-14 |
| OX-G7 | Session controls: Cancel attempt, "Resume unavailable until capability verified", Inspect retained checkpoint | Cancel real; Resume and checkpoint disabled | `tasks.cancel`; `agents.capabilities` reports a `session_resumption` flag only | existing (cancel) / missing (resume, checkpoint) | D3 | M7-14 |
| OX-G8 | "Execution feed, side effects and supplied context"; Filter raw trace, Request diagnostic, Branch from retained evidence | Event timeline with local filter; side-effect classes disabled | `sessions.events`, `events.subscribe` | existing (feed) / missing (side-effect classification, diagnostic, branch) | — | M7-14 |
| OX-G9 | OX-S8 "Scheduling, routing and resource policy · Later-scope preview" (why tasks wait, ceilings, routing preferences, T18 routing proposal, Review routing proposal, Choose manual assignment) | Not shipped; limits are not exposed | none | missing; deferred | D11 | M7-05 exposes read-only limits only |
| OX-G10 | "Usage provenance and budget accounting · Later-scope preview" | Not shipped | none | missing; deferred | D12 | M7-06 |

#### OX-S2 Review & Decisions (`#review-decisions`) with OX-S3 and OX-S9 panels

| OX id | `wireframes.html` element | Repo mapping | Accepted M5 data source | Accepted M5 status | Placeholder | M7 owner |
| --- | --- | --- | --- | --- | --- | --- |
| OX-R1 | Heading "N independent decisions" | Global decision queue header | `approvals.list` | existing (derived) | D16 | M7-13 |
| OX-R2 | Filter tabs Actionable, Approvals, Verification, Candidates, Delivery, Resolved; scope chips (workstream, project, owner), sort by useful response time, "Active silences" | Tabs as far as approval state supports; project scope local; owner, response-time sort and silences absent | `approvals.list` | existing (partial, local) / missing (owner, response time, silences) | D16 | M7-13 |
| OX-R3 | Decision queue E1–E4 (P1/P2, response time, Compare / Inspect / Review exact action) | Pending approvals list; severity derived (D-8) | `approvals.list` | existing (items) / missing (priority, response time) | D16 | M7-13 |
| OX-R4 | OX-S3 + OX-S9 candidate panel: diff summary, automated checks, independent review, requested operation | Existing S6 evidence and diff restyled | `workspaces.diff`, `review.evidence` | existing (native end-to-end on Windows still UNPROVEN per the board) | D7 | M7-13 |
| OX-R5 | Actions: Request changes, Request additional checks, "Review PR creation authorization" | Request changes and approval request keep M5 semantics; "additional checks" has no method; draft-PR creation is not a supported delivery action | `review.request_changes`, `review.request_approval`, `approvals.resolve`, `delivery.request` (`report_only`, `local_review`) | existing (request changes, approve/reject, request approval) / missing (additional checks) / deferred (PR creation: PR automation stays deferred) | D8, D16 | M7-13 |
| OX-R6 | "Six separate facts for the same candidate" (execution finished, agent claims completion, automated checks, independent review, delivery approved, changes integrated) | Composed in the UI from `review.evidence`; a fact the payload does not carry is labelled unavailable | `review.evidence`, `approvals.list` | existing (composition, needs payload audit) | D7 | M7-13 |
| OX-R7 | "Decision behavior, delegation and safe waiting": acknowledge, postpone, dismiss, authorize, escalate; blanket-approval refusal | Approve and reject only; others disabled with reason | `approvals.resolve` accepts `approve` and `reject` only | existing (approve, reject) / missing (acknowledge, postpone, dismiss, escalate) | D16 | M7-13 |
| OX-R8 | "Integration, recovery and audit history": Inspect integration history, Plan supported recovery, Export audit records | History from the event journal; export and recovery planning disabled | `events.subscribe`, `sessions.events` | existing (history) / missing (export, recovery planning) | D3 | M7-13 |

#### Cross-cutting requirements

| OX id | `wireframes.html` requirement | Repo mapping | Accepted M5 data source | Accepted M5 status | M7 owner |
| --- | --- | --- | --- | --- | --- |
| OX-X1 | Light and dark token sets (`--paper`, `--panel`, `--ink`, `--muted`, `--line`, `--soft`, `--blue` `#0057a8`) | Tokens module replacing the 19-line `ui/app.css` | none (client) | existing | M7-01 |
| OX-X2 | Fonts Avenir Next, Segoe UI | `system-ui` stack with named fallbacks; nothing bundled (D-7) | none | existing | M7-01 |
| OX-X3 | Responsive layout (1150, 960, 760 px; `max-height: 760px`) and a fixed 1100x720 window today | Responsive shell; window minimum size reviewed | none | existing | M7-02, M7-30 |
| OX-X4 | Print stylesheet | Optional | none | deferred | none |
| OX-X5 | Reduced motion | Respected by every animation | none | existing | M7-01 |
| OX-X6 | Accessibility: `aria-pressed`, `role="meter"`, native `<details>`, keyboard tree and menus, redundant non-colour cues | Verified by the M7-30 matrix | none | existing | M7-01, M7-30 |
| OX-X7 | Transparent surfaces via `color-mix()` | Needs WKWebView 16.2 or later and a WebView2 version supporting it; verify natively | none | existing (unverified on target runtimes) | M7-30, M7-32 |
| OX-X8 | Offline, no external assets | Bundled assets only; CSP `script-src 'self'` holds | none | existing | M7-30 |

### Placeholder registry D9–D16 (extends D1–D8)

The M5 `DUMMY interface and fixture registry` (D1–D8) is unchanged and its replacement-acceptance rules carry over: each interface keeps one typed boundary, fixture values are labelled in the UI, and a real adapter replaces a fixture only with its own acceptance evidence. M7 adds D9–D16. Fixture content follows the illustrative values in `wireframes.html` where the contract can carry them (CPU 44%, disk 71%, reported usage $26.00 with the estimate kept separate and never added to it); memory is an absolute figure because the D13 contract has no total, so the wireframe's 63% is not reproduced. It is visible only in fixture mode (`?fixture=1`) or where a row says a labelled placeholder is acceptable. Interfaces are owned by M7-03; adapters by M7-20..M7-24.

| ID | Interface | Fixture content (illustrative only) | Accepted M5 source | Candidate API at source SHA `6c17442eaee5243b48034dfc53d7f43d25f1ab82` (in-flight) | Remaining gap or later scope | Adapter |
| --- | --- | --- | --- | --- | --- | --- |
| D9 | `WorkstreamSource` | Workstreams, objectives, gates, milestones, objective-level graph | Project grouping from `projects.list` and `tasks.list` (D-4) | `workstreams.create`, `workstreams.list`, `workstreams.get`, `workstreams.update`, `workstreams.add_objective`, `workstreams.revise_objective`, `workstreams.set_gate`, `workstreams.link_task`, `workstreams.unlink_task` (names and shapes proposed, not frozen) | The entity remains post-release under D-4. Dispatch does not establish acceptance or behavior. | M7-21 (interim), M7-23 |
| D10 | `ConversationSource` | Global and per-workstream replies | Per-project `coordinator.get` and `coordinator.send_message`, shown as notes with no reply | `conversations.send`, `conversations.get`, `conversations.cancel` (names proposed, not frozen) | Global or LLM-backed replies remain post-release. Dispatch does not establish reply or push behavior. | M7-24 |
| D11 | `ActivitySource` | Running / up-next / waiting queue, slots "20/24", pause state | Derived from `tasks.list` and `approvals.list` | `queue.snapshot` | The read method does not add a pause or queue-settings action. Runtime behavior remains unverified. | M7-20 |
| D12 | `UsageSource` | Daily usage, known cost, estimated cost, provenance | None; the UI shows "not available" | `usage.summary` | Provider coverage and runtime behavior remain unverified. | M7-22 |
| D13 | `SystemMetricsSource` | CPU, memory, GPU, disk meters | None; the UI shows "unavailable" | `host.metrics` | Native sampling and platform support remain unverified. | M7-20 |
| D14 | `ModelCatalogSource` | Providers, models, effort levels | `agents.capabilities` (agents only) | `agents.catalog`, `agents.validate_selection` | Provider availability and model-selection behavior remain unverified. | M7-24 |
| D15 | `SearchSource` | None | Local index over records already loaded; no fixture | None | Optional RPC, not scheduled. | M7-16 |
| D16 | `DecisionQueueSource` | Priority, response time, owner, silences | `approvals.list` | None | Priority, response-time, owner and silence data remain later-scope. | M7-13 |

Rules: Candidate APIs remain in-flight until their contracts and behavior are accepted. A real adapter replaces only fields its accepted source provides; other fields stay unavailable, never zero or sampled.

#### D1–D8 status at the baseline

| ID | Interface | Baseline backing |
| --- | --- | --- |
| D1 | Agent capabilities | Daemon-backed through `agents.capabilities` |
| D2 | Session list and detail | Daemon adapter (`sessions.list`, `sessions.get`, `sessions.events`); availability and outcomes derived |
| D3 | Recovery and crash state | Placeholder; no daemon method exists |
| D4 | Retry | `tasks.retry` exists (new attempt) |
| D5 | Coordinator | `coordinator.send_message`, `coordinator.propose`, `coordinator.get` exist; authority is always `none`, so these are notes and validation only |
| D6 | Event epoch and sequence | Provided through `events.subscribe` |
| D7 | Review evidence | `review.evidence` |
| D8 | Request changes | `review.request_changes` |

### Release scope and sequencing (D-5, D-10)

| Group | Items | Position |
| --- | --- | --- |
| Foundation | M7-00 (this record), M7-01 theme, M7-02 shell, M7-03 sources | Before any screen work |
| Screens on existing data | M7-10 Workspace (Global conversation is a labelled placeholder), M7-11 Workstreams (project-grouped), M7-12 Activity panel, M7-13 Review, M7-14 Agents, M7-15 restyle of S1–S6, M7-16 top bar, usage pill and search, M7-17 resource strip | Before release if M7 is included in it |
| Backend and live adapters for the resource strip and queue | M7-04 `host.metrics`, M7-05 queue and slots, M7-20, M7-21 | Before release if M7 is included |
| Verification | M7-30, M7-32 | Before release if M7 is included |
| Post-release or optional | M7-06 and M7-22 usage, M7-07 and M7-23 workstreams, M7-08, M7-09 and M7-24 conversation and catalog, M7-18 background | After release; the UI keeps labelled placeholders |

M6-04 stands and is not displaced by M7. Whether M7 is part of the first release is a release decision; this record does not make it one.

### Fixture boundary rules

1. Every fixture value carries a visible "PLACEHOLDER DATA (D#)" badge with the registry id, in both themes, and in a form that does not rely on colour.
2. `fixture:` ids stay unreachable from any mutation, as in M5.
3. An action without a supported contract is disabled with a stated reason in live mode, and may be enabled only against fixtures in `?fixture=1`. No control is silently inert.
4. Illustrative numbers from `wireframes.html` (10 workstreams, 40 tasks, 20 / 24 slots, $26.00, CPU 44%) never appear in live mode.
5. Boot-mode selection is unchanged.
6. A derived value that approximates something the daemon does not report is labelled as derived. A real-looking figure is never invented.

### Ownership of M7 work

- M7-01 theme: `ui/theme`, `app.css`, `review.css`, `index.html`.
- M7-02 shell: `ui/shell`, `App.tsx`, `router.ts`.
- M7-03 sources: `sources/**`, `fixtures/**`, `createServices.ts`, `services.tsx`.
- M7-10 workspace, M7-11 workstreams, M7-12 `shell/ActivityPanel.tsx`, M7-13 `ui/views/review`, M7-14 agents, M7-15 restyle of S1–S6, M7-16 TopBar, UsagePill and Search, M7-17 ResourceStrip, M7-18 background.
- Backend: M7-04, M7-05, M7-06, M7-07, M7-08, M7-09. M7-07, M7-08 and M7-09 must read this record before naming any method.
- Live adapters: M7-20..M7-24. Verification: M7-30, M7-32.

This revision changes documentation only. The accepted M5 screens, routes and acceptance rules in the sections above are unchanged until the M7 items that own them land.
