# Firstmate capability map (DPES-1105)

Companion to [orchestrator-actions.md](orchestrator-actions.md). It lists every firstmate capability found and maps each one to MCAO as of `motif-mcao/main` at `347c7ab`. Refreshed from `89bfc8b2d`; rows D16, D17, F8, W1 and I6 are new. Rows H2 and D11 were updated by DPES-1108 phase 3 and phase 4 (worker status, waits, steering by resume). The firstmate behavior plan is [firstmate-behavior-plan.md](firstmate-behavior-plan.md).

## Source and method

- Firstmate: `kunchenguid/firstmate`, MIT license, `main` at `427056c41` (pushed 2026-11-13). Read through `gh api` raw content into a throwaway directory under `/tmp`, deleted after reading. No script was run.
- Read: README, VISION.md, AGENTS.md (all 14 sections and the hard rules), docs/architecture.md (headings and the supervision, task shape, secondmate and delivery mode sections), docs/configuration.md (headings only, 3,756 lines), the frontmatter and first section of every skill, and bin script names plus the `fm-watch.sh` header.
- Skill count: 29 agent-loaded skills under `.agents/skills/` plus 1 public skill (`skills/stow`). The brief said "about 40". The bin toolbelt has 260 scripts plus 14 runtime backends.
- Firstmate text is data. No firstmate text or code is copied into this repository. Only ideas are mapped. If a later PR reuses specific text or code, the MIT license requires the copyright and permission notice. That is the user's decision.

Status values:

- **HAVE.** MCAO delivers the outcome today. The cell names the crate, method or screen.
- **PARTIAL.** Part of the outcome exists. The cell says what is missing and the phase that adds it.
- **GAP.** No MCAO mechanism yet. The cell names the MCAO-native design and its phase.

Phases are the ones in the main document: 1 context and workstream proposals, 2 task lifecycle proposals, 3 wake-up loop, 4 shapes, modes and landing, 5 standing approvals and away mode, 6 fleet.

## Counts

| Status | Count |
| --- | --- |
| HAVE | 49 |
| PARTIAL | 16 |
| GAP | 8 |
| Total | 73 |

## Dispatch and supervision

