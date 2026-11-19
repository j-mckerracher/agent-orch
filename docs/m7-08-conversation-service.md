# M7-08 — Orchestrator conversation service (design record)

Plan: `/Users/mckerracher.joshua/Code/agent-orch-worktrees/mckerracher-joshua-mclm-friendly-guide/docs/technical-implementation-plan.md`.
Board item: M7-08 (depends on M7-00, M5-02c). Status: backend service and
tests implemented in M7-08; the UI binding is M7-24; the real agent runtime is
M7-25 (see "M7-25: the agent runtime" below).

## Decision summary

| Question | Decision |
| --- | --- |
| Runtime behind the conversation | **Provider-neutral.** The daemon depends on the `ConversationRuntime` trait (`api_conversation_runtime.rs`), not on a product. The OMP/model clauses of `orch-impl-m.md` are overridden by the UI gap analysis. No OMP behavior is invented. |
| Runtime that ships | M7-08 shipped `UnavailableRuntime` (every turn failed with `runtime_unavailable`). **M7-25 replaced it** with `AgentRuntime` (`api_conversation_agent.rs`), which launches an installed agent. When no agent is launchable the turn still fails closed with `runtime_unavailable`. |
| Version pin | **Mandatory.** `ConversationService` takes the list of accepted `RuntimeIdentity` values (name + exact version). A runtime that requires a pin and has an empty list gets `runtime_unpinned`. An identity not in the list gets `version_mismatch`. In both cases the runtime never receives the message. M7-25 pins the versions it was live-checked against (see below). |
| Persistence and recovery owner | The **daemon** owns the transcript (SQLite, migration `0031_conversations.sql`, renumbered from 0030 at integration). Runtime state is never trusted or resumed. On startup `with_config` marks every `running` turn `failed` with `interrupted_by_restart`. |
| Authority | **None.** Conversation methods only read and write conversation rows. Text (user or runtime) is data and is never parsed into methods, with one exception: a completed reply may end with one `mcao-actions` block, which the daemon parses into pending proposals. The block is parsed only for a Restricted turn sent by a session that negotiated `orchestrator_actions`; any other turn strips it (`action_block` is `stripped`) and creates no proposals. A proposal carries no authority until the user approves it in a later step. Coordinator proposals still go through `coordinator.propose` and engine validation. Every response carries `"authority": "none"`. |

## Data model

- `conversations` is implicit per target: `conversation:global` or
  `conversation:workstream:<id>`, or `conversation:project:<project_id>`.
- `conversation_turns`: one per user message. States `running`, `completed`,
  `failed`, `cancelled`. At most one `running` turn per conversation (partial
  unique index). Records the runtime name and version used.
- `conversation_entries`: append-only. Updates abort. Deletes abort unless a history clear is running (DPES-1105, see "Clear history and remove coordinator" below). Kinds
  `user_message`, `reply_chunk`, `turn_completed`, `turn_failed`,
  `turn_cancelled`. `sequence` is a per-database monotonic cursor; entry ids are
  deterministic (`<turn>:user`, `<turn>:chunk:N`, `<turn>:end`), so every reply
  chunk is correlated to its message by `turn_id`.
- Bounds: message 8 KiB, chunk 16 KiB (larger deltas are split at character
  boundaries), reply 256 KiB per turn (over the bound the turn fails with
  `reply_too_large` and the runtime is canceled).
- First terminal state wins. A late chunk after cancel or failure is rejected
  (`SinkClosed`) and never stored.

## RPC surface (proposed names; capability `coordinator`)

Names are proposals so concurrent daemon work does not collide; no new
`Capability` variant was added.

- `conversations.send {target, text, idempotency_key, selection?, permission?}`
  — records the message and starts a turn. The call succeeds even when the
  runtime then fails; the failure is a `turn_failed` entry plus
  `turn.error_code`. A second send while a turn is `running` is `Conflict`.
  `selection` is the model choice for this message (see "Per-message choice"
  under M7-25); it is omitted when the user has chosen none and no saved choice
  is in effect, or when the choice is unavailable or not confirmed yet, and a
  send without it is the request older clients have always sent. `permission` is the
  level the message runs at (see "Permission levels" under M7-25); a send
  without it runs at `restricted`.
- `conversations.get {target, after_sequence?, epoch?}` — page of 100 entries,
  `more`, `next_after_sequence`, `latest_sequence`, latest `turn`, store `epoch`.
  `after_sequence` beyond the latest is `ReplayGap`; a wrong `epoch` is
  `EpochMismatch`; an unused conversation reads as empty. Every page also
  carries `proposals` (DPES-1105): every `pending` proposal of the conversation,
  plus the newest settled ones (`executed`, `failed`, `rejected`, `superseded`),
  newest first, at most 100 settled and at most 256 KiB of settled JSON.
  `proposals_more` is `true` when a settled proposal was left out by either
  limit; pending proposals are never left out. Each turn carries
  `action_block`: `null`, `accepted`, `rejected` or `stripped`. A
  `turn_completed` entry carries the same `action_block` when its turn has one,
  and omits the field when it has none, so a client can keep a per-turn notice
  after later messages arrive. Every proposal also carries `denial` (the reason the
  envelope refused a task, otherwise `null`) and `target` (display data read at
  response time). `target` is `{"project_name": …}` for a task proposal,
  `{"task_title": …, "task_state": …}` for a cancel, retry or link, and `null` for
  the other kinds or a task that no longer exists. Neither holds a path. See "Task
  proposals" below.
- `actions.resolve {proposal_id, decision, digest, idempotency_key}` — `decision`
  is `approve` or `reject`. `digest` is the lowercase hex digest the proposal shows;
  a value that does not match the stored digest is `proposal_invalid` and changes
  nothing, so an approval binds the parameters the user saw. A proposal from a
  turn that did not run at `restricted` is also `proposal_invalid`. Needs the
  `orchestrator_actions` capability; a session without it gets `-32004`, the
  capability-not-negotiated error. It also needs the approver credential: a
  session that did not prove it gets `-32012` (`Forbidden`) before any lookup
  runs, see "Approver credential" below. `resolve` then checks the configured
  approver against the approval allowlist, and the desktop is the only client that
  sends `actions.resolve`. Returns `{"proposal": …}`, the proposal as it stands
  after the call. An approved proposal runs in the same call. A run that
  fails still returns success, with `state` `failed` and an `error_code`. Codes:
  `proposal_not_found` (-32021), `proposal_not_pending` (-32022) and
  `proposal_invalid` (-32023), plus the usual `invalid_params` and
  `idempotency_in_progress`. See "Action approval and execution" below.
- `actions.pending {}` — read only (DPES-1105, phase 2). Returns
  `{"proposals": […], "more": false}`: the `pending` proposals of every
  conversation, newest first, at most 100 and at most 256 KiB of JSON. `more` is
  `true` when a pending proposal was left out. Each entry is a proposal as
  `conversations.get` shows it, plus `scope`: `{"kind":"global"}` or
  `{"kind":"workstream","workstream_id":"…"}`. Needs the `orchestrator_actions`
  capability; a session without it gets `-32004`. It changes nothing, so it is not
  a user-only method: a session that holds the token alone may call it.
