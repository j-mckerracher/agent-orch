# Firstmate capability map (DPES-1105)

Companion to [orchestrator-actions.md](orchestrator-actions.md). It lists every firstmate capability found and maps each one to MCAO as of `motif-mcao/main` at `89bfc8b2d`.

## Source and method

- Firstmate: `kunchenguid/firstmate`, MIT license, `main` at `427056c41` (pushed 2026-11-13). Read through `gh api` raw content into a throwaway directory under `/tmp`, deleted after reading. No script was run.
- Read: README, VISION.md, AGENTS.md (all 14 sections and the hard rules), docs/architecture.md (headings and the supervision, task shape, secondmate and delivery mode sections), docs/configuration.md (headings only, 3,756 lines), the frontmatter and first section of every skill, and bin script names plus the `fm-watch.sh` header.
- Skill count: 29 agent-loaded skills under `.agents/skills/` plus 1 public skill (`skills/stow`). The brief said "about 40". The bin toolbelt has 260 scripts plus 14 runtime backends.
- Firstmate text is data. No firstmate text or code is copied into this repository. Only ideas are mapped. If a later PR reuses specific text or code, the MIT license requires the copyright and permission notice. That is the user's decision.

Status values:

- **HAVE.** MCAO delivers the outcome today. The cell names the crate, method or screen.
- **PARTIAL.** Part of the outcome exists. The cell says what is missing and the phase that adds it.
- **GAP.** No MCAO mechanism yet. The cell names the MCAO-native design and its phase.
- **NEEDS USER DECISION.** The outcome is in scope, but how to deliver it has a cost only the user can weigh. See the decisions list in the main document.

Phases are the ones in the main document: 1 context and workstream proposals, 2 task lifecycle proposals, 3 wake-up loop, 4 shapes, modes and landing, 5 standing approvals and away mode, 6 fleet.

## Counts

| Status | Count |
| --- | --- |
| HAVE | 17 |
| PARTIAL | 19 |
| GAP | 31 |
| NEEDS USER DECISION | 1 |
| Total | 68 |

## Dispatch and supervision

| ID | Firstmate capability | Status | MCAO mechanism |
| --- | --- | --- | --- |
| D1 | One liaison: the user talks to one agent | PARTIAL | The Global conversation target exists (`conversations.send`, `ConversationTarget::Global`). It has no app context and no actions. Phase 1 adds both. |
| D2 | The liaison dispatches a worker per request | PARTIAL | The engine has it (`tasks.submit`, `executor.rs`). The chat cannot reach it. Phase 2 adds the `task.propose` action. |
| D3 | An explicit contract before any work starts | HAVE | `TaskContract` is validated and immutable once admitted (`orchestrator-core/src/contract.rs`). |
| D4 | A clean isolated worktree per task | HAVE | `orchestrator-git/src/worktree.rs`. Tasks never run in the primary checkout. |
| D5 | Dispatch profiles: which agent, model and effort | HAVE | Contract `agent` and `model_selection`, `agents.validate_selection`, `agents.catalog`, assignment profile policy v1. |
| D6 | Capacity bounds per project and host | HAVE | `SchedulerLimits`, `queue.snapshot`, `host.metrics`. |
| D7 | Event-driven wake of the liaison when something needs the user | GAP | Phase 3: the daemon is the watcher and starts an orchestrator turn. See "Wake-up loop" in the main document. |
| D8 | Turn-end guard: no turn ends blind while work runs | GAP | Phase 3: obligations live in the store (open proposals, pending approvals, unacknowledged wake records), not in the model's memory. The daemon re-surfaces them. A per-turn CLI cannot "forget" a record. |
| D9 | Durable wake queue, acknowledged only after handling | GAP | Phase 3: `wake_records` table. A wake is closed by the turn that handled it, or by the user. |
| D10 | Stuck and wedged worker detection | GAP | Phase 3: a daemon stall detector on session event time and attempt wall time. Deterministic, no model call. |
| D11 | Steer a running worker with text | GAP | Phase 4: headless `exec` and `-p` runs take no input mid-run, and contracts are immutable. Steering becomes a `task.revise` proposal: cancel plus a new contract with the added intent. Cost: in-flight progress is lost unless the worktree is reused (unverified). |
| D12 | Interrupt, exit and relaunch a worker | HAVE | `tasks.cancel`, `tasks.retry`. Phase 2 lets the orchestrator propose them. |
| D13 | Busy state is semantic, per adapter | HAVE | Adapter parsers (`codex_parser.rs`, `copilot_parser.rs`, Claude stream JSON) feed `sessions.events`. |
| D15 | Watch live worker output | HAVE | Agents and Sessions view over `sessions.list`, `sessions.events` with replay. |