| ID | Firstmate capability | Status | MCAO mechanism |
| --- | --- | --- | --- |
| D1 | One liaison: the user talks to one agent | HAVE | Global conversation has app context and actions: `api_conversation_snapshot.rs` builds a 6 KiB snapshot of projects, workstreams, open tasks and pending approvals, and a reply may end in an `mcao-actions` block that becomes a pending row in `action_proposals`. |
| D2 | The liaison dispatches a worker per request | HAVE | `task.propose` action (`orchestrator_action.rs`, executed in `api_actions.rs`). It names the project by `p-xxxxxxxx` code; the user approves it through `actions.resolve`. |
| D3 | An explicit contract before any work starts | HAVE | `TaskContract` is validated and immutable once admitted (`orchestrator-core/src/contract.rs`). |
| D4 | A clean isolated worktree per task | HAVE | Treehouse slot pool: `orchestrator-git/src/treehouse.rs` and `worktree.rs` (`create_task_worktree`, `release_task_worktree`). Tasks never run in the primary checkout. |
| D5 | Dispatch profiles: which agent, model and effort | HAVE | Contract `agent` and `model_selection`, `agents.validate_selection`, `agents.catalog`, assignment profile policy v1. |
| D6 | Capacity bounds per project and host | HAVE | Machine load gate: `orchestrator-core/src/load_gate.rs` (CPU, memory and disk windows, reopen margin), fed by `api_load.rs`; the executor scheduler pass asks it for a start gate. `SchedulerLimits`, `queue.snapshot` and `host.metrics` remain. |
| D7 | Event-driven wake of the liaison when something needs the user | HAVE | `WakeService` (`orchestrator-daemon/src/wake.rs`): a 2 s tick classifies durable events (`wake_events.rs`) into `wake_records` and starts a Restricted wake turn only when `needs_model` is set. Kinds are in `WakeKind` (`orchestrator-store/src/sqlite_wake.rs`). |
| D8 | Turn-end guard: no turn ends blind while work runs | HAVE | Obligations live in the store, not model memory: the service arms a `work_check` per scope while work exists and re-raises `proposal_waiting` and `delivery_waiting` from `pending_wake_records`. A notice is committed before any turn starts. |
| D9 | Durable wake queue, acknowledged only after handling | HAVE | `wake_records` states noticed, pending, running, handled, skipped, failed; a record closes only after its turn settles; `monitoring.ack` marks it seen. Dedupe and debounce: `orchestrator-core/src/supervision.rs`. |
| D10 | Stuck and wedged worker detection | HAVE | `supervision::stalled`: over wall time (default 120 min) or no session event for `max(interval, 5 min)`. Deterministic, no model call, yields a `task_stalled` notice. There is no worktree-change signal. |
| D11 | Steer a running worker with text | PARTIAL | A worker that reports `needs-decision` or `blocked` on its status file ends its turn and its task waits (`TaskState::Waiting`, no process, slot kept). The user's answer (`decisions.answer`, `decisions.reconcile`) or an approved `task.message` resumes the same CLI session with the reply as the next prompt (`claude --resume`, `codex exec resume`, `copilot --resume`; DPES-1108 phase 4, `executor.rs`, `task_messages`). `task.revise` (never grantable) still cancels and replaces a task for a change of scope. Missing: text for a worker that is still running (only a waiting one takes a message), and a Codex Auto task cannot resume at its level, so it fails closed. |
| D12 | Interrupt, exit and relaunch a worker | HAVE | `tasks.cancel`, `tasks.retry` and the `task.cancel`, `task.retry` proposals. |
| D13 | Busy state is semantic, per adapter | HAVE | Adapter parsers (`codex_parser.rs`, `copilot_parser.rs`, Claude stream JSON) feed `sessions.events`. |
| D15 | Watch live worker output | HAVE | Agents and Sessions view over `sessions.list`, `sessions.events` with replay. |
| D16 | Wake rate and cost limits | HAVE | `supervision.rs`: 6 per hour and 40 per day per scope, optional daily token cap, 60 s debounce, batch cap 20, user-turn precedence; `agent_failing` pause after repeated failures. |
| D17 | Dispatch defaults and work classes | HAVE | `orchestrator_settings.dispatch_defaults`, `settings.workClasses.get` and `.set` (`api_work_classes.rs`), applied at submit time. |

## Task shapes

| ID | Firstmate capability | Status | MCAO mechanism |
| --- | --- | --- | --- |
| T1 | Ship task: a project change through a delivery path | HAVE | `TaskKind::Implement` with `DeliveryPolicy::LocalReview` or `PullRequest`. |
| T2 | Scout task: an investigation report, never a PR | HAVE | `task_reports`: the executor stores a redacted scout report; `reports.list`, `reports.get`, `reports.relay` (`api_reports.rs`). A `scout_report_ready` wake runs a Restricted global turn that carries the report as delimited data. |
| T3 | Promote a scout to a ship | HAVE | `task.propose` with `promote_from`. `promotion_denial` (`api_actions.rs`) requires a Succeeded Investigate task in the same project whose report was relayed. Lineage row `promoted_from`. |
| T4 | Decision inventory before a scout is cleaned up | GAP | Phase 4: decisions found in a report become held proposals. Cleanup waits until each is answered or held. |

## Project and mode management

