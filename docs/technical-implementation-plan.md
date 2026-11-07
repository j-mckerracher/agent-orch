Technical Implementation Plan: Cross-Platform AI Agent Orchestrator

Proposed architecture · v1.0

Target: macOS and Windows Stack: Rust · Tauri 2 · React · TypeScript · SQLite Architecture: Native orchestration engine with replaceable coding-agent adapters Initial agents: Codex, Claude Code, Pi

1. Executive summary

Build a desktop application that lets one user delegate software engineering work to multiple autonomous coding agents without personally managing individual agent sessions.

The application will handle task decomposition, parallel execution, Git isolation, agent supervision, verification, approvals, and delivery.

The product should preserve FirstMate’s core principles while replacing its shell-script-driven infrastructure with a strongly typed, cross-platform Rust engine.

FirstMate’s documented capabilities include isolated worktrees, task contracts, persistent supervision, multiple coding harnesses, separate implementation and investigation workflows, and approval-controlled delivery. GitHub +2

Architectural decisions

Decision

Recommendation

Language

Rust rather than Zig

Desktop framework

Tauri 2

Frontend

React + TypeScript + Vite

Engine

Independent Rust library

Background execution

Separate local Rust daemon

Communication

Local authenticated IPC

Persistence

SQLite with WAL

Agent integration

Native subprocess protocols

Git operations

System Git executable

Parallelism

Tokio tasks + isolated worktrees

Terminal integration

Deferred until after core MVP

Deployment

Signed native installers

The most important decision: The Rust engine must work independently of the desktop application. Closing the window should not necessarily terminate ongoing agent work.

2. System architecture

React / TypeScript Desktop UI

Projects · Conversations · Task board · Diffs · Approvals

Tauri commands / local IPC client

Tauri Host

Window lifecycle · OS integration · Notifications · Installer

Authenticated local socket · JSON-RPC · Event stream

Rust Engine Daemon

Long-lived, stateful orchestration service

Task Orchestrator

Agent Supervisor

Dispatch Scheduler

Git Workspace Manager

Verification & Delivery

Approval / Policy Engine

Recovery Manager

Event Journal

Codex

Protocol adapter

Claude Code

Protocol adapter

Pi

Protocol adapter

Git Worktrees

Source isolation

SQLite

Durable state

Why a separate daemon?

A Tauri-only background runtime would be simpler to implement, but it ties running agents to the application process.

A daemon provides three important capabilities:

The UI can be closed and reopened without disrupting work.
A standalone CLI can access the same orchestration engine.
Future mobile or remote clients can connect through an explicitly authorized gateway.
The daemon should launch on demand, not require privileged system installation or a permanently running system service.

For the first working prototype, the engine can run directly in the CLI process. Introduce daemon IPC after the underlying task execution model is proven.

3. Rust workspace layout

Use a Cargo workspace with explicit boundaries.

agent-orchestrator/

├── Cargo.toml

├── crates/

│   ├── orchestrator-core/

│   │   ├── domain/

│   │   ├── tasks/

│   │   ├── scheduler/

│   │   ├── supervisor/

│   │   └── policy/

│   │

│   ├── orchestrator-agents/

│   │   ├── adapter.rs

│   │   ├── codex.rs

│   │   ├── claude.rs

│   │   ├── pi.rs

│   │   └── protocol.rs

│   │

│   ├── orchestrator-git/

│   │   ├── repository.rs

│   │   ├── worktree.rs

│   │   ├── diff.rs

│   │   └── delivery.rs

│   │

│   ├── orchestrator-store/

│   │   ├── migrations/

│   │   ├── tasks.rs

│   │   ├── sessions.rs

│   │   └── events.rs

│   │

│   ├── orchestrator-runtime/

│   │   ├── process.rs

│   │   ├── lifecycle.rs

│   │   └── recovery.rs

│   │

│   ├── orchestrator-ipc/

│   │   ├── protocol.rs

│   │   ├── server.rs

│   │   └── client.rs

│   │

│   ├── orchestrator-daemon/

│   │   └── main.rs