- `conversations.cancel {target, turn_id, idempotency_key}` — scoped to the
  target's conversation (otherwise `NotFound`). Finishes the turn `cancelled`,
  then asks the runtime to stop (best effort). `cancel_applied=false` when the
  turn had already ended.
- `target` is `{"kind":"global"}`, `{"kind":"workstream","workstream_id":"…"}`,
  or `{"kind":"project","project_id":"…"}`.
  Unknown fields are rejected.

### Project coordinator replies

The `project_conversations` capability advertises project targets on the shared
runtime. The project must exist and its coordinator must be active. Replies,
model selection, permission levels, cancellation, idempotency, and restart
recovery use the same path as Global and workstreams. The snapshot identifies
the project by its sanitized label, not its root path.

Project turns retain the coordinator's no-action authority: action blocks are
stripped and cannot create proposals. Monitoring remains Global/workstream-only.
Legacy `coordinator.get` notes remain readable and are shown before runtime
exchanges; they are not replayed as agent requests. Clearing or removing a
coordinator deletes both histories in one transaction and refuses while a user
turn runs. Restore starts empty. Older clients may still use the legacy note API;
new clients never fall back to it for sending.

Migration 0054 adds project conversation scopes without changing existing
conversation IDs or entries. It follows 0053's opt-in transition: removed
coordinators remain removed, and cleared notes are not restored. The store holds
one connection with foreign keys disabled during
migration, restores enforcement, and verifies all foreign keys before opening.

