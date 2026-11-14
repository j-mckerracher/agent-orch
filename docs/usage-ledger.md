# Usage ledger and `usage.summary` (M7-06)

Plan: /Users/mckerracher.joshua/Code/agent-orch-worktrees/mckerracher-joshua-mclm-friendly-guide/docs/technical-implementation-plan.md
(gap analysis D-11: cost is shown as unknown until reported, never as zero).

## What adapters report (spike)

| Adapter | Tokens | Cost | Source of the figures |
| --- | --- | --- | --- |
| Codex (`exec --json`) | yes: `turn.completed.usage` (`input_tokens` includes `cached_input_tokens`; `cache_write_input_tokens`, `output_tokens`, `reasoning_output_tokens`) | never | recorded runs in `docs/evidence/m1-05-macos/` |
| Claude Code (`stream-json`) | yes: terminal `result.usage` (`input_tokens` excludes cache) | yes: `result.total_cost_usd`, a list price computed by the CLI, not provider billing | recorded run `docs/evidence/m4-02-macos/20261107T215309-0700.stdout.jsonl` |
| Pi / OMP RPC | expected: assistant `message_end.message.usage` | expected: `usage.cost.total`, USD, computed by the runtime | **Not verified against a live stream.** No real Pi stream exists in this repo. The field names match the `Usage`/`AssistantMessage` types in the installed Pi 1.5.0 docs (`docs/message-types.md`; `reasoning` is already inside `output`, so it is not read) and an OMP session file; the recorded handshakes carry only a price table, and the tests use synthetic JSON and a fake RPC child. Pi figures must not be trusted until a recorded `message_end` is replayed in `usage_replay.rs`. |

Before M7-06 every adapter discarded this data.

## Normalisation

`UsageReport` (crate `orchestrator-agents`, `usage.rs`) is taken from an
adapter with `AgentHandle::take_usage()`. `input_tokens` is fresh input only
(Codex cached tokens are subtracted), `cache_read_tokens` and
`cache_write_tokens` are separate, `output_tokens` includes reasoning and
`reasoning_tokens` is informational. Cost is an integer in nano-units of a
currency. A report with no tokens and no cost is dropped. A zero cost or an
all-zero token count is treated as unknown, because a runtime that has no price
or no figures cannot be told apart from a free call.

No price table exists in the agents crate. Estimated cost is derived in the
store crate at summary time (see "Estimated cost" below); `estimated` receipts
written by something else are still supported by the schema and summary.

## Persistence

Migration `0020_usage_receipts.sql` adds `usage_receipts`. A receipt is unique
per `(task_id, session_id, source, receipt_key)`. Replaying the same
observation is a no-op; a revision replaces it only when its `reported_at_ms`
is strictly newer. A receipt is attributed to the day of its `window_end_ms`, so
a receipt that arrives late lands in the day it belongs to. The migration only
adds a table, so existing data directories upgrade in place.

The daemon executor records receipts while it supervises the agent: after every
event, at session exit, when the event stream ends or fails, and after a
cancellation, so a cancelled run keeps whatever it had already reported (for
Claude Code, the cost on its `result` event only appears if the run got that
far; otherwise the cost stays unknown, never zero). Model text is stored cut to
128 bytes.

A failure to record is only logged to stderr and never fails the run. The
receipt is then lost with no durable signal and no counter; the summary shows
that session's figures as unknown or partial, not as zero.

## `usage.summary`

Params: `timezone` (label echoed back), `utc_offset_seconds` (-43200 to 50400),
optional `now_unix_ms`, and optionally both `window_start_unix_ms` and
`window_end_unix_ms` (at most 26 hours) to pin a DST day. The daemon has no
timezone database, so the client supplies the offset, which is applied as fixed
for the window.

Result: `window`, `totals`, `providers[]`. A bucket has `sessions`,
`sessions_with_tokens`, `sessions_with_cost`, `token_status` and `cost_status`
(`unknown`, `partial`, `complete`), and separate `reported` and `estimated`
figures. A token figure that nothing reported is `null`. Costs are
`[{currency, amount}]` decimal strings and are never merged across currencies;
an empty list means unknown. Sessions are counted from their `session.created`
event, so sessions created before the journal existed are not counted.

Method scope: gated by the `agents` capability.

### Estimated cost (M7-26)

A receipt with tokens but no cost (Codex always; Claude or Pi when the cost is
absent) is priced from a small native table, `usage_prices.rs` in
`orchestrator-store`, when the summary is computed. Nothing is written back:
stored receipts keep their cost, source and model, and a reported cost is never
replaced or recomputed. The estimate goes into `estimated.costs` (USD) only;
the tokens are not repeated there, because they are already in `reported`.

* Table: integer nano-USD per token for input, cache read, cache write and
  output, for the Codex/OpenAI and Claude/Anthropic models in our adapters.
  `version` (currently `2026-11-11.1`) changes with any price edit; `basis` is
  `list_price_standard_tier`; `source` names where the prices were taken.
  Prices are list prices, not a bill. Batch, flex, priority and long-context
  tiers are not modelled, and Anthropic cache writes use the 5-minute rate.
  An estimate may therefore understate. Prices are not refreshed at runtime
  (no network); a new table means a new version.
* Unknown stays unknown: a model that is absent or not in the table, or a
  count the price needs that is `null`, gives no estimate. Never 0. A model
  with no cache-write price accepts a cache-write count of `null` or 0 only.
* A session is estimated whole or not at all: if any cost-less token receipt
  of it in the window cannot be priced, the session contributes nothing, so a
  partial sum is never shown as complete. A session that has a stored
  `estimated` cost receipt is not estimated again.
* Model names are matched case-folded, without a `[context]` tag or a trailing
  `-YYYYMMDD`, with `.` read as `-` for `claude-*`. A Codex stream names no
  model, so the daemon records the model the run was **requested** with
  (`model_selection.model`) on Codex receipts that name none. A Codex run
  started without a model selection has no model and stays unknown. This is a
  requested model, not one the provider reported.

Additive fields (a client must ignore unknown fields and tolerate their
absence, as an older daemon does not send them): each bucket adds
`sessions_needing_estimate` (sessions with a cost-less token receipt),
`sessions_with_estimated_cost`, `sessions_with_known_cost` (reported or
estimated), `estimated_cost_status` (estimated vs. needing) and
`known_cost_status` (known vs. all sessions). The result adds
`price_table: {version, basis, source, currency, models}`. `cost_status` keeps
its meaning, reported only.

## Known gaps

* The CLI workflow path does not record usage.
* Pi usage parsing is unverified (see the spike table).
* Claude cost is a client-side list price and may differ from the bill.
* Estimated cost is a standard-tier list price; Codex runs without a model
  selection have no model and stay unknown.