│   │

│   └── orchestrator-cli/

│       └── main.rs

│

├── apps/

│   └── desktop/

│       ├── src/

│       ├── src-tauri/

│       └── package.json

│

├── tests/

│   ├── fixtures/

│   └── integration/

│

└── docs/

    ├── architecture.md

    ├── protocols.md

    └── security.md

The dependency direction should be:

core defines contracts; adapters, storage, Git, and runtime implement them; daemon composes the implementations; UI and CLI act as clients.

Avoid allowing the core domain to depend on Tauri, SQLite, Git implementation details, or any particular agent provider.

4. Rust dependencies

The proposed dependencies favor mature, relatively simple implementations.

Crate

Purpose

Priority

tokio

Async scheduling, subprocesses, channels

Required

serde / serde_json

Data serialization and agent protocols

Required

uuid

Task, session, and event identifiers

Required

thiserror

Typed library errors

Required

tracing / tracing-subscriber

Structured logs and diagnostics

Required

sqlx

SQLite persistence, transactions, migrations

Required

async-trait

Async adapter interfaces, if needed

Required

clap

Standalone CLI

Required

tokio-util

Cancellation primitives

Required

schemars

Shared JSON schemas

Recommended

git2

Optional structured Git operations

Later

portable-pty

Cross-platform interactive terminal sessions

Later

keyring

OS credential storage where appropriate

Later

portable-pty provides cross-platform PTY abstractions, and SQLx provides SQLite integration with asynchronous operations. Rust +1

Additional tools:

Frontend: React, TypeScript, Vite, TanStack Query, Zustand, Tailwind CSS.
Diff visualization: Monaco Editor or CodeMirror.
Testing: Rust built-in test framework, tempfile, assert_cmd, Playwright.
CI: GitHub Actions with macOS and Windows runners.
Dependency auditing: cargo audit, cargo deny, and lockfile-aware JS auditing.
Pin direct dependencies and commit both Cargo and npm lockfiles. Do not select crate versions based solely on this plan; verify their latest compatible stable releases when initializing the workspace.

5. Domain model and interfaces

5.1 Primary entities

Model the system with six principal domain objects.

Entity

Responsibility

Project

Registered Git repository and its policies

Task

Unit of requested work

TaskContract

Objective, delivery mode, verification, autonomy

AgentSession

One agent’s execution lifecycle

Workspace

Branch and worktree owned by a task

Approval

Explicit authorization for a gated action

Separate Task from AgentSession: a task may have multiple execution attempts, retries, or follow-up sessions.

Likewise, a completed agent session must not automatically imply a completed task.

5.2 Task contract

Every task must have an explicit, persisted contract before execution.

pub struct TaskContract {

    pub id: Uuid,

    pub project_id: Uuid,

    pub parent_id: Option<Uuid>,

    pub objective: String,

    pub kind: TaskKind,

    pub agent: AgentSelection,

    pub dependencies: Vec<Uuid>,

    pub verification: VerificationPolicy,

    pub delivery: DeliveryPolicy,

    pub autonomy: AutonomyPolicy,

}



pub enum TaskKind {

    Implement,

    Investigate,

}



pub enum DeliveryPolicy {

    ReportOnly,

    LocalReview,

    PullRequest,

}



pub enum AutonomyPolicy {

    ReadOnly,

    WorkspaceWrites,

    ApprovalRequired,

}

This is a simplified domain design, not a literal reproduction of FirstMate’s schema.

The task contract should also record resource limits, branch metadata, retry policy, model selection where applicable, and the exact adapter version used for execution.

Important: An agent-selected plan is a proposal, not authorization. An LLM must not be able to silently change its own autonomy or delivery policy.

5.3 Task state machine

Primary successful path, with alternate paths for cancellation, pausing, interruption, and failure. Verification failures can trigger a bounded repair attempt.

All transitions must be validated centrally.

For example:

pub fn transition(

    current: TaskState,

    event: TaskEvent,

) -> Result<TaskState, TransitionError>;

Never let the frontend directly overwrite a task’s status.