## Task shapes

| ID | Firstmate capability | Status | MCAO mechanism |
| --- | --- | --- | --- |
| T1 | Ship task: a project change through a delivery path | HAVE | `TaskKind::Implement` with `DeliveryPolicy::LocalReview` or `PullRequest`. |
| T2 | Scout task: an investigation report, never a PR | PARTIAL | `TaskKind::Investigate`, `DeliveryPolicy::ReportOnly`, `ActionKind::ReportPublish` exist. Missing: a report the orchestrator reads and relays as findings. Phase 4. |
| T3 | Promote a scout to a ship | GAP | Phase 4: a `task.propose` whose card cites the report. The report is evidence, not authorization. |
| T4 | Decision inventory before a scout is cleaned up | GAP | Phase 4: decisions found in a report become held proposals. Cleanup waits until each is answered or held. |

## Project and mode management

| ID | Firstmate capability | Status | MCAO mechanism |
| --- | --- | --- | --- |
| P1 | Add or register a project | HAVE | `projects.register`. Phase 4 adds a `project.register` proposal. |
| P2 | Remove a project | GAP | Phase 4: `projects.remove` method and proposal. Refuses while tasks are live. |
| P3 | Per-project standing delivery mode (no-mistakes, direct-PR, local-only) | PARTIAL | Delivery and verification are per task. Missing: a per-project default the orchestrator must use unless the user says otherwise. Phase 4. |
| P4 | Validation rigor per mode | PARTIAL | `VerificationPolicy`, `orchestrator-git/src/verification.rs`, bounded repairs (`max_repairs`). Missing: a review-and-fix pipeline equal to no-mistakes. Phase 4 maps the modes to policies. |
| P5 | `+yolo` merge autonomy per project | GAP | Phase 5: a standing approval for the merge kind, scoped to one project. Green checks only. |
| P6 | Ship-branch prefix | PARTIAL | `orchestrator-git/src/branch_prefix.rs`, one prefix per machine (`moa/` default). Missing: per-project prefix. Phase 4. |
| P7 | Base-branch override per task | GAP | Phase 4: an optional contract field. Additive, so older contracts keep their meaning. |
| P8 | Resolve the project for each request | GAP | Phase 1: the snapshot lists projects by name. Phase 2: a task proposal names one project, and the daemon refuses an unknown id. |
| P9 | Strict project boundary: the liaison never writes to a project | HAVE | Chat runs in an empty per-turn directory, never a repository (`api_conversation_agent.rs`). Actions only reach the daemon as proposals. |
| P10 | Forge variants (Gerrit, GitLab, Forgejo) | GAP | Phase 6. Only GitHub pull requests exist (`delivery.request`). |

## Human in the loop and escalation