| ID | Firstmate capability | Status | MCAO mechanism |
| --- | --- | --- | --- |
| P1 | Add or register a project | HAVE | `projects.register`. Phase 4 adds a `project.register` proposal. |
| P2 | Remove a project | HAVE | `project.remove` proposal (never grantable) soft-archives via `projects.archived_at`; `ArchiveError` refuses `ActiveTasks` and `PendingApprovals`. No standalone `projects.remove` method. |
| P3 | Per-project standing delivery mode (no-mistakes, direct-PR, local-only) | HAVE | `project_defaults`: mode, base_branch, branch_prefix, checks, merge_method (`orchestrator-core/src/project_mode.rs`; `projects.get_defaults`, `projects.set_defaults`). A PR mode from chat also needs the `chat_pr_delivery` setting. |
| P4 | Validation rigor per mode | PARTIAL | `ProjectDefaults::verification()` maps the mode to a `VerificationPolicy` with the project checks (`orchestrator-git/src/verification.rs`, bounded repairs). No review-and-fix pipeline equal to no-mistakes. |
| P5 | `+yolo` merge autonomy per project | HAVE | A merge grant is a `standing_grants` row: user-created only, one project, `confirm_unattended_merge`, at most 20 uses and 24 h, head-branch prefix, covers only `delivery.merge`. `orchestrator-core/src/landing.rs` requires green required checks and a head SHA equal to the bound commit. |
| P6 | Ship-branch prefix | HAVE | `project_defaults.branch_prefix` overrides the machine prefix in `orchestrator-git/src/branch_prefix.rs`. |
| P7 | Base-branch override per task | PARTIAL | Per-project `project_defaults.base_branch`; `worktree.rs` cuts the slot from it. No per-task contract field. |
| P8 | Resolve the project for each request | HAVE | The snapshot lists projects as `<folder> p-xxxxxxxx`; `resolve_project` (`api_actions.rs`) refuses unknown and ambiguous codes. |
| P9 | Strict project boundary: the liaison never writes to a project | HAVE | Chat runs in an empty per-turn directory, never a repository (`api_conversation_agent.rs`). Actions only reach the daemon as proposals. |
| P10 | Forge variants (Gerrit, GitLab, Forgejo) | GAP | Phase 6. Only GitHub pull requests exist (`delivery.request`). |

## Human in the loop and escalation

| ID | Firstmate capability | Status | MCAO mechanism |
| --- | --- | --- | --- |
| H1 | Escalate only real decisions, in outcome words | HAVE | `wake_events.rs::classify` is pure and runs in code; `needs_model` is false for `delivery_waiting`, so the user is notified without a model turn. |
| H2 | A decision is a task held for the user | HAVE | Approvals (`approvals.*`), action cards (`actions.*`), `decision_holds` (`decision.hold`, `holds.*`) and questions: `decision_holds` rows of kind `question` bound to a task, raised by `decision.ask` (phase 3) or by the task's own worker when it reports `needs-decision` (phase 4), closed only by the user's verbatim answer (`decisions.answer`) or evidence (`decisions.reconcile`), recorded append-only in `decision_answers`. An open question blocks scheduling (`AwaitingDecision`) and holds its task in `Waiting`, and the answer resumes the worker. No candidate is recorded while a worker's own question is open. Listed under Needs your decision. |
| H3 | Never merge without the user's explicit word | HAVE | Approval integrity: the approval binds kind, parameter digest, candidate commit and task, and agents cannot approve (`orchestrator-core/src/approval.rs`). |
| H4 | Never tear down unlanded work | PARTIAL | `orchestrator_git::delivery::check_worktree_safe_to_delete` guards the CLI `cleanup`, and delivery never merges. The daemon returns a task slot on archive or merge (`slot_returned`, `api.rs`) with no check for finished-but-unlanded local work. Plan phase 5. |
| H5 | Policy for validation findings that ask the user | PARTIAL | A `candidate_ready` wake runs a turn that reads the attempt summary and changed-file count. Review findings (`review.request_changes`) are not routed to the orchestrator as decision cards. |
| H6 | Report outcomes faithfully | HAVE | The preamble forbids claiming an action before the daemon confirms it (`api_conversation_agent.rs`); the UI shows only store state. |
| H7 | Escalate a needed credential or login | HAVE | `sign_in_needed` wake (`wake_events.rs`, `orchestrator-agents/src/copilot_sign_in.rs`). |
| H8 | A current user instruction outranks standing rules | HAVE | `grant_matcher.rs` approves only pending Restricted-turn proposals created after the grant; revoke is checked on every use; `project.remove`, `task.cancel`, `task.revise` are never grantable. |

## Away and quiet modes