Transitions and their audit events must be committed atomically.

5.4 Agent adapter contract

Use a capability-aware adapter interface instead of assuming every provider supports the same features.

#[async_trait]

pub trait AgentAdapter: Send + Sync {

    fn kind(&self) -> AgentKind;



    async fn detect(

        &self,

    ) -> Result<AgentCapabilities>;



    async fn spawn(

        &self,

        request: AgentLaunchRequest,

    ) -> Result<Box<dyn AgentHandle>>;

}



#[async_trait]

pub trait AgentHandle: Send {

    async fn next_event(

        &mut self,

    ) -> Result<Option<AgentEvent>>;



    async fn cancel(

        &mut self,

    ) -> Result<()>;



    async fn wait(

        &mut self,

    ) -> Result<AgentExit>;

}

AgentCapabilities should expose actual support for streaming, interactive input, cancellation, session resumption, structured output, approvals, and model selection.

Do not represent unsupported capabilities as successful no-ops.

The runtime supervisor owns process handles and reconciles process exits. The adapter owns provider-specific protocol translation.

5.5 Canonical event protocol

Translate agent output into a small internal vocabulary:

pub enum AgentEvent {

    SessionStarted {

        session_id: Uuid,

    },

    MessageDelta {

        text: String,

    },

    ToolStarted {

        tool_id: String,

        name: String,

    },

    ToolFinished {

        tool_id: String,

        success: bool,

    },

    ApprovalRequested {

        approval_id: Uuid,

        description: String,

    },

    TurnFinished {

        outcome: TurnOutcome,

    },

    SessionExited {

        exit_code: Option<i32>,

    },

    ProtocolError {

        message: String,

    },

}

Add correlation IDs, timestamps, sequence numbers, and raw-event references in the persisted event envelope.

Keep the raw provider events available for debugging. A canonical translation may lose important provider-specific details.

Separate high-frequency output deltas from important durable events. Persist control-plane events reliably and write large raw output streams to bounded logs.

6. Coding-agent integrations

I would implement the adapters in this order.

1. Codex — first adapter

Start with codex exec for one-shot execution. Evaluate Codex app-server when persistent sessions, approvals, and richer structured interaction are needed.

OpenAI documents these as distinct integration approaches. OpenAI Developers

2. Pi — second adapter

Use Pi’s JSONL RPC interface over stdin/stdout. Its documented protocol supports persistent subprocess communication, correlated responses, and asynchronous events. GitHub +1

3. Claude Code — third adapter

Integrate its supported headless and structured-output interfaces, verifying the precise launch flags and permission behavior against the installed CLI version.

Each adapter needs a conformance test suite covering launch, output framing, cancellation, nonzero exits, unexpected EOF, unsupported features, session interruption, and approval handling.

Do not automatically enable the most permissive execution flags. Permission behavior must be explicit and provider-specific.

7. Scheduling and concurrency

Use a dependency-aware bounded scheduler rather than assigning one thread to each agent.

A task becomes runnable only when its dependencies are satisfied, its contract is authorized, and execution capacity exists.

The scheduler should enforce:

A global maximum of concurrent agents.
A configurable concurrency limit per project.
Exclusive worktree ownership per running task.
Fair scheduling so one project cannot monopolize workers.
Bounded retries with backoff.
Optional provider-specific concurrency limits.
For the MVP, use a default of two concurrently executing workers.

pub struct SchedulerConfig {

    pub max_concurrent_agents: usize,

    pub max_agents_per_project: usize,

    pub max_retry_attempts: u32,

}

Use Tokio’s Semaphore for bounded concurrency and CancellationToken for coordinated shutdown.

A task’s dependencies should form a directed acyclic graph. Reject dependency cycles at submission time.

On retry, create a new execution-attempt record. Never erase the failed attempt.

8. Git worktree manager

Every implementation task should execute in its own worktree.

Example:

my-project/

  .git/

  src/



orchestrator-data/

  worktrees/

    task-a1/

    task-b2/

    task-c3/

Use the system Git executable initially. It provides broad compatibility with advanced Git behaviors without introducing an additional Git implementation layer.