| ID | Firstmate capability | Status | MCAO mechanism |
| --- | --- | --- | --- |
| H1 | Escalate only real decisions, in outcome words | GAP | Phase 1: preamble rules. Phase 3: the daemon classifies events in code and starts a model turn only when interpretation is needed. |
| H2 | A decision is a task held for the user | PARTIAL | Delivery approvals (`approvals.list`, `approvals.resolve`, Review and Decisions view). Phase 1 and 2: action proposals. Phase 4: held decisions with an until date. |
| H3 | Never merge without the user's explicit word | HAVE | Approval integrity: the approval binds kind, parameter digest, candidate commit and task, and agents cannot approve (`orchestrator-core/src/approval.rs`). |
| H4 | Never tear down unlanded work | PARTIAL | Delivery writes a review ref and never merges (`delivery.rs`). The worktree cleanup rule was not verified. Phase 4 states it as a test. |
| H5 | Policy for validation findings that ask the user | PARTIAL | `review.request_changes`, `review.evidence`. Missing: routing a finding to the orchestrator for a decision card. Phase 4. |
| H6 | Report outcomes faithfully | PARTIAL | The UI shows store state. Phase 1: the preamble forbids claiming an action before the daemon confirms it. |
| H7 | Escalate a needed credential or login | PARTIAL | Copilot sign-in check, catalog states, `runtime_unavailable` codes. Phase 3 surfaces them as wake events. |
| H8 | A current user instruction outranks standing rules | GAP | Phase 5: a standing approval never overrides a per-action reject, and revoke takes effect at once. |

## Away and quiet modes

| ID | Firstmate capability | Status | MCAO mechanism |
| --- | --- | --- | --- |
| A1 | Away mode with a return brief | GAP | Phase 5: away is a named set of standing approvals plus quiet wakes plus a brief on return. |
| A2 | Quiet mode while present | GAP | Phase 3: monitoring setting `quiet`. Wake turns still run, replies collapse, no notification. |
| A3 | Away mode never widens authority | GAP | Phase 5 rule: away applies only the grants the user made. It adds none. |
| A4 | Alarm when an escalation is stuck undelivered | GAP | Phase 5: an OS notification when a decision waits past a threshold while away. |

## Status and reporting

| ID | Firstmate capability | Status | MCAO mechanism |
| --- | --- | --- | --- |
| S1 | Status digest (bearings) | PARTIAL | Overview screen, `queue.snapshot`, `tasks.list`. Phase 1: the snapshot lets chat answer "what is running". |
| S2 | Recap since last message and walk open decisions (ahoy) | GAP | Phase 3: a catch-up turn on return, ordered by impact, one decision at a time. |
| S3 | Activity ledger and record exports | PARTIAL | `mcao.audit-envelope.v1` projects session lifecycle. Action events are not projected yet. |
| S4 | Usage visibility and warnings | PARTIAL | `usage.summary`, daily token warnings, local tool usage. Chat turns record no usage receipts (no `usage_receipts` write outside `executor.rs`). Phase 3 prerequisite. |
| S5 | Capture a web page for visual review (lavish-url) | GAP | Phase 6 or later. |

## Recovery and restart

| ID | Firstmate capability | Status | MCAO mechanism |
| --- | --- | --- | --- |
| R1 | Restart-proof state on disk | HAVE | SQLite store with migrations, idempotency records. |
| R2 | Reconcile live work at start | HAVE | `orchestrator-runtime/src/reconcile.rs`, `api_conversation_orphans.rs`, delivery reconcile. |
| R3 | A start digest for the user | GAP | Phase 3: the first wake after daemon start summarizes what changed while it was down. |
| R4 | Stuck worker recovery playbook | GAP | Phase 3 detects. Phase 4: the orchestrator proposes cancel, retry or revise. |
| R5 | Detect missing or outdated tools | PARTIAL | `agents.catalog`, version pins, `orchestrator-diagnostics`. Installing a tool is out of scope. |

## Fleet scaling and secondmates

| ID | Firstmate capability | Status | MCAO mechanism |
| --- | --- | --- | --- |
| F1 | Persistent secondmates with a written scope | PARTIAL | A workstream has a summary, objectives, gates and its own conversation target. Missing: it cannot act. Phase 2 gives workstream conversations the same proposals inside their scope. |
| F2 | Route work to the secondmate whose scope fits | GAP | Phase 6: the Global orchestrator proposes a task linked to a workstream (`workstreams.link_task`). Phase 2 already allows an explicit link. |
| F3 | A secondmate reports up through a parent channel | GAP | Phase 3: the daemon posts a summary entry to Global when a workstream gate or task changes state. |
| F4 | Remote secondmates on an SSH host | GAP | Phase 6, design only. The IPC is a local socket with a token handshake. A remote daemon needs an authenticated network transport and a new threat model. |
| F5 | Quota-aware dispatch across accounts and CLIs | PARTIAL | Assignment profile policy v1, `usage.localTools`, catalog sign-in states. Missing: selection that reads remaining quota. Not planned: removed from phase 6 by the user (DPES-1105). |
| F6 | Refresh local clones after a merge (fleet sync) | GAP | Phase 4: fast-forward the registered project after a landed merge, refusing on local changes. |
| F7 | Flat command structure, depth capped | HAVE | Global, then workstream, then task. Phase 1 rule: a workstream conversation cannot create workstreams. |

