# Security Architecture and Threat Model

Updated: 2026-11-07T11:38:03-07:00 (America/Denver).

Status: M0-04 candidate. This document defines the v1 threat model for the
orchestrator and states which threats are addressed by the domain contracts in
`crates/orchestrator-core` and which are deferred. It follows plan section 13
and is deliberately narrower than the full plan: v1 is trusted local execution
requiring user authorization, not secure containment.

The M6-01 review of dependencies, environment allowlists, untrusted inputs,
and delivery boundaries is in
[`security/m6-01-threat-model-and-dependency-review.md`](security/m6-01-threat-model-and-dependency-review.md).

## 1. Threat model statement

The application launches coding agents that execute arbitrary code under the
same operating-system account as the user. The security goal for v1 is not to
contain a hostile agent; it is to guarantee that **nothing executes or changes
beyond what the user explicitly authorized, and that authorization is
auditable and cannot be self-granted by an agent**.

Trust levels, from most to least trusted:

1. **The human user** — the only origin of new authority.
2. **The engine** (`orchestrator-core` state machine, policy evaluation,
   delivery) — deterministic code the user runs. It applies authority but
   creates none.
3. **The coordinator** — an LLM session. Proposes contracts and requests
   actions. Its output is untrusted input to the engine's validator.
4. **Worker agents** — LLM sessions executing a contract. Their output,
   proposals, and self-reports are untrusted. They cannot certify their own
   delivery.
5. **Repository content, tool output, logs, external documents** — untrusted
   input to every LLM in the system (prompt-injection surface).

## 2. Trust boundaries and controls

| Boundary | Threat | Control | Enforced in |
|---|---|---|---|
| UI → engine | Unauthorized or forged state writes | Mutating requests go through the daemon's authenticated local IPC; the frontend never writes task state directly | plan §10; `task::transition` is the only state writer |
| Coordinator → engine | Self-granted authority, policy drift | Schema-validated proposals checked against an `Authorization` envelope: a proposal must request exactly the autonomy it was granted and a delivery from the envelope's explicit set; an LLM proposal cannot widen its own autonomy or delivery policy | `contract::validate_proposal`, `authorization::Authorization` |
| Agent → engine state | Agent claims its own completion or success | Task state advances only on engine-observed facts (candidate commit, verification outcome); an agent exit is an attempt fact, never a task fact | `task::Attempt`/`Session` separation; `TaskEvent` origins |
| Agent → filesystem | Writes outside the task worktree | Worktree is the working directory; worktree is source isolation, not a sandbox. Higher-risk isolation is deferred (see §5) | plan §8 |
| Agent → credentials | Token exfiltration or reuse | Environment allowlist; provider auth via documented mechanisms only; tokens never recorded in task logs | plan §13 |
| Agent → Git | Unapproved merge, force-push, checkout corruption | Delivery only of the verified candidate commit; approval bound to the exact commit; no auto-merge in v1 | `authorization::apply`, delivery policies |
| Agent → external services | Unauthorized publishing | Network/publishing operations are user-authorized actions, each with its own approval | delivery policies |
| Prompt injection → coordinator/worker | Instructions in repository or tool output that attempt to authorize new actions | Instructions found in untrusted input have no authority; the only authority source is the user's recorded approval and the user's envelope. The engine does not pass free text from repository content into policy decisions | this document §3; engine design |

## 3. Authorization invariants

These are invariants of the system, not features. Each is backed by code in
`crates/orchestrator-core` (M0-04) or by a deferred milestone.

1. **Authority originates with the user only.** No agent, coordinator, or
   repository artifact can create an authorization. The engine applies
   authorizations; it does not mint them.
2. **Proposals are never grants.** A coordinator or worker may propose a
   contract, a plan, a completion, or a delivery. Each proposal must pass
   engine-side validation and, where gated, an approval, before it has effect.
3. **No self-widening.** A proposal's autonomy must be *exactly* the
   autonomy its authorization envelope grants — there is no total order on
   `AutonomyPolicy`, so nothing is "more" than anything else, and an agent
   cannot propose a different autonomy than it was granted. A proposal's
   delivery must be one of the choices in the envelope's explicit delivery
   set; `LocalReview` never implies `PullRequest`. Delivery policy and
   autonomy are orthogonal fields; one never implies the other.
4. **Central, exclusive state writing.** Task and attempt state changes are
   computed only by `task::transition` / the attempt rules. Every accepted
   transition emits exactly one `AuditEvent`; persistence of state change and
   audit event is one transaction (store's responsibility). The UI and
   agents can request; only the engine decides. Only `Succeeded` and
   `Cancelled` are irreversible; quiescent-failure states accept only an
   authorized retry that starts a fresh attempt, preserving the failed one.
5. **Task/session separation.** A completed agent session does not complete a
   task. A task may span multiple attempts and sessions; failed attempts are
   preserved, never erased. Completion requires engine-side verification of
   a candidate commit, not an agent report.
6. **Approval integrity.** An approval is bound to the fully typed
   `authorization::Action`: the operation kind, its destination and
   parameters, the candidate commit, and the task. A change to *any* field —
   a new commit, a different task, a different destination or parameter, a
   different operation kind — is a mismatch (`ApplyError::Mismatch`) and
   requires a new approval. Approvals are single-use and terminal once
   consumed, voided, or expired.
7. **Verification independence.** A worker may propose completion; the
   engine evaluates the contract's `VerificationPolicy`. Verification
   commands are argument arrays from the contract — not the repository's
   own configuration — so a worker editing the project's test setup cannot
   weaken a required gate; they run under an explicit trust policy with
   resource limits. The verified candidate is the recorded commit, not the
   agent's claim.
8. **No shell interpolation.** Untrusted task text is never interpolated into
   shell command strings; executables are launched with explicit argument
   arrays and working directories (plan §13).
9. **Auditability.** Every transition, approval, verification run, and
   delivery is journalable; a decision made without its audit record is a
   defect, not a recovery path.

## 4. Failure semantics treated as security properties

- **Process exit is not completion.** A zero exit code from an agent proves
  nothing about the task; the state machine has no transition from agent exit
  to `Succeeded` without a verified candidate.
- **PID is not identity.** Recovery validates process identity, not recorded
  PID (plan §9).
- **Candidate change after approval.** Re-verification and re-approval are
  required; the old approval is voided, not "reused with the new commit".
- **Crash between approval check and delivery.** The approval remains
  `Granted` and un-consumed; delivery consumes it in the same transaction it
  records. No double-delivery from retry.

## 5. Explicitly out of scope for v1 (deferred)

- **Cross-platform filesystem sandboxing.** Worktrees are source isolation,
  not containment. An agent can touch anything its OS account can. Uniform
  macOS/Windows sandbox guarantees are deferred (plan §19); strict
  sandboxing is required before unattended operation on arbitrary
  repositories.
- **Remote or multi-user operation.** v1 is single-user, local. The daemon
  IPC is local and authenticated; a future remote gateway is an explicit
  scope change with its own threat model.
- **Supply-chain hardening of the engine itself.** Signed releases and
  update verification are M6.
- **Adversarial agent as security boundary.** v1 assumes the user authorizes
  which agents run and inspects results before integration.

## 6. Test obligations

The security properties above map to test categories in plan §16. M0-04
supplies the domain-level tests (transition legality, terminal states,
envelope checks, approval binding). Subsequent milestones must cover, at
minimum: hostile repository text that attempts to induce state writes
(security tests), approval replay after candidate change (persistence
tests), and path traversal into the primary checkout (git integration
tests).