| ID | Firstmate capability | Status | MCAO mechanism |
| --- | --- | --- | --- |
| A1 | Away mode with a return brief | HAVE | `MonitoringMode::Away` (user-only `monitoring.set`); `monitoring.brief` and `monitoring.brief_ack` build a count-only return brief (`api_brief.rs`, `return_brief.rs`; UI `ReturnBrief.tsx`). |
| A2 | Quiet mode while present | HAVE | `MonitoringMode::Quiet`: wake turns still run, `notifies()` is false (`supervision.rs`, `api_monitoring.rs`). |
| A3 | Away mode never widens authority | HAVE | Away is quiet plus a brief window and adds no authority; results state `authority: none`. Grants are separate user-made `standing_grants`. |
| A4 | Alarm when an escalation is stuck undelivered | PARTIAL | OS reminders for unanswered delivery approvals after 4 h, then every 4 h (`src-tauri/src/notifications.rs`, `DecisionNotifications.tsx`). Approvals only, not action proposals, and not tied to away mode. |

## Status and reporting

| ID | Firstmate capability | Status | MCAO mechanism |
| --- | --- | --- | --- |
| S1 | Status digest (bearings) | HAVE | The chat snapshot answers "what is running", plus the Overview screen and `queue.snapshot`. |
| S2 | Recap since last message and walk open decisions (ahoy) | PARTIAL | Count-only return brief (`monitoring.brief`). No model catch-up turn ordered by impact. |
| S3 | Activity ledger and record exports | PARTIAL | `mcao.audit-envelope.v1` projects session lifecycle. Action events are not projected yet. |
| S4 | Usage visibility and warnings | PARTIAL | Chat turns record `conversation_usage_receipts`; they feed the wake token cap and grant budgets but `usage.summary` does not show them. |
| S5 | Capture a web page for visual review (lavish-url) | GAP | Phase 6 or later. |

## Recovery and restart

| ID | Firstmate capability | Status | MCAO mechanism |
| --- | --- | --- | --- |
| R1 | Restart-proof state on disk | HAVE | SQLite store with migrations, idempotency records. |
| R2 | Reconcile live work at start | HAVE | `orchestrator-runtime/src/reconcile.rs`, `api_conversation_orphans.rs`, delivery reconcile. |
| R3 | A start digest for the user | HAVE | `daemon_start` wake at startup; start and work-check records are handled together as one catch-up turn. |
| R4 | Stuck worker recovery playbook | HAVE | `task_failed` and `task_stalled` wakes; the orchestrator proposes `task.cancel`, `task.retry` or `task.revise`, and the user approves each. |
| R5 | Detect missing or outdated tools | PARTIAL | `agents.catalog`, version pins, `orchestrator-diagnostics`. Installing a tool is out of scope. |

## Fleet scaling and secondmates

| ID | Firstmate capability | Status | MCAO mechanism |
| --- | --- | --- | --- |
| F1 | Persistent secondmates with a written scope | PARTIAL | Workstream conversations carry proposals inside their own workstream (`BlockScope::Workstream`). Project coordinator conversations cannot act: their action blocks are stripped (`TurnBlock::Stripped`). Plan phase 7. |
| F2 | Route work to the secondmate whose scope fits | HAVE | Global `task.propose` with `workstream_id` and `objective_id`, and `workstream.link_task`. Routing is the model's judgment, not a code router. |
| F3 | A secondmate reports up through a parent channel | GAP | Phase 3: the daemon posts a summary entry to Global when a workstream gate or task changes state. |
| F4 | Remote secondmates on an SSH host | GAP | Phase 6, design only. The IPC is a local socket with a token handshake. A remote daemon needs an authenticated network transport and a new threat model. |
| F5 | Quota-aware dispatch across accounts and CLIs | PARTIAL | Assignment profile policy v1, `usage.localTools`, catalog sign-in states. Missing: selection that reads remaining quota. Not planned: removed from phase 6 by the user (DPES-1105). |
| F6 | Refresh local clones after a merge (fleet sync) | GAP | Phase 4: fast-forward the registered project after a landed merge, refusing on local changes. |
| F7 | Flat command structure, depth capped | HAVE | Global, then workstream, then task; coordinator conversations are read-only. |
| F8 | Per-project coordinator conversation | PARTIAL | `ConversationTarget::Project`, `coordinator.send_message`, `coordinator.propose`; opt-in. It can talk and propose a decomposition but not act, and has no wake scope reporting to the parent. |

## Backlog

