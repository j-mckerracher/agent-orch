# Agent-orch audit envelope v1

DPES-736. Repository contract as of 2026-11-16 UTC.

`mcao.audit-envelope.v1` describes source audit records for agent-orch. Its SIEM projection is
`audit.activity` in the released [AEC v1.0.0 event contract](https://github.com/mc-innersource/motif-audit-event-contracts/blob/9f93a59cccde8ec2165e785a0b64a5f03de26713/spec/siem-event-v1.md#mcao-activity-projection).
That release, commit `9f93a59cccde8ec2165e785a0b64a5f03de26713`, owns the wire schema and its closed
`data` and `lineage` objects. This document defines the repository's sources and correlation rules.
It does not change the released wire contract.

## Fields and provenance

The source schema is [mcao.audit-envelope.v1.schema.json](mcao.audit-envelope.v1.schema.json).
The projection is implemented by `orchestrator_store::audit_envelope::project_session_activity`.
The mapper accepts a recorded `TaskDurableEvent`, its owning `TaskContract`, explicit trusted
context, and explicit writer fields. It creates no identities, IDs, chain hashes, or timestamps.

| Source member | Repository source and rule | SIEM destination |
| --- | --- | --- |
| `schema` | Constant `mcao.audit-envelope.v1` | `source_route`; SIEM `schema` is `mcao.siem.event/v1` |
| `ts` | Writer-supplied original observed event instant, UTC RFC 3339 with exactly three fractional digits and `Z` | `occurred_at_utc`, unchanged |
| `event_id` | Writer-assigned UUIDv7, stable across retries | `lineage.event_id`; top-level `event_id` is `audit.activity:` followed by it |
| `home_id` | Trusted home-private configuration UUIDv4; not a user ID | `data.home_id` |
| `seq` | Writer-assigned nonnegative sequence per home | `lineage.seq` |
| `prev_hash` | Writer-supplied previous source-record hash, null exactly at sequence zero, otherwise `sha256:` plus 64 lowercase hex digits | `lineage.prev_hash` |
| `timezone` | Trusted runtime user's IANA timezone | Used to derive `day_key`; omitted from SIEM |
| `day_key` | Calendar date of `ts` in `timezone`, including daylight-saving rules | `data.day_key`; never substituted with the UTC batch date |
| `event` | `durable_events.kind`, unchanged | `data.event` |
| `category` | Explicit kind mapping below | `data.category` |
| `outcome` | `not_applicable` for the currently mapped lifecycle and manual-release facts | `data.outcome` |
| `platform` | Constant `mcao` | `data.platform` |
| `session_id` | Nonempty `durable_events.session_id` | `data.session_id` |
| `task_id` | `durable_events.task_id`; must equal the supplied contract's `id` | `data.task_id` |
| `agent.role` | Explicit trusted role: firstmate, secondmate, ship, or scout | `data.agent.role` |
| `agent.harness` | Owning `TaskContract.agent`; must be nonempty | `data.agent.harness` |
| `agent.model` | `TaskContract.model_selection.model`, or null when unrecorded; empty strings rejected | `data.agent.model` |
| `actor.type` | Explicit writer-supplied user, agent, or system classification | `data.actor.type` |
| `actor.id` | Explicit nonempty writer-supplied ID; for a user actor, exactly the resolved pseudonymous user ID | `data.actor.id` |
| `repo` | Null in this projection. A local project path is not a repository slug | `data.repo` |
| `pr` | Null in this projection. A local review ref is not a PR URL | `data.pr` |
| `commit` | Null in this projection. Session rows do not record a delivered commit | `data.commit` |
| `user.id` | Trusted identity resolver's `hmac-sha256:` pseudonym with 64 lowercase hex digits | `data.user.id` |
| `user.identity_source` | Trusted resolver's github_login, git_email, or local_account | `data.user.identity_source` |

`durable_events.created_at` is SQLite write time in UTC at whole-second precision. It is not an
original millisecond event instant and is not used as `ts`. A future writer must capture the event
instant at observation and persist it for retries. Projection or read time must never replace it.
The existing journal cannot retroactively supply missing source timestamps, identities, roles, or
chain values. Those omissions remain explicit, rather than being filled with plausible values.

The mapper validates identity, UUID versions, actor equality, chain-field shape, timestamp and
calendar validity, timezone, task ownership, session presence, harness, and model. Unresolved
identity returns `identity_unresolved`; the writer must refuse emission and alert. This module
neither implements an identity resolver nor decides where that alert is sent.

## Journal correlation

`SqliteStore::task_events_after(task_id, after_sequence, limit)` makes one SQL query over
`durable_events`, with a task-constrained left join to `delivery_steps`. It returns the task's
ordered journal rows and recorded delivery lineage. Its `sequence` is the store-wide journal
cursor. Other tasks consume positions, so gaps in a task's results do not prove deletion. This
cursor is unrelated to the audit writer's per-home `seq` and must never become that sequence.

| Fact | Recorded correlation source | Projection treatment |
| --- | --- | --- |
| Task | Every row's `task_id` | `data.task_id` when the row can be projected |
| Dispatch attempt | `attempt.created` and `attempt.state_changed` payload `attempt_id`; `session.created` also records `attempt_id` | Retained in task-query payload; no invented SIEM lineage member |
| Session | Row `session_id`; `session.created` and `session.state_changed` payload `session_id` | `data.session_id` |
| Approval | `approval.*` payload `approval_id` | Retained in task-query payload; no invented session or SIEM lineage member |
| Delivery | `delivery.*` payload `step_id` joins `delivery_steps.id` within the same task | `delivery.approval_id`, `delivery.verification_run_id`, `delivery.candidate_sha` on the query result |
| Cancellation recovery | `cancellation.owner_manually_released` row session and payload attempt/session/generation | Session projects; payload remains local correlation evidence |

AEC activity lineage is closed and has exactly `event_id`, `seq`, and `prev_hash`. Attempt,
approval, delivery-step, verification-run, and candidate IDs cannot be inserted into that object.
They remain available through the task query. A delivery candidate SHA is not automatically the
commit executed by an earlier session. Payload contents, review destinations, and local paths are
not copied into audit output.

The recorded-stream integration test drives actual `SqliteStore` task admission, attempt creation,
session lifecycle, approval request/approval, and delivery recording. It then checks the single-query
correlation, pagination, task isolation, and session-to-fixture projection. Trusted and writer inputs
are explicitly synthetic test inputs. This is store integration evidence, not a deployed audit chain.

## Kind mapping and refusals

| Journal kind | Category | Current activity support |
| --- | --- | --- |
| `session.created` | lifecycle | Projects its recorded session |
| `session.state_changed` | lifecycle | Projects its recorded session, including the cancellation-confirmation producer |
| `cancellation.owner_manually_released` | supervision | Projects its recorded session; a user actor must use the resolved pseudonym |
| `task.created`, `task.transition`, `workspace.added` | Unassigned | Current rows have no session; refused with `SessionRequired` |
| `attempt.created`, `attempt.state_changed` | Unassigned | Current rows have no session; refused with `SessionRequired` |
| `approval.*`, `delivery.*`, `review.changes_requested` | Unassigned | Current rows have no session; refused with `SessionRequired` |
| Any new session-scoped kind | Unassigned | Refused with `UnmappedKind` until reviewed mapping exists |

A task can have multiple attempts and sessions. The mapper must not attach the latest session to a
task-only fact or mint a session ID to make that fact fit the released activity schema. Missing
session provenance requires a producer change or an authoritative contract decision. This task
publishes that boundary; instrumentation, the durable chain writer, and shipping are subsequent
board scope. No daemon producer or delivery authority changes in this revision.

## Compatibility and privacy

Version 1 source readers ignore unknown extension members. Known fields still require their
declared type and semantic validity. The released SIEM projection is a strict allowlist and drops
source-only material. It carries chain values but cannot verify or reconstruct the original source
hash input. Source serialization, signing, source-chain verification, batching and transport remain
owned by their existing contracts.

The schema accepts nullable repo, PR and commit references for future trusted producers; this
mapper emits null because the selected session facts lack those sources. It never forwards the
raw journal payload. Caller-owned sanitization must run before durable audit spooling or sending;
schema validation alone does not establish that strings are safe. Never include prompts, messages,
tool inputs/results, credentials, tokens, signed URLs, request bodies, or raw user identities.
Operational diagnostics and traces are not durable audit delivery evidence.

## Verification

Use the immutable AEC release checkout or verified release bundle. Its `requirements-test.txt`
provides the validator dependencies. From this repository root:

```sh
cargo test --locked -p orchestrator-store
python docs/audit-envelope/validate_released_v1.py \
  --release /absolute/path/to/motif-audit-event-contracts-v1.0.0 \
  --fixtures docs/audit-envelope/fixtures
```

Also run the released AEC conformance suite using its own documented invocation. The fixture check
validates native positive and negative fixtures, checks rejection on the intended field, and validates
SIEM fixtures with both the released JSON Schema and the release's semantic validator. Each SIEM
fixture must also equal the allowlisted projection of its same-named source fixture. The native
`repo-pr-commit-present` fixture exercises the source schema for future trusted producers; the
current Rust journal mapper intentionally emits null for those references. The Rust test
compares projections of recorded session rows with those SIEM fixtures. Midnight fixtures cross the
local day boundary in America/Chicago; UTC batch-day behavior remains independent.
