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
| Authority | **None.** Conversation methods only read and write conversation rows. Text (user or runtime) is data; it is never parsed into methods. Proposals still go through `coordinator.propose` and engine validation. Every response carries `"authority": "none"`. |

## Data model

- `conversations` is implicit per target: `conversation:global` or
  `conversation:workstream:<id>`.
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
  `EpochMismatch`; an unused conversation reads as empty.
- `conversations.cancel {target, turn_id, idempotency_key}` — scoped to the
  target's conversation (otherwise `NotFound`). Finishes the turn `cancelled`,
  then asks the runtime to stop (best effort). `cancel_applied=false` when the
  turn had already ended.
- `target` is `{"kind":"global"}` or `{"kind":"workstream","workstream_id":"…"}`.
  Unknown fields are rejected.

Error codes on `turn_failed`: `runtime_unavailable`, `credentials_missing`,
`permission_denied`, `version_mismatch`, `runtime_unpinned`, `runtime_failed`,
`runtime_timeout`, `runtime_busy`, `reply_too_large`, `interrupted_by_restart`,
and, for a choice the catalog refuses (DPES-1105), one code per refusal reason:
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
is `NotFound`, and `describe` gives the agent a bounded one-line context
(title, state, summary). A foreign key remains optional for a later migration.

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
  names the agent; without it the first launchable one in catalog order is used
  (`codex`, then `claude-code`). `AGENT_ORCH_CONVERSATION_MODEL` and
  `AGENT_ORCH_CONVERSATION_EFFORT` are checked against the catalog; an option
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
  version are cached for 30 s.
- **Pins.** Known-good versions are compiled in (`codex` 0.167.0 and 0.167.2,
  `claude-code` 2.1.311). 0.167.2 was checked against its `codex exec --json`
  event order and command lifecycle. The operator can add more
  with `AGENT_ORCH_CONVERSATION_PINS=name@version,...`. An unpinned version gets
  `version_mismatch` and never receives the message. `start_turn` compares the
  identity again, but only against the cached selection: an upgrade of the CLI
  inside the 30 s window is not detected.
- **Prompt.** A fixed frame (talk only, no actions), the scope, a bounded
  workstream context, up to 6 earlier completed turns (4 KiB per reply, 12 KiB
  in total, which keeps the Windows command line short) and the current message
  last. The message is passed as one argument after `--`, never through a shell.
- **Process.** Agent arguments for the message's permission level
  (`args_for_conversation`, see "Permission levels" below). Each Codex and
  Claude Code turn runs in its own empty directory under
  `<data-dir>/conversation-scratch`, removed when the turn ends. The adapter's
  `ChildSupervisor` provides a Unix process group (killed as a
  group), a Windows Job Object, and a 64 KiB line cap. Turn deadline 180 s
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
  beyond the reply sink. A reply is data. Nothing is parsed from it.
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
  Copilot choice runs only when the daemon can launch the CLI at the pinned
  version. A missing or unpinned CLI fails the turn with `runtime_unavailable`.
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
The actor is always NULL, because the daemon has no caller identity today. The
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
flag and a `clear_generation`. No row means active and never cleared.
`coordinator.remove` deletes the log and sets `removed`. `coordinator.restore`
clears the flag, and the coordinator starts empty. `coordinator.clear` empties
the log. Sends, proposals and reads refuse a removed coordinator with `not_found`.
`projects.list` returns `coordinator_active` for each project.

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

## Not covered here

Push streaming, native macOS/Windows end-to-end, and Windows Job Object
cleanup for the catalog and version probes.