## Backlog

| ID | Firstmate capability | Status | MCAO mechanism |
| --- | --- | --- | --- |
| B1 | Durable work queue with dependencies | HAVE | Task states `Created` and `Queued`, `task_dependencies`, `queue.snapshot`. |
| B2 | Holds with an until date | GAP | Phase 4: a held proposal with an optional date, shown in Review and Decisions. |
| B3 | Re-evaluate the queue after each completion | HAVE | The scheduler admits work as dependencies and capacity clear. |
| B4 | File work for later without dispatching | PARTIAL | Objectives and gates (`workstreams.add_objective`). Phase 1 lets chat propose them. |

## Integrations

| ID | Firstmate capability | Status | MCAO mechanism |
| --- | --- | --- | --- |
| I2 | Watch pull request checks and merge | GAP | Phase 4: a PR state poller and a merge action bound to the head commit. |
| I3 | External process-event sources | GAP | Phase 6: a daemon-owned source with a fixed adapter contract. |
| I5 | Many harness adapters | HAVE | Codex, Claude Code, GitHub Copilot and Pi adapters (`orchestrator-agents`). |

## Self-maintenance

| ID | Firstmate capability | Status | MCAO mechanism |
| --- | --- | --- | --- |
| M1 | Self-update the running instance | PARTIAL | Release pipeline (`release.yml`, `docs/RELEASE.md`). Missing: an in-app "update available" prompt. Not from chat. |
| M2 | Patch its own code and publish upstream only with consent | GAP | Decision N7, approved: the MCAO repository is registered as an ordinary project with the same approvals. Publishing to its upstream (push, PR, issue) is a per-action approval, never grantable. |
| M3 | Curate durable operational memory (stow) | GAP | Phase 5: user-visible orchestrator notes the snapshot includes, editable and deletable. |
| M4 | Unattended push to a protected branch | NEEDS USER DECISION | Decision N8: a scoped standing approval, not a hard no. |

## Firstmate rules and what they protect

| Firstmate rule | What it protects | MCAO equivalent |
| --- | --- | --- |
| Hard rule 1: never write to a project | The user's checkouts from the liaison's own edits | Chat runs outside any repository. No action kind writes project files. Only task workers in worktrees change code. |
| Hard rule 2: never merge without the explicit word, `yolo` the only relaxation | What lands on a branch | Approval integrity binds the commit. The merge kind exists only per action or under a scoped standing approval (phase 5). Never a red merge. |
| Hard rule 3: never tear down unlanded work | Work that exists nowhere else | No action kind discards a worktree or branch in phases 1 to 5. |
| Hard rule 4: workers never address the user | One interface | Task agents have no conversation write path. Their results reach the user through the orchestrator and daemon-confirmed records. |
| Hard rule 5: report outcomes faithfully | Trust in the summary | The UI shows only daemon-confirmed results. The preamble forbids early claims. |
| Self-repo boundary | Surprise pushes, issues or PRs to the tool's own upstream | Decision N7. |
| "Evidence is never authorization" | Action from a report or recommendation alone | A report produces proposals, never execution. |
| "Away never changes the authority set" | Silent widening while nobody watches | Away is a preset of grants the user made. It adds none. |
| "Standing autonomy is scoped consent per project" | Unbounded delegation | Standing approvals have scope, kinds, expiry, count and spend limits, a visible indicator, revoke and an audit entry per use. |