Worktree lifecycle

Validate that the project is a Git repository.
Record the requested base ref and its resolved commit.
Allocate a unique branch and worktree path.
Create the worktree without changing the user’s active checkout.
Associate the worktree with exactly one task.
Launch the agent with the worktree as its working directory.
Collect diffs, commits, and verification results.
Obtain approval for delivery.
Merge or create a PR through the configured delivery mode.
Clean up only after a successful, verifiable delivery and safe-retention checks.
A unique task branch might be named:

orchestrator/task-a1b2c3

Do not reuse branches across unrelated tasks.

Critical Git rules

Never run automatic hard resets in the user’s primary checkout.
Never delete uncommitted work.
Never force-push without explicit authorization.
Never silently overwrite unrelated changes.
Never treat a worker-reported test result as authoritative.
Never delete a worktree containing unlanded changes or unresolved artifacts.
For parallel tasks targeting the same destination branch, serialized delivery is the simplest reliable model. Validate each candidate against the latest destination state before merging.

Worktrees provide source-code isolation, not a security sandbox. An agent in a worktree may still be able to access files, network resources, and credentials permitted to its operating-system account.

9. Persistence and recovery

Use SQLite as the source of truth for durable orchestration state.

Enable WAL mode, foreign keys, and a busy timeout. Keep database access behind a repository interface; UI components and agent adapters must not execute arbitrary SQL.

Initial database schema

Table

Key fields

Purpose

projects

id, root_path, settings

Registered repositories

tasks

id, project_id, parent_id, state, contract_json

Task definitions

task_dependencies

task_id, depends_on_id

Dependency graph

attempts

id, task_id, state, started_at, ended_at

Execution attempts

sessions

id, attempt_id, adapter, provider_session_id, pid

Agent execution

workspaces

id, task_id, path, branch, base_commit

Git isolation

events

seq, task_id, kind, payload, created_at

Ordered audit journal

approvals

id, task_id, action, status, resolved_by

Decisions

verification_runs

id, task_id, commit, status, log_path

Validation evidence

artifacts

id, task_id, type, path, checksum

Retained outputs

Store logs and large artifacts on disk, not as large SQLite blobs.

Crash recovery

On daemon startup:

Acquire a single-instance lock for the configured data directory.
Open the database and run migrations.
Reconcile unfinished attempts against actual OS process state.
Validate process identity, not only the recorded PID, to avoid PID reuse errors.
Inspect recorded worktree paths and Git metadata.
Reconcile partial delivery operations against Git’s actual state.
Classify interrupted tasks as recoverable, blocked, completed, or requiring inspection.
Resume scheduling only after reconciliation.
An existing process ID alone does not prove that the original agent is still running. Likewise, a process exit code of zero does not prove task completion.

For Windows, process-tree management needs explicit treatment. The first implementation should evaluate Windows Job Objects for reliable descendant-process cleanup; on macOS, use process-group and session semantics where appropriate.

A subprocess should never be considered safely detachable merely because it survived its parent’s exit.

10. Daemon IPC and frontend interfaces

Use a versioned JSON-RPC-style protocol over a local transport.

macOS: Unix domain socket.
Windows: Named pipe.
Transport: length-prefixed JSON messages.
Streaming: subscribed event notifications with sequence numbers.
Authentication: OS-level access restrictions plus a per-installation or per-session authorization mechanism.
Compatibility: explicit protocol versions and capability negotiation.
Avoid an unauthenticated localhost HTTP server.

A command envelope might look like:

{

  "jsonrpc": "2.0",

  "id": "req-42",

  "method": "tasks.submit",

  "params": {

    "projectId": "project-123",

    "objective": "Implement OAuth login",

    "delivery": "local-review"

  }

}

Primary methods:

API

Purpose

projects.register

Register a repository

projects.list

Enumerate projects

tasks.submit

Submit a validated task contract

tasks.list

Query tasks

tasks.get

Fetch task details

tasks.cancel

Request cancellation

tasks.retry

Start a new attempt

agents.capabilities