Error codes on `turn_failed`: `runtime_unavailable`, `credentials_missing`,
`permission_denied`, `version_mismatch` (a required flag is absent from a
readable help; the entry's body names the flag), `runtime_changed`, `runtime_unpinned`,
`runtime_failed`, `runtime_timeout`, `runtime_busy`, `reply_too_large`,
`interrupted_by_restart`, `runtime_provider_limit` (the agent's provider refused the turn for a
spend cap, usage, rate or quota limit; the entry's body is `provider_limit`), and, for a choice the
catalog refuses (DPES-1105), one code per refusal reason:
`selection_agent_unavailable`, `selection_authentication_failed`,
`selection_agent_state_unknown`, `selection_agent_unsupported`,
`selection_models_unsupported`, `selection_models_unknown`,
`selection_unknown_model`, `selection_effort_requires_model`,
`selection_effort_unsupported`, `selection_effort_unknown`.

Permission codes (DPES-1105). `permission_level_unsupported` is emitted: the
agent has no mapping for the level. In PR A that is Extended, on every agent.
`permission_review_denied` and `permission_review_unavailable` are defined but
**not emitted until the live check** (see "Permission levels" under M7-25).

## Ordering, idempotency, replay

- Turn id = stable hash of the idempotency key; a retried send returns the
  stored result and calls the runtime once. The stored result holds references
  only (DPES-1105): the reply is read back from the store. Same key with different text is
  `IdempotencyConflict`. The shared idempotency layer caches failures too: a
  rejected attempt (for example `Conflict`) stays rejected for that key, so a
  retry after the turn ends needs a new key.
- A crash between the idempotency claim and its completion leaves the key
  `IdempotencyInProgress` (existing fail-closed behavior of
  `idempotency.rs`).
- Reconnect replay is cursor polling: the client keeps `next_after_sequence`
  and calls `conversations.get`. Conversation events do not enter the global
  durable event journal because `durable_events.task_id` is NOT NULL; there is
  no push stream in this item.

## M7-07 seam (workstreams)

M7-07 stored workstreams after this item was written. The conversation keeps
`workstream_id` as an opaque, nullable, length-bounded text column with **no
foreign key**, and resolves it through the `WorkstreamDirectory` trait. M7-25
supplies the store-backed directory (`StoreWorkstreams`): an unknown workstream
is `NotFound`. What the agent learns about workstreams comes from the app
state snapshot (see "Prompt" under M7-25). A foreign key remains optional for a
later migration.

## Concurrency

`start_turn` runs under the global idempotency gate and must not block; real
adapters spawn work and return a control handle. The control registry is only
populated while the store shows the turn `running`; terminal commits happen in
the store first, so a stale control is never left registered.

## M7-25: the agent runtime

`AgentRuntime` implements `ConversationRuntime` over the M7-09 catalog and the
M7-06 adapters. It is bound in `DaemonApi::with_config`; the daemon binary needs
no flag.

- **Selection.** `AGENT_ORCH_CONVERSATION_AGENT` (`codex` or `claude-code`)
  names the agent and keeps the environment's model and effort. Without it, the
  Settings default agent (`dispatch_defaults.default_agent`) runs alone, with the
  model and effort saved for it; an environment model or effort replaces that
  pair. With no Settings default, the first launchable one in catalog order is
  used (`codex`, then `claude-code`). A Settings default that names no chat agent
  fails `runtime_unavailable` and no other agent runs. `AGENT_ORCH_CONVERSATION_MODEL`
  and `AGENT_ORCH_CONVERSATION_EFFORT` are checked against the catalog; an option
  the catalog does not offer fails closed. Agent program paths are the existing
  `AGENT_ORCH_CODEX_PROGRAM` / `AGENT_ORCH_CLAUDE_PROGRAM`.
- **Launchable.** The adapter's `detect()` must succeed, a bounded
  `--version` probe (500 ms, at most 1 KiB of output) must give a version, and
  the catalog must report the agent `available` (signed in). A program that is
  present but cannot be started, exits non-zero, prints something that is not a
  version or prints too much is not launchable: the turn fails
  `runtime_unavailable` at once and nothing is launched. A probe that does not
  answer in 500 ms (a hung program, or a cold start of a real CLI) and a
  catalog that has not been read yet are retryable `Conflict`s, but only for
  15 s (`conversations.send` calls `prepare()` before the idempotency gate);
  after that the turn fails `runtime_unavailable` too. The selection and
  version are cached for 30 s, per choice. Wake turns and report relays run
  the default agent, not the user's last choice, so they settle the default
  first: `reports.relay` calls `prepare()` before its idempotency gate, and the
  wake loop leaves its windows due and retries on the next tick until the
  default is settled. Saving the settings calls `settings_changed`, which drops
  the cached default and its retry window, so the next selection reads the new
  default agent at once instead of after the 30 s window.
- **Verified versions.** Compiled in (`codex` 0.167.0 and 0.167.2,
  `claude-code` 2.1.311, `github-copilot` at its pinned version). 0.167.2 was
  checked against its `codex exec --json` event order and command lifecycle.
  These run with no warning. An operator marks another version verified with
  `AGENT_ORCH_CONVERSATION_PINS=name@version,...`. The pin list decides whether
  the warning shows; it does not decide whether a version may run.
- **Unverified versions (DPES-1106).** A CLI version the list does not name
  runs, and the desktop shows once per agent and version per app session:
  `<Agent> <version> has not been verified with MCAO. Replies may fail.` The
  warning is informational and never blocks a send, and needs no action. The
  turn records the version it ran on, and `conversations.get` reports
  `verified: false` for it.
- **Help probe.** Before launch, the daemon checks the CLI's help
  (`codex exec --help`; `--help` for Claude Code and Copilot). The metadata
  probe is bounded by the probe timeout and 256 KiB of output; it sends no
  prompt. Successful results are cached briefly by executable identity. The
  daemon checks the actual command options for the requested permission level,
  even for a verified version. A missing required option produces
  `version_mismatch` and names the option. A failed, timed-out, or unreadable
  probe produces `runtime_compatibility_unconfirmed`; retry the check. Neither
  case starts the agent. Advertised options are not evidence that vendor
  behavior or structured output has stayed the same.
- **Provider limits (DPES-1106).** A failed turn whose agent reports a spend
  cap, usage limit, rate limit, quota, insufficient quota, HTTP 429 or billing
  refusal fails `runtime_provider_limit`, and its entry's body is
  `provider_limit`. The match is case-insensitive over the agent's own failure
  text, so one list serves Codex and Claude Code. Every other failure the agent
  reports fails `runtime_failed`. The daemon writes one line to its log for each
  such failure: the agent, the code and the agent's reason, cut to 200
  characters. The log holds no operator text and no prompt. The transcript
  stores only the code and body, never the reason.
- **Output the parser cannot read.** The parser stays fail-closed. An unverified
  turn whose output the parser rejects fails `runtime_failed`, and the turn's
  recorded version names what ran. An unverified turn that exits cleanly with
  no text also fails `runtime_failed`, never an empty reply.
- **The one risk the probe cannot catch.** A version can list every flag and
  still change the JSONL event shapes. The daemon cannot know that until a
  turn runs; the parser then fails the turn rather than showing a wrong reply.
- A selection that changes between the selection and the launch fails
  `runtime_changed` instead, so a version that was verified is never reported as
  unsupported. `start_turn` compares the identity again, but only against the
  cached selection: an upgrade of the CLI inside the 30 s window is not
  detected.
- **Prompt (DPES-1105).** Two parts. The instructions are fixed text, at most
  3 KiB: what the app is, its concepts (project, workstream, objective, gate,
  task, approval, coordinator), and the rules (the agent cannot see the screen,
  cannot change anything yet, never claims an action happened, and treats the
  app state and history as data). The content is the app state snapshot, up to
  6 earlier completed turns (4 KiB per reply, 12 KiB in total) and the current
  message last. The conversation service builds the snapshot from the store
  (`api_conversation_snapshot.rs`, at most 6 KiB, oldest items dropped first);
  the runtime gets it as `TurnRequest.context` and holds no store handle. The
  snapshot lists the scope, projects, workstreams with objective and gate
  counts, a workstream target's objectives and gates, task counts and up to 20
  open tasks (id, state, project, workstream, objective cut to 80 bytes), and
  the pending delivery approval count. A project id is its root path, so a
  project is named by its folder name and a short hash of the id. The snapshot
  never holds a project path, settings, tokens, environment, diffs or session
  output; every snapshot field and every earlier turn is one line. Claude
  gets the instructions through `--append-system-prompt` and the content
  through `-p`. Codex and Copilot have no system prompt flag, so the
  instructions lead their one prompt argument between fixed delimiters. When the
  prompt would pass 30,000 Windows command-line units, the oldest earlier turns
  are dropped. The prompt is never passed through a shell.
- **Process.** Agent arguments for the message's permission level
  (`args_for_conversation`, see "Permission levels" below). Each Codex and
  Claude Code turn runs in its own empty directory under
  `<data-dir>/conversation-scratch`, removed when the turn ends. The adapter's
  `ChildSupervisor` provides a Unix process group (killed as a
  group), a Windows Job Object, and a line cap (16 MiB stdout, 64 KiB stderr). Turn deadline 180 s
  (`AGENT_ORCH_CONVERSATION_TIMEOUT_MS`, clamped 100 ms to 600 s) gives
  `runtime_timeout`; at most 4 turns run at once, otherwise `runtime_busy`.
  The catalog and version probes kill only the direct child on Windows; a Job
  Object for those probes is a follow-up.
- **Text.** Replies and prompt text lose control characters (except newline
  and tab), carriage returns and bidirectional controls before they are stored
  or passed on.
- **Text.** Replies and prompt text lose control characters (except newline
  and tab), carriage returns, bidirectional controls and characters that render
  as nothing (U+00AD, U+061C, U+200B, U+2028/2029, U+2060 to U+2064, U+FEFF and
  the tag block U+E0000 to U+E007F) before they are stored or passed on. The
  zero-width joiner and non-joiner stay, because emoji sequences and several
  scripts need them; tag-based flag emoji lose their tags.
- **Authority.** The runtime holds no store, task, approval or delivery handle
  beyond the reply sink. A reply is data, with one exception: a completed reply
  may end with one `mcao-actions` block. The daemon parses that block into
  pending proposals, which carry no authority until the user approves them in a
  later step. Only a Restricted or Auto turn from a session that negotiated
  `orchestrator_actions` gets a parse, and only that turn's instructions name
  the block format. At Auto and Full the agent has a shell and could reach the
  daemon's local token. A shell can edit the store or replace the daemon, which
  no credential stops. Full gets no actions. Auto does, because it is the default
  chat level and the user accepted that limit (DPES-1105). Any other turn
  strips the block, records
  `action_block` `stripped`, and creates no proposals. A block that does not
  pass is recorded as `rejected`, and a block in a canceled or failed turn is
  ignored.
- **Recovery.** Unchanged: the daemon owns the transcript, a turn the daemon
  lost fails with `interrupted_by_restart`, and replay is by sequence. A
  replayed `conversations.send` returns the turn as it is now, not as
  recorded. After a `kill -9` the agent can outlive the daemon, so every
  running agent is recorded (pid, process group, start time) under
  `<data-dir>/conversation-agents`, and the next start kills the group of each
  record only when the process still matches that identity. A leader that is
  gone, or any identity mismatch, is never signaled. A group whose leader
  died first is left alone, as are agents spawned within milliseconds before
  the kill. Windows relies on the Job Object (`KILL_ON_JOB_CLOSE`).
- **Known limit.** A launched agent can read the user's files and runs the
  user's own agent settings and hooks, as any run of that CLI does. What a run
  may change depends on its permission level (see "Permission levels"). No
  level limits what it can read.

**Per-message choice (DPES-1105).** `conversations.send` takes an optional
`selection: {agent, model?, effort?}`, the agent, model and effort the picker
chose for that message. The rules:

- **Per message.** The choice applies to the message it is sent with, and the
  daemon stores nothing beyond the turn's identity. The desktop saves the
  choice in its browser storage on each change, restores it when the picker
  mounts, and sends it with each message until the user changes it (DPES-1105).
  A restored choice counts as the user's own: it is sent as if just picked.
  Fixture mode neither restores nor saves a choice.
- **Unavailable is shown, never replaced.** On a daemon that takes a choice, a
  saved choice the daemon no longer offers is shown as unavailable, with the
  daemon's reason: its agent is not launchable, signed out, or not listed, or its
  model or effort is not listed. It is not sent, and the message goes without a
  choice, as an untouched picker does. A choice the catalog has not confirmed yet
  is treated the same way until it is confirmed. The saved choice is kept until
  the user changes it.
- **A daemon that cannot take a choice blocks the send, and never drops it
  (DPES-1105).** When the connected daemon lacks the `conversation_selection`
  capability and a choice is saved, a Global or workstream message is blocked,
  never sent without the choice. The picker's note says why, and the blocked
  message says to open model settings and choose "Use the daemon default", or to
  update the daemon. That control removes the saved choice, so the next send goes
  without a selection. Nothing else clears a saved choice, so a reconnect to a
  daemon that takes one sends the saved choice again.
- **Coordinator notes carry no choice (DPES-1105).** A note runs no agent, so the
  desktop sends `coordinator.send_message` without `selection` and without
  `permission`, whatever the picker holds. A note leaves the saved choice and the
  level as they were. The daemon's send-message params deny unknown fields, and
  the desktop source refuses a choice or level for a note with `invalid-params`
  before any request is sent, so a note never carries one.
- **Replaces the configured default.** A message with a choice ignores
  `AGENT_ORCH_CONVERSATION_*` for that message. A chosen "Default model" means
  the agent's own default, which is what the picker shows.
- **Validated by the catalog.** The chosen agent must be `codex`,
  `claude-code` or `github-copilot`. `pi` has no chat adapter and is refused
  with `selection_agent_unsupported`. The model and effort go through the
  catalog's `validate` with its reason, as in the M7-09 catalog. A refusal is a
  failed turn with that reason's code. The message stays in the transcript, and
  no agent is launched and no other agent answers.
- **Copilot is chosen like the others, with its own limits (DPES-1105).** A
  Copilot choice runs when the CLI is available and advertises the required
  command options. A missing CLI fails the turn with `runtime_unavailable`.
  The Copilot CLI cannot list models or efforts without a billable call, so its
  catalog offers neither: a model is refused with `selection_models_unsupported`
  and an effort with `selection_effort_unsupported`. A choice with neither runs
  the default model from the user's Copilot settings. A send never starts a
  sign-in check. The sign-in record comes from earlier runs and from the
  explicit check. A run that shows the account signed out fails its turn with
  `selection_authentication_failed`, and one that shows no Copilot seat fails
  with `selection_agent_unavailable`. Those records are kept, so the next send
  and the picker give the same reason. A Copilot message uses premium requests
  or AI credits on the user's Copilot seat, and the picker says so.
- **Malformed is a bad request.** An empty agent, a model or effort that fails
  the contract's id and effort rules, or an unknown field is `InvalidParams`
  and records no turn. The picker never sends one.
- **Omitted means today's behavior.** Without `selection`, the configured
  default path runs unchanged, and the request serializes exactly as before, so
  its idempotency fingerprint is unchanged.
- **Capability.** The desktop sends `selection` only when the daemon advertises
  `conversation_selection`, which an older daemon does not. Against one, the
  composer blocks a non-default choice with a message rather than dropping it.
- **Launch and pin.** A chosen agent is launched with the same adapter
  arguments as a configured one, and its identity is pinned and recorded for
  that turn like any other.

Tests: in-process (`api_conversations_tests.rs`, `api_conversation_agent.rs`),
process level with `examples/fake_agent.rs` (`tests/conversation_runtime.rs`),
desktop e2e (`apps/desktop/e2e/workspace-conversation.e2e.test.tsx`), and one
opt-in live test (`AGENT_ORCH_LIVE_CONVERSATION=1 cargo test -p
orchestrator-daemon --test conversation_runtime -- --ignored`).

## Clear history and remove coordinator (DPES-1105)

**Decision (user, binding).** A user can clear one conversation's history
permanently, from the Global orchestrator or a project coordinator. A project
coordinator can be removed and later restored. The project and its tasks are
never touched. There is no "clear all".

**The amended append-only rule.** Migration `0032_conversation_clear.sql`
replaces the delete triggers on `conversation_entries` with a gate. A delete from
`conversation_entries`, `conversation_turns` or `coordinator_entries` aborts
unless the single row of `conversation_clear_gate` is open. Only the store's
clear transaction opens it, inside `BEGIN IMMEDIATE`, and closes it before
commit. A rollback closes it too, because the gate is an ordinary row. Updates
still abort. The triggers protect the daemon's own code paths; anyone who can
write the database file can still edit it directly. A clear is the only deletion
of conversation content.

**Permanence.** The connection runs with `secure_delete=ON`, so deleted rows are
overwritten with zeros. Each clear ends with `PRAGMA wal_checkpoint(TRUNCATE)`,
and so does every open after migrations. A test scans every file in the data
directory after a clear and finds no message or reply text. A busy checkpoint is
not an error, and then old WAL frames stay until the next checkpoint.

**Audit.** Migration `0033_conversation_audit.sql` adds `conversation_audit`,
which is append-only: `UPDATE` and `DELETE` both abort. Each clear, remove and
restore writes one row with its action, its scope (`conversation:global`,
`conversation:workstream:<id>` or `coordinator:<project_id>`), its actor, the
number of entries and turns deleted, and a timestamp. The row never holds content.
The actor is always NULL. The row does not yet record which session made the
call; the approver credential decides only whether the call is allowed. The
clear is not written to `durable_events`, which requires a task id.

**Idempotency and text.** Migration `0034_idempotency_scrub.sql` rewrites every
cached `conversations.send`, `coordinator.send_message` and `coordinator.propose`
result to references: `entry_id`, `entry_sequence`, `entry_created_at`, and for
conversations `turn_id` and `epoch`. Message bodies leave
`idempotency_records.result`. Every send response, including the first, is
rebuilt from the store.

*Fingerprint policy.* A fingerprint stays while its message exists. A replay
uses it to detect a reused key, and it reveals nothing the entry does not already
expose. A clear replaces the fingerprints of the sends it deletes with
`CLEARED_FINGERPRINT` (64 zeros), because a hash of a short message can be
checked against a guess. A retry with such a key returns the cleared reference
whatever its text, so the key stays spent. The runtime never runs again for it.

*Replay of a cleared message* returns `{id, sequence, created_at, cleared: true}`
as its entry, with no body, and `"turn": null`.

*Known residual.* A send that crashed after claiming its key and before saving
its result has no result to link to an entry, so a clear cannot tombstone its
fingerprint. The fingerprint is a hash, not text.

**Concurrency.** `conversations.clear` is refused with `conversation_busy`
(`RpcError::ConversationBusy`, code -32019) while a turn is running. The check
and the delete share one transaction, so a turn cannot start between them.
Refusals are cached for their key, as every failed mutation is, so a retry needs
a new key. Coordinators have no turns, so they are never busy.

**Cursors.** Each scope has a `clear_generation`. `conversations.get` and
`coordinator.get` return it, and it rises with every clear and every removal. A
reader that sees a new value discards what it holds and reads again from the
start. For a conversation, a cursor ahead of the newest entry is `ReplayGap`;
the desktop treats that as a reset as well, not as an error.

**Coordinators.** Migration `0032` adds `coordinator_scopes` with a `removed`
flag and a `clear_generation`. A coordinator is opt-in: a project has one only
while its row has `removed = 0`. Registering a project only makes it available
for work and adds no coordinator. `coordinator.restore` adds one (shown as
"Add coordinator"), and it starts empty. `coordinator.remove` deletes the log and
sets `removed`. `coordinator.clear` empties the log. Sends, proposals and reads
refuse a project without a coordinator with `not_found`. `projects.list` returns
`coordinator_active` for each project. Migration `0053_coordinators_opt_in.sql`
removes every coordinator that existed before this rule, with its notes, and
writes one `remove` audit row for each one it removes.

**Methods and capability.** The methods are `conversations.clear
{target, idempotency_key}`, `coordinator.clear {project_id, idempotency_key}`,
`coordinator.remove {project_id, idempotency_key}` and
`coordinator.restore {project_id, idempotency_key}`. They need the capability
`conversation_history`, which older daemons do not advertise. A desktop that
asks for it against an older daemon degrades: the handshake retries without it,
and the actions are disabled with a reason. Each method is idempotent by key and
by state, and reports what it changed.

**Selection capability.** `conversations.send` with `selection` needs
`conversation_selection` (DPES-1105). An older daemon does not advertise it, and
the desktop then blocks a non-default choice instead of sending the message
without it. A send with no choice needs no capability.

**Permission levels (DPES-1105).** `conversations.send` takes an optional
`permission`: `restricted`, `extended`, `auto` or `full`, the level the message
runs at. Rules:

- **Default and wire.** Absent means `restricted`, as a request from before the
  field existed. The desktop chooses `auto` by sending it. An older daemon
  rejects an unknown field with `InvalidParams` and runs nothing, so a client
  must not send `permission` to one. Capability `permission_levels` (daemon
  wide, like `conversation_selection`) says this daemon accepts the field.
- **Mapping.** Each agent's argument builder maps the level to its flags. The
  full per-agent table is in `docs/design/agent-permission-levels.md`, under
  "Cross-agent argv by level". PR A maps chats for Codex, Claude Code and
  Copilot. Extended has no chat mapping yet on any agent, so it is refused.
- **Refused levels run nothing.** A turn whose level has no mapping is recorded
  as failed with `permission_level_unsupported`, with no level recorded and no
  agent launched. No run falls back to a looser level.
- **Recorded.** A launched turn records `permission_level` and
  `permission_profile` (for example `codex:auto:v1`), and both appear on the
  turn in `conversations.send` and `conversations.get`. The level is written
  once the agent has started, so a turn that never launched records none.
  Migration `0035_conversation_permission.sql` adds both columns as nullable,
  and a NULL row is a turn from before permission levels.
- **Codex.** `--skip-git-repo-check` and `--ephemeral` stay at every level,
  Full included. Neither loosens anything, and the per-turn directory is not a
  repository.
- **Copilot.** `--max-ai-credits 30` is passed only when the credit cap is on.
  The daemon reads `AGENT_ORCH_COPILOT_CREDIT_CAP` once at start, and the value
  `1` turns the cap on. It is off by default, and then no Copilot run carries
  it. The cap is soft and 30 is the minimum the CLI accepts. The user has not
  chosen the value 30.
- **Denials.** A denied action does not run, and the turn still completes with
  its reply. No flag makes an agent continue past a denial. The denial codes
  `permission_review_denied` and `permission_review_unavailable` are defined,
  but **no code emits them yet**. How the CLIs report a denial or a failed
  review is unverified, and a parser written before that is known would test
  only itself. The live check in the design file decides this. It is a follow-up, not a gate,
  and it has not run. Tasks follow the same rule.
- **Not passed.** Codex `--ignore-user-config` is not passed to chats. It would
  drop the user's model and provider settings from `~/.codex/config.toml`, and
  that is unverified. Tasks do pass it (see "Task permission levels" below).

**Task permission levels (DPES-1105, PR C).** `tasks.submit` takes an optional
`permission` with the same four values. Its rules:

- **Default and storage.** Absent means `restricted`, both on the wire and in
  the stored contract (`contract_json`). A Restricted contract omits the field,
  so every task stored before this change reads back as Restricted.
- **Codex tasks change.** Before this change a Codex task ran
  `exec --json --approve-for-me` with no sandbox pin. Now a task with no level
  runs as Restricted: `exec --json --sandbox workspace-write
  --ignore-user-config`, with no `--approve-for-me`, so an approval request has
  no reviewer and is refused. Auto runs `exec --json --sandbox workspace-write
  --approve-for-me --ignore-user-config`, the closest equivalent of the old
  argv with the sandbox pinned. Full runs `exec --json` with the user's config.
- **Claude and Copilot tasks.** Restricted keeps the argv these tasks already
  had. Extended, Auto and Full are mapped per the design file. Claude Auto
  denies `git push` and `gh`. Copilot Auto denies `git push` and sends shell
  requests with no allow rule to the judge.
- **Extended on Codex is refused.** `tasks.submit` returns `InvalidParams` and
  creates no task. The diagnostics log records `permission_level_unsupported`.
  The RPC error carries no code, because `RpcError` has no variant for it.
- **Spend caps.** No Claude task run passes a spend cap, and no switch for one
  exists yet. Copilot task runs follow the same switch as chats: they pass
  `--max-ai-credits 30` only when `AGENT_ORCH_COPILOT_CREDIT_CAP` is `1`.
- **Recorded.** A launched task writes an `agent_launched` diagnostics event
  with `permission_level` and `permission_profile` (for example
  `codex:auto:task:v1`). Tasks need no migration. The stored contract carries the
  level the task was submitted with, which is the level it launches with.

**Not covered.** Workstream conversations can be cleared through the API, but
the desktop does not offer it. Single messages cannot be deleted. No retention
policy is applied; history is kept until someone clears it.

## Action approval and execution (DPES-1105, PR 1c)

**Who may propose.** Actions exist only for turns at `restricted`, whatever the
chat level is set to: the chat level does not widen authority. At `auto` and
`full` the agent has a shell, so it could read the daemon's local token and call
`actions.resolve` itself. The approver credential refuses that call from a
session that does not hold the secret, but it is not a sandbox, so those turns
still get no actions. See "Approver credential" below.

**States.** A proposal moves `pending` to `approved` (approve) or `rejected`
(reject), then `approved` to `executed` or `failed`. A pending proposal can
also become `superseded` (a changed request replaces it). A rejected parent fails
its pending dependents with `dependency_not_executed`; a superseded parent
supersedes them. `expired` is never entered. Every transition is a conditional update, `WHERE state = ?`, and
writes one event row with actor `user`. A transition that loses the race is
`proposal_not_pending`.

**Dependents.** An `objective.add` that names a workstream proposal by ref
depends on it. Approving it before the parent has `executed` gives
`proposal_invalid`, and nothing changes.

**Failure codes.** A failed execution stores one of `stale_revision` (the
workstream or objective moved on since the proposal read it), `not_found`,
`conflict` (an archived workstream, an illegal transition, a task already
linked), `invalid` (the proposal no longer fits its scope or names an empty
change), `dependency_not_executed`, or `internal` (a store failure inside the
change itself). A store error while the approval, a claim or an outcome is
written leaves the proposal `approved`, and the next start retries it. The
failure is logged with a path-free cause, and the daemon still starts.

**One run, one key.** Approve and reject run under the caller's key, through
the shared idempotency executor. `actions.resolve` holds the idempotency gate
for the whole call, so no clear or workstream write lands between approve and
execute. The change itself runs under its own key, `action-` followed by the
hash of the proposal id, claimed under the `workstreams.*` method it uses. Its
cached outcome holds only `result_ref` or `error_code`, so a replay tells
`stale_revision` from `conflict` without re-running anything. Writes made by an
action record the actor `action:<proposal id>` in the revision history.

**Replays.**

- The same key twice returns the cached result.
- A second key for an approved proposal gets `proposal_not_pending`.
- Reject then approve gets `proposal_not_pending`.
- Approve after the conversation is cleared gets `proposal_not_found`, and so
  does a replay of that key.
- The approval allowlist refuses an agent actor, with nothing changed. That is a
  unit-level check (`the_approval_allowlist_refuses_an_agent_actor`), not a wire
  guarantee. Every wire session acts as the local user, so on the wire the
  approver credential decides who may call `actions.resolve`.
- A digest that does not match refuses with `proposal_invalid` and changes
  nothing.

**Refusals are cached.** `proposal_invalid` (including a digest mismatch) and
`proposal_not_pending` are cached under the caller's key like every refusal, so a
retry needs a new key. The desktop already uses a new key after any definite
outcome.

**Restart.** Before the daemon serves a request, every `approved` proposal is
executed, oldest first in insertion order. An execution claim that a stopped
process left open is checked against the store first: if the record it would
create or change is already there, the claim is completed with that record;
otherwise the change runs once. If that check fails, the proposal stays
`approved` and the next start retries it. A failure on one proposal is logged
and the rest still run; startup never fails because of it. Pending proposals are
never touched at startup.

| Stopped after | Store state | Next start | Same user key on retry |
| --- | --- | --- | --- |
| Key claimed, before the approval committed | `pending` | Nothing to do | `idempotency_in_progress` |
| Approval committed, before the user key completed | `approved` | Executes the change | `idempotency_in_progress` |
| User key completed, before the change was claimed | `approved` | Executes the change | Cached success, `executed` |
| Change claimed, before its effect committed | `approved` | Runs the change | Cached success, `executed` |
| Effect committed, before its claim completed | `approved` | Records the existing record | Cached success, `executed` |
| Claim completed, before the outcome was recorded | `approved` | Replays the cached outcome | Cached success, `executed` |

The retry column shows what the same user key returns. A key whose process
stopped before its outcome was recorded returns `idempotency_in_progress`,
which every method does today. A new key gets the proposal's real state after
the restart. The desktop's per-click keys (DPES-1105, phase 1d) keep the retry
case rare.

**Snapshot.** The next turn's snapshot lists each recent proposal with its
state, and with its outcome: `executed` shows the record it created, and
`failed` shows the code. A block in a workstream title or in a reply is never
approved by the agent; only `actions.resolve` changes a proposal.

**Event and audit.** `action_proposal_events` gets one row per transition, with
actor `user`. Clearing a conversation deletes its proposals, and their events
stay: the events table is append-only, has no foreign key, and holds digests
and codes, never parameters or text.

**Tests.** `crates/orchestrator-daemon/src/api_actions_tests.rs` covers each
kind, replays, refusals, digest checks, stale revisions, dependencies, a
pending proposal kept past the settled cap, a restart between approve and execute,
and an update and a revise whose effect committed before a stop.
`crates/orchestrator-daemon/tests/conversation_runtime.rs` runs approval through
the real daemon, including the capability refusal (`-32004`), the snapshot
outcome, a restart between approve and execute, and the same outcome at each
permission level.

### Approver credential

Without the approver credential, any process that held `daemon.token` and the
socket could call `actions.resolve`, and an Auto or Full chat turn has a shell.
The approver credential closes that protocol path. It is not a sandbox.

The desktop host creates the secret and gives it to the daemon it starts. Only a
session that presents the secret is an approver.

- The host generates 32 random bytes from the OS random source. It starts the
  daemon with `--approver-stdin`, writes the secret as 64 hex characters to the
  daemon's stdin pipe, and closes the pipe.
- The daemon reads the secret once, before it creates any server resource, and
  keeps it in memory only. It is never written to disk, an argument, an
  environment variable or a log. The daemon does not read stdin again.
- The host presents the secret in the `approver` field of the handshake, and only
  to a daemon it spawned itself. The daemon compares it in constant time.
- The welcome carries `approver: true` or `approver: false`. The webview never
  receives the secret.
- A missing, wrong or malformed `approver` does not fail the handshake. The
  session connects as a non-approver and keeps read access. A session cannot
  become an approver later, because a second handshake is refused.
- A daemon started without `--approver-stdin` has no secret, so every session on
  it is a non-approver.

The gate refuses the methods in `USER_ONLY_METHODS` to a session that is not an
approver. The list is `actions.resolve` and `approvals.resolve`. The refusal is
`Forbidden` (-32012). It comes before any handler runs and before the idempotency
lookup, so a non-approver cannot replay a key to read a cached approval. A session
that did not negotiate the capability still gets `-32004` first. Later phases add
`settings.set` (phase 2), `monitoring.set` (phase 3), `projects.set_defaults`,
`holds.set`, `holds.release`, `reports.get` and `reports.relay` (phase 4), and
`grants.create` and `grants.revoke` (phase 5), each in the same change that routes
the method.

The credential does not enable actions at Full. Actions run at Restricted and Auto.

What it does not stop. A process of the same user can still:

- edit `store.sqlite` directly, and mark a proposal approved without any RPC;
- kill the daemon and start its own on the same data directory, with its own
  secret;
- call `shutdown`, which any token holder may do (the host forbids it for the
  webview);
- harvest the secret by swapping the socket the host connects to. A fix needs a
  server nonce, and that is deferred;
- on Windows, read the memory of the daemon or the host.

The token also still opens `tasks.submit`, `tasks.cancel`, `tasks.retry` and
`workstreams.*`, including gate changes. So the proposal approval gates only
Restricted chat proposals. No wire path lets a process that holds only the token
approve or decide for the user.

A host that did not spawn the running daemon is not an approver. This happens,
for example, after an app upgrade while the old daemon still runs. That session
can still read and cancel, but it cannot approve or reject. The daemon exits
about 60 seconds after its last client leaves. A daemon that a host then spawns
is an approver. The host has no command yet to restart such a daemon. On Windows
the app exits with its last window, so the secret is lost while the daemon keeps
running. The action card and the Review view say so in plain words.

## Task proposals (DPES-1105, phase 2)

Phase 2 adds four kinds to the action block, a fixed envelope that a proposed
task must pass, and the read method `actions.pending`. Everything above holds
unchanged: actions exist only for a Restricted or Auto turn, nothing happens until the
user approves the card, and the approver decides on the card only.

### The four kinds

A block may mix these with the workstream and objective kinds, at most ten actions
in all. A field the kind does not list rejects the whole block.

| Kind | Parameters | What approval does |
| --- | --- | --- |
| `task.propose` | `project`, `objective`, `agent`, `task_kind`, `delivery`, and optionally `workstream_id` or `workstream_ref`, and `objective_id` | Creates one task and queues it. When a workstream is named, links the task to it, and to the objective when `objective_id` is set. In the global conversation with no workstream named, links it to the "Global tasks" workstream, made on first use; an archived one leaves the task unlinked. |
| `task.cancel` | `task_id` | Cancels a created, queued or verifying task at once. A running task ends the proposal `executed` with the text "Cancellation requested", because the task stops when its agent confirms. A finished task fails `conflict`. |
| `task.retry` | `task_id` | Queues one new attempt of a failed or verification-failed task. A running or queued task fails `conflict`, and a running attempt is never cancelled or restarted. |
| `workstream.link_task` | `workstream_id`, `task_id`, and optionally `objective_id` | Links an existing task to the workstream. A task linked elsewhere fails `conflict`. |

The `task.propose` fields:

- `project` is the `p-` code the snapshot shows after a project's folder name, `p-`
  and eight lowercase hex digits. The daemon turns it into a project when the card
  is approved: no project is `not_found`, two projects with one code are `conflict`.
  A path is never accepted and never stored.
- `objective` is at most 8 KiB of text with no control characters other than tab
  and line breaks.
- `agent` is `codex` or `claude-code`.
- `task_kind` is `Implement` or `Investigate` and defaults to `Implement`.
- `delivery` is `ReportOnly` or `LocalReview` and defaults to `LocalReview`.
  `PullRequest` parses, so the envelope can refuse it and the user can see why.
- `workstream_id` names an existing workstream. `workstream_ref` names a
  `workstream.create` earlier in the same block, and the task then depends on that
  card as an `objective.add` does. A block names at most one of the two, and
  `objective_id` needs `workstream_id`.

Both defaults are written into the stored parameters, so the card shows what would
run and a request that omits `delivery` matches one that says `LocalReview`. A
proposal never names autonomy, verification, permission, attempt limits,
dependencies or a parent task. The envelope and the daemon fix them.

### The chat envelope

A task proposed from chat may ask for no more than the fixed `chat_task_envelope()`
in `orchestrator-core`:

- Autonomy `ApprovalRequired`, taken from the envelope, never from the agent.
- Verification `None`: no verification commands run from chat.
- Delivery `ReportOnly` or `LocalReview`. `PullRequest` is not granted. The user can
  still submit one by hand in the Agents view.
- One retry and one repair, the desktop form's defaults, and no dependencies.
- No permission is set, so the task takes the daemon default, Restricted. Chat can
  never raise autonomy, verification or permission.

`ReplySink::block` checks each `task.propose` against the envelope when the turn
completes: after the block parses and the scope checks pass, it calls
`ActionBlock::checked(&chat_task_envelope())`. The store never learns policy; it
reads the verdict for each action. Today the envelope is a constant. When it comes
from settings, `actions.resolve` must check a task again on approve.

### A denied proposal

A task the envelope refuses is stored at once as a `failed` proposal with
`error_code` `action_denied` and a `denial` key:

| `denial` | Why |
| --- | --- |
| `autonomy_not_authorized` | The proposal asks for more autonomy than the envelope grants. |
| `verification_not_authorized` | The proposal asks for verification the envelope does not grant. |
| `delivery_not_authorized` | The delivery is outside the envelope, such as `PullRequest`. |
| `contract_invalid` | The task contract does not validate, such as an empty objective. |

The agent chooses neither autonomy nor verification, so a real block can only reach
the delivery key and the contract key. The other two keys are tested with synthetic
envelopes in `orchestrator-core`.

A denied proposal is a record, not a decision:

- It is terminal. `actions.resolve` on it returns `proposal_not_pending`
  (-32022), and execution only ever runs from `approved`, so it never creates a
  task. There is no new state and no new RPC error.
- It gets two events: created `pending`, then `pending` to `failed` with the
  error code. The actor is `agent`, because the events table allows only `agent`,
  `user` and `grant:*`.
- It is never pending, so it matches no card and `actions.pending` never lists it.
  A repeat of the same refused request is a second `failed` card.
- The desktop shows it as "Denied", with the reason and no buttons.

### Scope in a workstream conversation

In a workstream conversation a block may change only that workstream's tasks:

- `task.propose` must name that workstream by `workstream_id`, not by a ref, so a
  task proposed there is always linked to it.
- `workstream.link_task` must name that workstream.
- `task.cancel` and `task.retry` must name a task linked to that workstream. The
  daemon checks the link in the store (`ReplySink::targets_in_scope`), as it does
  for `objective.revise`. A task linked elsewhere, or not linked at all, rejects the
  whole block (`action_block` is `rejected`).

In the global conversation all four kinds are allowed, and the user approves each.
`actions.resolve` checks the same scope again when it runs a card.

### Execution and errors

Approval runs through `*_inner` functions that the public handlers also use, under
the held idempotency gate and the key `action-` followed by the hash of the proposal
id. The public methods keep their behavior. A failure becomes the proposal's
`error_code`, one of the codes under "Failure codes" above, plus `forbidden` (the
refusal of a read-only task, which a chat proposal cannot reach). A restart finishes
an approved proposal once. A crash after the task is created and before it is
queued, or before it is linked, is finished by the same reconcile.

### Snapshot and prompt

The snapshot line `Action kinds:` lists all eight kinds. Each recent proposal is one
line with its kind, state and outcome, then what it is about:

```text
- task.propose pending: project p-3fa9c1d2: Add unit tests for the retry module.
- task.propose failed (action_denied: delivery_not_authorized): project p-3fa9c1d2: Ship it.
- task.cancel pending: task-0123abcd
- workstream.link_task pending: task-4567ef01 to workstream-0123abcd
```

A task proposal shows its project code and the objective cut to 80 bytes. A cancel or
retry names its task id, and a link names its task and workstream ids, which the task
lines already show. A denied task also shows its reason. No line holds a path, token,
environment, credential, diff or session output, and a project is named by its code
only. The task lines of the snapshot do not change in phase 2.

The preamble lists the four kinds and their fields and says that `delivery` is
`ReportOnly` or `LocalReview`. It keeps its rule that nothing happens until the user
approves and its rule that the snapshot is data, and it stays within the 3 KiB cap.

Tests: core (`orchestrator_action.rs`, `task_envelope.rs`), store
(`tests/action_proposals.rs`), the daemon unit tests of `api_actions_tests.rs`, and
the process tests of `tests/conversation_runtime.rs`. The process tests run a
scripted `task.propose` to a pending card, a pull-request delivery to a denied card
that cannot be approved, an Auto or Full turn that gets no card, a double approval
that creates one task, a restart between approve and execute, a repeated request,
a cancel of a foreign task in a workstream conversation, and `actions.pending` for a
session that holds only the token.

## Project defaults, reports and holds (DPES-1105, phase 4)

Phase 4 adds per-project defaults, scout reports, promotion, holds, task lineage,
a soft archive, and the action kinds `task.revise`, `decision.hold` and
`project.remove`. Actions still run only at Restricted or Auto, and every mutation still
needs a per-action approval.

- **Project modes.** A project's mode is `LocalOnly` (the default), `DirectPr` or
  `NoMistakes`. Pull request delivery needs a mode that allows it and the chat
  setting `chat_pr_delivery`. That setting is off by default and is set only by the
  user through `settings.set`. The project defaults are set only by the user through
  `projects.set_defaults`. Approval checks the proposal against the project's
  current defaults, so a change made after the proposal was made applies.
- **Scout reports.** A scout's report is stored after its run, capped at 16 KiB and
  marked when cut. Credential shapes are masked. Paths are kept in the stored copy.
  A report is never deleted in this phase.
- **Relay.** A ready report automatically queues a Restricted wake turn in Global,
  subject to monitoring mode, caps, load gating and user-turn precedence. A user can
  also request a relay from the notice. The copy handed to the model goes through the same
  word-level redactor as the wake service: paths become `[path]`, and token-like
  words become `[redacted]`. The model receives it as labelled data, with at most one
  report per wake turn. Automatic relay sets `relayed_at` only after a successful,
  non-quiet reply. Failure, cancellation and a quiet reply do not mark it relayed.
  A failed or quiet relay is retried once. After the second failed or quiet reply, the
  notice reads that the automatic relay failed, and the report stays unrelayed.
- **Methods.** `reports.get` and `reports.relay` are user-only. `reports.list` returns
  metadata only and is open.
- **Promotion.** A scout promotes to a ship only after its report has been relayed,
  which sets `relayed_at`.
- **Holds.** `decision.hold` puts a hold on a decision. `holds.set` and `holds.release` are
  user-only. The hold date is from today to 60 days ahead by the daemon clock, and
  the hold lasts until the end of that day in UTC.
- **Lineage.** A task created by `task.revise` or by promotion links to the task it
  came from. A link is written once, never changed, and never deleted. The store
  refuses a link that would close a cycle.
- **`task.revise`.** Asks the old task to stop and admits a new task with the
  instruction added to the objective. The load gate and the worker limits still
  apply. Never grantable.
- **Soft archive.** `project.remove` sets `projects.archived_at`. No row is deleted.
  `projects.register` for the same root clears the marker. Never grantable.

## Standing approvals (DPES-1105, phase 5)

A standing approval, or grant, lets the daemon run a matching proposal without a
per-action card. Only the user creates one. An agent never creates, changes or
revokes a grant.

- **Creation and revoke.** `grants.create` takes one scope, a set of kinds, an expiry,
  a use count and a token budget. `grants.revoke` takes effect at once. Both are
  user-only. `grants.list` is open. A grant is never edited: to change its terms,
  revoke it and create another.
- **Scope.** One project or one workstream, never all projects. A workstream grant
  covers a task on any of its member projects, even one not linked to the workstream.
- **Grantable kinds.** The code's `GRANTABLE_KINDS` list has six kinds:
  `workstream.update` (title and summary only, never a change of `state`),
  `objective.add`, `objective.revise`, `task.propose`, `task.retry` and
  `workstream.link_task`. A grant names at most 16 kinds. `decision.hold` and
  `workstream.create` are not grantable in this build.
- **Never grantable.** The code's `NEVER_GRANTABLE` list has three entries:
  `project.remove`, `task.cancel` and `task.revise`. `delivery.merge` is covered only by
  a merge grant, which may name no other kind (see `orchestrator-actions.md`).
- **Expiry.** Required. Default 8 hours, maximum 7 days, minimum 60 seconds.
- **Uses.** Default 20, maximum 200.
- **Token budget.** Required, from 1 to 1,000,000,000 tokens. It counts the tokens
  that usage receipts report for tasks the grant started, from the grant's creation.
  A figure that was never reported counts as 0. A reached budget or use limit blocks
  new uses only. It never cancels a running task.
- **Matching.** The matcher checks for new proposals every 2 seconds. A grant never
  matches a proposal made before the grant was created.
- **Use.** A match is approved with the actor `grant:<id>`. The use is consumed in
  the same transaction as the approval. Each use writes an audit event with the grant
  id and the proposal digest. The approval runs the same re-check against the current
  defaults and the same execution as a user approval.
- **Revoke and expiry.** A revoke is immediate. An execution under a grant stops before
  it starts if the grant is revoked, expired, out of uses or out of budget at that
  moment. A use already spent is not refunded.
- **Merge grant (N8).** The unattended merge grant is built with the pull request
  poller and the merge action (phase 4c). Its rules and the checks that still need a
  live `gh` are in `orchestrator-actions.md`, "Pull request watching and merge".

### Away mode and the return brief

- Away is a monitoring mode, set by the user through `monitoring.set`. It adds no
  authority. Grants apply in every monitoring mode, and proposals still need a card
  unless a grant matches.
- The return brief covers the window from the moment away mode started to now. It
  holds counts, ids, short titles, states and project codes. It never holds free text
  from an agent, and it is never put in a prompt.
- The brief is kept until the user acknowledges it with `monitoring.brief_ack`. That
  method is open to any session and only marks the brief as seen.

## Orchestrator notes (DPES-1105, phase 6)

Notes are short reminders the user writes for the orchestrator, such as a branch
name or a standing fact. The backend and the desktop editor are built.

- Only the user writes a note. No action kind writes one, and `notes.set` is
  user-only.
- A scope holds up to 10 notes. The scopes are the global conversation and one
  workstream. Each note is at most 200 bytes.
- A note is refused, not stripped, when it is empty, too long, has a control
  character, or looks sensitive. Sensitive means a path, a URL with credentials, or a
  token-shaped string. The check is a pattern list, not a guarantee. The snapshot
  redactor is a second layer.
- The snapshot shows the notes as a delimited block of data, never as instructions.
  A workstream conversation sees its own notes, then the global notes.
- Wake turns do not see notes.
- A notes change touches no autonomy, grant or approval.
- Clearing a conversation's history deletes that scope's notes. Clearing the global
  conversation deletes the global notes, which every workstream conversation lists.
- The `notes.set` reply carries no note text, because replies are cached.

## Not covered here

Push streaming, native macOS/Windows end-to-end, and Windows Job Object
cleanup for the catalog and version probes.