| ID | Firstmate capability | Status | MCAO mechanism |
| --- | --- | --- | --- |
| B1 | Durable work queue with dependencies | HAVE | Task states `Created` and `Queued`, `task_dependencies`, `queue.snapshot`. |
| B2 | Holds with an until date | HAVE | `decision_holds`: `decision.hold` action, `holds.set`, `holds.release`, `holds.list`; `until` is a `YYYY-MM-DD` date. A question is deferred the same way (`decisions.defer`): it stays open and keeps its task held, and is marked deferred until that day ends. |
| B3 | Re-evaluate the queue after each completion | HAVE | The scheduler admits work as dependencies and capacity clear. |
| B4 | File work for later without dispatching | HAVE | Chat can propose `workstream.create`, `objective.add` and `objective.revise`; nothing is dispatched without a task. |
| W1 | Order initiatives | PARTIAL | `workstreams.prioritize` and `workstreams.priority`; advisory, the scheduler does not read it. |

## Integrations

| ID | Firstmate capability | Status | MCAO mechanism |
| --- | --- | --- | --- |
| I2 | Watch pull request checks and merge | HAVE | `pull_requests` and `pull_request_repos`; the landing thread (`orchestrator-daemon/src/landing.rs`) polls PR state and required checks; `delivery.merge` is bound to the head commit. The user opens the PR by hand. |
| I3 | External process-event sources | GAP | Phase 6: a daemon-owned source with a fixed adapter contract. |
| I5 | Many harness adapters | HAVE | Codex, Claude Code, GitHub Copilot and Pi adapters (`orchestrator-agents`). |
| I6 | Read-only tool server for a chat turn | PARTIAL | `mcp_server.rs`, off by default: three read tools over ids, Restricted non-wake turns only. Not an external event source. |

## Self-maintenance

| ID | Firstmate capability | Status | MCAO mechanism |
| --- | --- | --- | --- |
| M1 | Self-update the running instance | PARTIAL | Release pipeline (`release.yml`, `docs/RELEASE.md`). Missing: an in-app "update available" prompt. Not from chat. |
| M2 | Patch its own code and publish upstream only with consent | GAP | Decision N7: not now. The orchestrator gets no self-patching or self-publishing support. |
| M3 | Curate durable operational memory (stow) | HAVE | DPES-1108 phase 6: `memory_entries` and the append-only `memory_archive` (0067). Shared preferences (2,048 bytes, read by every turn including wakes, read-only outside the global conversation), workstream or project preferences (2,048), learnings per scope (1,024, stale after 180 days unconfirmed). Keyed entries update in place. `memory.list` / `memory.set` / `memory.remove` / `memory.restore`, and the `memory.set` action. UI `PreferencesSection.tsx` lists stale entries for the user, never deletes them. |
| M4 | Unattended push to a protected branch | HAVE | A merge grant (see P5) is the opt-in unattended merge; none exists until the user creates one. |

## Firstmate rules and what they protect

| Firstmate rule | What it protects | MCAO equivalent |
| --- | --- | --- |
| Hard rule 1: never write to a project | The user's checkouts from the liaison's own edits | Chat runs outside any repository. No action kind writes project files. Only task workers in worktrees change code. |
| Hard rule 2: never merge without the explicit word, `yolo` the only relaxation | What lands on a branch | Approval integrity binds the commit. The merge kind exists only per action or under a scoped standing approval (phase 5). Never a red merge. |
| Hard rule 3: never tear down unlanded work | Work that exists nowhere else | No action kind discards a worktree or branch in phases 1 to 5. |
| Hard rule 4: workers never address the user | One interface | Task agents have no conversation write path. Their results reach the user through the orchestrator and daemon-confirmed records. |
| Hard rule 5: report outcomes faithfully | Trust in the summary | The UI shows only daemon-confirmed results. The preamble forbids early claims. |
| Self-repo boundary | Surprise pushes, issues or PRs to the tool's own upstream | Nothing is built for self-publishing (N7). Publishing to MCAO's own upstream is never grantable. |
| "Evidence is never authorization" | Action from a report or recommendation alone | A report produces proposals, never execution. |
| "Away never changes the authority set" | Silent widening while nobody watches | Away is a preset of grants the user made. It adds none. |
| "Standing autonomy is scoped consent per project" | Unbounded delegation | Standing approvals have scope, kinds, expiry, count and spend limits, a visible indicator, revoke and an audit entry per use. |