Discover installed agents

sessions.events

Replay task/session events

events.subscribe

Receive live events

approvals.resolve

Resolve a pending decision

workspaces.diff

Retrieve scoped Git diffs

delivery.request

Begin an authorized delivery

For mutating requests, include idempotency keys or another deduplication mechanism so reconnects and network retries cannot spawn duplicate tasks.

The event stream should support reconnection using the last observed sequence number. This prevents UI clients from missing durable decisions or status transitions.

Tauri supports commands, events, and higher-throughput channels. Use Tauri to manage the local desktop integration, while the daemon protocol remains independent. Tauri

11. Coordinator design

The coordinator is another agent session, but its authority is restricted to proposing and requesting operations through the Rust engine.

It should not directly own worktree creation, process supervision, policy evaluation, or task-state transitions.

Coordinator workflow

Receive the user’s objective.
Read bounded project context.
Propose a structured plan.
Identify dependencies and parallelizable tasks.
Generate proposed task contracts.
Submit those contracts to the engine’s validator.
Request approval for operations outside the configured autonomy envelope.
Observe worker events and verification outcomes.
Propose retries, follow-up tasks, or delivery.
Present final results and outstanding decisions.
Require structured coordinator output validated against a schema.

For example:

{

  "summary": "Implement OAuth authentication",

  "tasks": [

    {

      "key": "backend",

      "objective": "Implement OAuth backend",

      "dependsOn": [],

      "kind": "implement"

    },

    {

      "key": "frontend",

      "objective": "Implement login UI",

      "dependsOn": [],

      "kind": "implement"

    },

    {

      "key": "integration",

      "objective": "Integrate and test login",

      "dependsOn": ["backend", "frontend"],

      "kind": "implement"

    }

  ]

}

The Rust engine assigns canonical IDs, validates dependencies, resolves agent selection, and persists accepted contracts.

Supervision without unnecessary LLM calls

Most supervision should be deterministic.

The coordinator should be invoked only when meaningful interpretation or planning is required, such as a completed worker task, a blocked dependency, ambiguous verification failures, or a decision requiring the user’s judgment.

Ordinary process status checks, log streaming, timeouts, and dependency satisfaction do not need model inference.

This follows FirstMate’s emphasis on event-driven supervision without consuming model tokens merely to poll worker status. Jay Taylor’s notes

12. Verification, approval, and delivery

A worker may propose that its task is complete. It cannot certify its own successful delivery.

The engine should independently evaluate the task against its verification policy.

Verification pipeline

For an implementation task:

Confirm the worktree still belongs to the expected task.
Capture the candidate commit and diff.
Run configured validation commands.
Record exit status, duration, output, and the verified commit.
Check for unexpected changes to protected paths.
Evaluate delivery policy.
Request approval where required.
Deliver only the verified candidate, or rerun verification after changes.
In particular, a worker must not be able to weaken a required test gate by editing the project’s verification configuration and then reporting success.

Verification commands are themselves executable code. Run them under an explicit trust policy, with resource limits and isolation appropriate to the project.

Delivery modes

Report only: Produce a standalone investigation or review artifact. Do not merge code.

Local review: Prepare a candidate diff and require explicit authorization before integration.

Pull request: Push an authorized branch and create a PR, initially using the GitHub CLI. Do not merge merely because a PR was created.

For the MVP, support report-only and local-review delivery. Add PR automation after safe local delivery is working.

13. Security architecture

Security is a first-class implementation requirement because the application launches coding agents that can execute arbitrary code.

Trust boundaries

Boundary

Control

UI → daemon

Authenticated local IPC and method authorization

Coordinator → engine

Schema validation and immutable policy enforcement

Agent → filesystem

Restricted working directory and optional sandbox

Agent → credentials

Explicit environment allowlist

Agent → Git

Controlled delivery and protected branches

Agent → external services

User-authorized network and publishing operations

Plugins/extensions → runtime

Explicit trust and installation model

Required controls

Process execution: Never interpolate untrusted task text into shell command strings. Pass executable paths and argument arrays directly. Keep the working directory explicit.

