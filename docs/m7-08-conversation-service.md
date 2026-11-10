# M7-08 — Orchestrator conversation service (design record)

Plan: `/Users/mckerracher.joshua/Code/agent-orch-worktrees/mckerracher-joshua-mclm-friendly-guide/docs/technical-implementation-plan.md`.
Board item: M7-08 (depends on M7-00, M5-02c). Status: backend service and
tests implemented; live runtime adapter and UI binding are separate items.

## Decision summary

| Question | Decision |
| --- | --- |
| Runtime behind the conversation | **Provider-neutral.** The daemon depends on the `ConversationRuntime` trait (`api_conversation_runtime.rs`), not on a product. The OMP/model clauses of `orch-impl-m.md` are overridden by the UI gap analysis. No OMP behaviour is invented. |
| Runtime that ships in this item | `UnavailableRuntime` (name `unavailable`, version `none`). Every turn fails closed with `runtime_unavailable`. A real adapter is bound by M7-24; the catalog and selection are M7-09. |
| Version pin | **Mandatory.** `ConversationService` takes `Option<RuntimeIdentity>` (name + exact version). A runtime that requires a pin and has none gets `runtime_unpinned`. A pin that differs from the runtime's reported identity gets `version_mismatch`. In both cases the runtime never receives the message. The pin value is set by the adapter item together with its evidence; this item pins nothing because it ships no real runtime. |
| Persistence and recovery owner | The **daemon** owns the transcript (SQLite, migration `0030_conversations.sql`). Runtime state is never trusted or resumed. On startup `with_config` marks every `running` turn `failed` with `interrupted_by_restart`. |
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
`reply_too_large`, `interrupted_by_restart`.

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

M7-07 is not merged. This item stores `workstream_id` as an opaque, nullable,
length-bounded text column with **no foreign key**, and resolves it through the
`WorkstreamDirectory` trait. The default `NoWorkstreams` resolves nothing, so
every workstream target returns `NotFound`; global works. M7-07 must (1) supply a
`WorkstreamDirectory` backed by its table and pass it to
`ConversationService::new` in `DaemonApi::with_config`, and (2) optionally add a
foreign key in its own migration. Nothing else changes.

## Concurrency

`start_turn` runs under the global idempotency gate and must not block; real
adapters spawn work and return a control handle. The control registry is only
populated while the store shows the turn `running`; terminal commits happen in
the store first, so a stale control is never left registered.

## Not covered here

Real adapter and its pinned version/evidence, push streaming, client wrappers
and UI binding (M7-24), native macOS/Windows end-to-end.
