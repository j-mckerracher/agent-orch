# M7-08 — Orchestrator conversation service (design record)

Plan: `/Users/mckerracher.joshua/Code/agent-orch-worktrees/mckerracher-joshua-mclm-friendly-guide/docs/technical-implementation-plan.md`.
Board item: M7-08 (depends on M7-00, M5-02c). Status: backend service and
tests implemented in M7-08; the UI binding is M7-24; the real agent runtime is
M7-25 (see "M7-25: the agent runtime" below).

## Decision summary

| Question | Decision |
| --- | --- |
| Runtime behind the conversation | **Provider-neutral.** The daemon depends on the `ConversationRuntime` trait (`api_conversation_runtime.rs`), not on a product. The OMP/model clauses of `orch-impl-m.md` are overridden by the UI gap analysis. No OMP behaviour is invented. |
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
- `conversation_entries`: append-only (update/delete triggers abort). Kinds
  `user_message`, `reply_chunk`, `turn_completed`, `turn_failed`,
  `turn_cancelled`. `sequence` is a per-database monotonic cursor; entry ids are
  deterministic (`<turn>:user`, `<turn>:chunk:N`, `<turn>:end`), so every reply
  chunk is correlated to its message by `turn_id`.
- Bounds: message 8 KiB, chunk 16 KiB (larger deltas are split at character
  boundaries), reply 256 KiB per turn (over the bound the turn fails with
  `reply_too_large` and the runtime is cancelled).
- First terminal state wins. A late chunk after cancel or failure is rejected
  (`SinkClosed`) and never stored.

## RPC surface (proposed names; capability `coordinator`)

Names are proposals so concurrent daemon work does not collide; no new
`Capability` variant was added.

- `conversations.send {target, text, idempotency_key}` — records the message and
  starts a turn. The call succeeds even when the runtime then fails; the failure
  is a `turn_failed` entry plus `turn.error_code`. A second send while a turn is
  `running` is `Conflict`.
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
`runtime_timeout`, `runtime_busy`, `reply_too_large`, `interrupted_by_restart`.

## Ordering, idempotency, replay

- Turn id = stable hash of the idempotency key; a retried send returns the
  stored result and calls the runtime once. Same key with different text is
  `IdempotencyConflict`. The shared idempotency layer caches failures too: a
  rejected attempt (for example `Conflict`) stays rejected for that key, so a
  retry after the turn ends needs a new key.
- A crash between the idempotency claim and its completion leaves the key
  `IdempotencyInProgress` (existing fail-closed behaviour of
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
- **Pins.** Known-good versions are compiled in (`codex` 0.167.0, `claude-code`
  2.1.311, the versions live-checked for this item). The operator can add more
  with `AGENT_ORCH_CONVERSATION_PINS=name@version,...`. An unpinned version gets
  `version_mismatch` and never receives the message. `start_turn` compares the
  identity again, but only against the cached selection: an upgrade of the CLI
  inside the 30 s window is not detected.
- **Prompt.** A fixed frame (talk only, no actions), the scope, a bounded
  workstream context, up to 6 earlier completed turns (4 KiB per reply, 12 KiB
  in total, which keeps the Windows command line short) and the current message
  last. The message is passed as one argument after `--`, never through a shell.
- **Process.** Read-only agent arguments (`args_for_conversation`), working
  directory `<data-dir>/conversation-scratch` (empty, daemon-owned). The
  adapter's `ChildSupervisor` provides a Unix process group (killed as a
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
  gone, or any identity mismatch, is never signalled. A group whose leader
  died first is left alone, as are agents spawned within milliseconds before
  the kill. Windows relies on the Job Object (`KILL_ON_JOB_CLOSE`).
- **Known limit.** A launched agent can read the user's files and runs the
  user's own agent settings and hooks, as any run of that CLI does. The
  read-only sandbox and the empty working directory limit what it changes, not
  what it can read.

Tests: in-process (`api_conversations_tests.rs`, `api_conversation_agent.rs`),
process level with `examples/fake_agent.rs` (`tests/conversation_runtime.rs`),
desktop e2e (`apps/desktop/e2e/workspace-conversation.e2e.test.tsx`), and one
opt-in live test (`AGENT_ORCH_LIVE_CONVERSATION=1 cargo test -p
orchestrator-daemon --test conversation_runtime -- --ignored`).

## Not covered here

Push streaming, native macOS/Windows end-to-end, and Windows Job Object
cleanup for the catalog and version probes.