Environment handling: Do not automatically forward the entire parent environment to agents. Preserve only required variables, and pass through provider authentication through documented mechanisms. Avoid recording tokens in task logs.

Filesystem isolation: A Git worktree is insufficient for untrusted code. Use a separate sandboxing layer for higher-risk tasks. Platform-specific isolation requires design and testing; do not claim uniform sandbox guarantees across macOS and Windows.

Prompt injection: Treat repository files, tool outputs, logs, and external documents as untrusted input to the coordinator. Instructions found there cannot authorize new actions or change policies.

Approval integrity: Record the exact action, parameters, relevant candidate commit, and approving actor. If the action changes after approval, require another approval.

Supply chain: Pin Rust and JavaScript dependencies, review Tauri plugins, and run dependency audits in CI. Do not automatically install repository-suggested coding-agent extensions or execute unreviewed setup scripts.

Updates: Signed application releases, verified update metadata, and no automatic self-modification of the orchestration engine.

Initial security model

For v1, describe agent execution as trusted local execution requiring user authorization, not as secure containment.

Implement stricter sandboxing before offering unattended operation on arbitrary repositories.

14. Desktop UI scope

The desktop application should organize information around work and decisions, not around terminal windows.

Orchestrator

3

Running

2

Awaiting review

8

Completed

Active project / authentication

Backend OAuth

Codex

Login component

Claude

API integration

Pi

Illustrative task activity indicators, not reliable completion percentages.

Approval required

Proposed merge · 7 files changed

UI specificationReview workflow

For the first desktop release, implement project selection, task submission, agent detection, live task events, cancellation, diff inspection, approval decisions, and recovery status.

Full interactive terminals, embedded code editing, remote hosts, and plugin marketplaces should remain outside the first release.

15. Implementation milestones

The schedule below is an indicative engineering estimate for one experienced developer, assisted by coding agents. It is not a delivery commitment.

M0

Architecture and feasibility

3–5 days

Create Cargo workspace and Tauri scaffold
Prove subprocess execution on macOS and Windows
Verify one Codex execution protocol
Define domain contracts and threat model
Exit criterion: A single task runs successfully on both OSes.

M1

Single-agent CLI

1–2 weeks

Implement task and workspace lifecycle
Run Codex inside a Git worktree
Capture output and exit status
Implement cancellation and cleanup
Exit criterion: CLI executes a task without touching the primary checkout.

M2

Persistence and concurrency

1–2 weeks

SQLite migrations and event journal
Dependency scheduler and concurrency limits
Task retry and process reconciliation
Recovery and crash-injection tests
Exit criterion: Two parallel tasks survive recoverable failures without lost changes.

M3

Coordinator and approvals

1–2 weeks

Structured task decomposition
Policy evaluation and independent verification
Approval state machine
Local-review delivery and safe cleanup
Exit criterion: A multi-task objective reaches a reviewed candidate without unapproved merging.

M4

Multi-agent support

1–2 weeks

Pi RPC adapter
Claude Code adapter
Capabilities and conformance tests
Agent selection and routing
Exit criterion: Each supported agent completes the same conformance workflow on both OSes.

M5

Tauri desktop UI

2–3 weeks

Daemon IPC and reconnectable event stream
Projects, task board, and coordinator conversation
Diff review and approval UI
Desktop lifecycle and installer packaging
Exit criterion: Closing/reopening the UI preserves running daemon tasks.

M6

Release hardening

1–2 weeks

Threat-model review and dependency auditing
macOS and Windows release CI
Signing, diagnostics, recovery usability
Installer and upgrade testing
Exit criterion: A clean-machine installation passes all release smoke tests.

Indicative total

9–15 weeks

Approximately, depending on protocol compatibility, Windows reliability, security work, and the depth of the desktop experience.

16. Testing strategy

Testing should focus on invariants and failure modes rather than only successful agent runs.

Test category

Required coverage

Domain unit tests

Task transitions, contract validation, DAG cycles

Adapter contract tests

Output parsing, cancellation, exit behavior

