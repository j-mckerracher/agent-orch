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

## Normalization

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
cancellation, so a canceled run keeps whatever it had already reported (for
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
  tiers are not modeled, and Anthropic cache writes use the 5-minute rate.
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

## Daily token warnings (DPES-747)

An operator can optionally put `usage-token-warnings.json` in the daemon data
directory. An absent file disables warnings. The file is read on each
`usage.summary` request, so a change takes effect on the desktop's next usage
refresh, on focus, or at local midnight. Example:

```json
{
  "schema_version": 1,
  "rules": [
    { "provider": "codex", "metric": "output_tokens", "limit": 100000 }
  ]
}
```

Each rule names the exact provider ID returned by `usage.summary`, one of
`input_tokens`, `cache_read_tokens`, `cache_write_tokens`, or `output_tokens`,
and a positive unsigned 64-bit integer limit. Input means fresh input; output
already includes reasoning. Categories are evaluated separately. Duplicate
provider/category rules, unknown configuration fields or versions, invalid
limits, more than 32 rules, nonregular files and files over 16 KiB are refused.
Replace the file atomically when editing it to avoid a temporarily incomplete
configuration.

The optional `token_warnings` result has `schema_version: 1`, a `state` of
`off`, `ok`, `invalid_configuration`, or `unavailable`, and `rules`. Each result
rule contains `provider`, `metric`, decimal-string `limit` and nullable
decimal-string `observed`, and nullable boolean `exceeded`. Strings preserve
the complete counter range. Unknown counts produce nulls. The desktop shows
configuration/read failures without hiding the usage figures.

A warning appears only when a recorded count is strictly greater than its
limit. Equality does not exceed. Counts are lower bounds over actual stored
receipts in the same reporting window as the usage summary. Missing receipts
or token categories can make the actual use higher; an unexceeded known count
does not establish that all usage was under the limit. The existing client
supplies the local-day window, including DST boundaries. No separate billing
day is inferred.

Each refresh recomputes current deduplicated receipts. Replays do not add
tokens; later receipt revisions replace earlier values in their original day.
Corrections can raise or clear a warning. The UI renders one row per rule and
does not append repeated notifications or journal events. The warning opens
the usage details and shows provider, category, count, threshold and the
window. It never blocks execution, admission, verification or merging.

This token-only warning uses MCAO-started runs. Other local tools remain a
separate source. A harness with no recorded count remains unknown, with no
invented monetary value. Optional spend alerts still require the approved
pricing, currency, threshold and delivery contract described in DPES-747;
they are not enabled by this configuration. Repository enablement remains
subject to its existing review gate.

## AI usage from local tools (DPES-1105)

`usage.summary` covers only runs MCAO started. `usage.localTools` (with
`usage.localTools.setEnabled` and `usage.localTools.refresh`) reports the
usage that Claude Code, Codex and GitHub Copilot recorded on this machine in
their own session files. It is a separate source and is never added to the
figures above.

* Opt-in and default off. Turning it on is consent to read the session files;
  installing the reader is a separate matter. Turning it off stops every read,
  kills a running reader and drops the cache.
* Estimates only. Costs are list-price estimates from the reader's price table
  (`basis` is not provider billing). A model without a price is counted as
  unpriced and makes the total a lower bound; it is never zero.
* Reader: codeburn, pinned to one version in `crates/orchestrator-local-usage/codeburn-pin.json`
  with the SHA-256 of the files it runs. Locations, in order: the bundle beside
  the daemon executable (`codeburn/node[.exe]`, `codeburn/pkg/dist/cli.js`);
  then, in debug builds only, `AGENT_ORCH_CODEBURN_BIN` and `PATH`. Release
  builds never use `PATH`.
* Only usage counters leave the reader's output. Model names, project names,
  paths and session ids are dropped by the parser and never stored or logged.
* Offline: the reader is started with `CODEBURN_PRICING_SNAPSHOT_ONLY=1`,
  `CODEBURN_FX_NO_FETCH=1`, a cache directory inside the daemon data directory,
  and an allowlisted environment with no proxy variables.