Git integration tests

Concurrent worktrees, dirty states, merge conflicts

Persistence tests

Transaction rollback, migrations, event replay

Recovery tests

Daemon crashes, process crashes, orphaned worktrees

Security tests

Path traversal, hostile repository text, unauthorized actions

Desktop tests

Reconnect, task rendering, approvals

Cross-platform tests

Windows paths, process cleanup, Git locking

Use temporary Git repositories and fake agents that can emit malformed events, exit unexpectedly, hang, spawn descendants, and simulate interrupted operations.

Real coding-agent tests should be a separate integration suite because they may require credentials, model quotas, and external network access.

CI must run on both native macOS and Windows. Linux CI alone will not establish Windows reliability.

Release acceptance criteria

The first stable release should demonstrate:

Concurrent execution without cross-task worktree corruption.
No unauthorized writes to the user’s primary checkout.
Reliable approval enforcement, including after restarts.
Recovery without silently losing uncommitted work.
Correct cancellation and process cleanup on both operating systems.
Deterministic handling of adapter protocol errors.
Installation and upgrade on clean macOS and Windows environments.
17. Major risks and mitigations

Risk

Impact

Mitigation

CLI protocol changes

Adapter failures

Capability detection and version-specific conformance tests

Windows subprocess behavior

Orphaned agents

Job Objects, process supervision tests

Concurrent Git operations

Conflicts or lost work

Worktree ownership, serialized delivery

Daemon crashes

Inconsistent state

Transactional journal and startup reconciliation

Agent prompt injection

Unauthorized actions

Engine-enforced policies, approval boundaries

Untrusted repository scripts

Host compromise

Sandboxing roadmap and restricted execution

Overengineering

Delayed usable MVP

Start with single-agent CLI

Excessive model calls

Cost and latency

Deterministic event-driven supervisor

18. Recommended first implementation slice

Do not begin with Tauri or the multi-agent coordinator.

Build this executable first:

orchestrator run \

  --project /path/to/repository \

  --agent codex \

  --task "Add unit tests for the authentication module"

It should:

Validate the repository and installed agent.
Create a task record and isolated worktree.
Launch Codex in that worktree.
Stream structured progress to the console.
Record the process result, diff, and task state.
Preserve the worktree for inspection.
Return a clear outcome and actionable error information.
Then add:

orchestrator tasks

orchestrator inspect <task-id>

orchestrator cancel <task-id>

orchestrator diff <task-id>

orchestrator cleanup <task-id>

Initially, these operate on local persisted state with one active CLI-owned runtime. Once the execution and recovery semantics are reliable, move the scheduler into the daemon and make the CLI a client.

This incremental approach prevents building a complicated desktop interface around an orchestration model that has not yet been validated.

19. Explicitly deferred capabilities

Do not implement these in v1:

Remote or distributed execution.
Cloud-hosted orchestration.
Multi-user collaboration.
Voice or messaging integrations.
Autonomous self-modification or automatic skill installation.
Vendor-independent interactive terminal emulation.
Automatic conflict resolution without review.
Unrestricted autonomous PR merging.
Full filesystem sandbox equivalence across operating systems.
Design stable extension points where appropriate, but avoid implementing abstractions solely for speculative future features.

20. Final recommendation

The implementation should proceed in this order:

Rust task execution → Git isolation → durable state → deterministic supervision → coordinator → multi-agent adapters → daemon → Tauri UI.

This sequence minimizes architectural risk while producing working software at each stage.

I would treat Milestones 0–3 as the first meaningful MVP, because they establish the actual value of the application: reliable delegated work, independent verification, and safe delivery. The desktop UI is valuable, but it should sit on top of those capabilities rather than define them.

A successful v1 would not need to reproduce all of FirstMate. It would provide its most important workflow—one user directing a supervised crew of coding agents—with stronger native lifecycle management and cross-platform support.

Reference material

FirstMate repository⁠
FirstMate architecture⁠
Tauri 2 documentation⁠
Codex integration architecture⁠
Pi RPC protocol⁠
portable-pty Rust documentation