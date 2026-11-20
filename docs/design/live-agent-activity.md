# Live agent activity (DPES-1044)

Status: design for the change that shows what an agent is doing while a chat turn or a task session runs.

## Problem

While a turn runs, the chat shows only "Waiting for the orchestrator's reply". Thinking is stored but tool calls are not, nothing says what the agent is doing now, and a hung agent looks like a busy one. Task chats have the same gap.

## What exists (checked against `origin/main` d16f5ba7)

| Area | State |
|---|---|
| `AgentEvent::ThinkingDelta` | Emitted by Claude Code, Codex, Copilot and pi. |
| `AgentEvent::ToolStarted` and `ToolFinished` | Defined. Only `pi_rpc_adapter.rs` emits them, with no summary. The chat loop (`api_conversation_agent.rs` `drive`) drops them in `_ => {}`. The task loop (`executor.rs` `monitor_agent_with`) ignores them. |
| Chat thinking storage | `thinking_chunk` rows in `conversation_entries` (append-only, migration 0063), 1 MiB per turn, one row per delta. |
| Task thinking storage | `session_thinking` table, 8 MiB per session, 16 KiB pieces, read by `sessions.thinking`. |
| Desktop | Chat re-reads the whole transcript every 1 s while the turn runs (`WorkspaceView.tsx` `POLL_MS`). Sessions poll `sessions.thinking` every 2 s (`useSessionThinking.ts`). |
| Push | `events.subscribe` streams the durable daemon journal. One subscription per connection. |
| Liveness | `SessionPulse` in `executor.rs` already keeps a last-event time per running session. |

Correction to the brief: thinking is stored per delta, so a long Claude turn writes thousands of rows. The full-transcript re-read every second then gets expensive. Coalescing is part of this work.

## Decisions

### Poll, do not push

The `events.subscribe` journal is durable and ordered ("Last durable sequence"). Activity is display data that arrives at tens of events per second and is already stored per turn and session. Putting it in the journal would bloat the journal and wake every subscriber. The stream also allows one subscriber per connection. So activity is read by cursor, at 1 s, on the same call the UI already makes. Cursor reads are cheap once thinking is coalesced.

### One stored shape, one more kind

Activity is ordered with thinking and reply text by one sequence, so it lives in the tables that already hold them.

- **Chat.** `conversation_entries` gains kinds `tool_started` and `tool_finished` (author `orchestrator`). A tool row is two append-only entries joined by tool id. A new migration rebuilds the table as 0063 did and adds the kinds to the `CHECK`s.
- **Sessions.** `session_thinking` is renamed `session_activity` and gains `kind TEXT NOT NULL DEFAULT 'thinking'` (`thinking`, `tool_started`, `tool_finished`). The thinking cap and marker logic count only `kind = 'thinking'` rows. `sessions.thinking` keeps its contract and returns thinking rows only.
- **Entry body for tool kinds** is a JSON object.
  - `tool_started`: `{"id": string, "name": string, "summary": string}`
  - `tool_finished`: `{"id": string, "ok": bool, "summary": string}`
  - A cap marker is a `tool_started` row with body `{"truncated": true}`.
- **Caps.** At most 400 `tool_started` rows per turn or session. After that one marker row is stored and later starts are dropped. A `tool_finished` row is stored only when its start was stored. Writes are best effort and never fail the turn. Chat turns track the count in `conversation_turns.activity_count`.

### Redaction and size

Tool input and output can hold secrets. Thinking stays unredacted (existing behavior, documented). Tool text goes through one daemon helper before it is stored:

1. Drop control characters and fold newlines to spaces (the existing `sanitize`).
2. `orchestrator_diagnostics::redact` (URL userinfo, sensitive key values, bearer tokens, known token shapes, registered secrets).
3. `orchestrator_diagnostics::truncate`, after redaction so a cut cannot hide a half credential. Summaries of a started tool keep 160 characters. Result previews keep 400 characters and end with the library's `...[truncated N chars]` marker.

Adapters hand raw summaries to the daemon. Full file contents and full command output are never stored. Adapters pick the field that identifies the call (path, command, pattern, URL) and cut it at 2 KiB before it leaves the adapter, so a huge argument is not copied around.

### Live snapshot

`SessionPulse` becomes `ActivityBoard`, keyed by session id or chat turn id. Per key it holds the phase, the running tool (id, name, summary), when the phase began, and the last event time. It is updated on every event in the chat loop and the executor loop, and the key is removed when the turn or session ends.

Phases: `starting`, `thinking`, `tool`, `replying`. No adapter emits an approval request today, so there is no approval phase; the UI shows a phase it does not know as the daemon sent it.

The daemon returns durations, not timestamps: `phase_ms` and `idle_ms`, both measured at read time. The UI adds the time since the reply arrived, so a clock difference between daemon and UI cannot show a wrong elapsed time.

### Thinking coalescing

Both loops buffer thinking deltas and write one row when the buffer reaches 2 KiB, 250 ms after its first delta, when any other event arrives, or when the loop ends. This keeps the order exact (thinking is flushed before a tool or reply row) and cuts rows by one to two orders of magnitude.

### Closing open rows

A tool with no `tool_finished` row in a turn or session that is no longer running renders as stopped. This covers cancel, timeout, failure and daemon restart without extra writes. The UI decides, from the turn state it already has.

## RPC and wire shape

| Call | Change |
|---|---|
| `conversations.get` | Entries may have kinds `tool_started` and `tool_finished`. The result gains `activity`, set only while the latest turn runs: `{"phase", "tool": {"id","name","summary"} or null, "phase_ms", "idle_ms"}`. |
| `sessions.activity` (new, capability `sessions`, read-only) | Params `{session_id, after?, limit?}`. Result `{"entries": [{"sequence","kind","body"}], "next_cursor", "truncated", "live": snapshot or null}`. `live` is null once the session is not running. |
| `sessions.thinking` | Unchanged. |

Version gate. `sessions.activity` is a new method, so an older daemon answers method-not-found and the UI keeps today's behavior through `isMissingFeature`: it falls back to `sessions.thinking` and shows no tool rows and no status line detail. For chat, an older daemon sends no tool kinds and no `activity`, so the UI shows today's thinking block with the new generic status line (elapsed only).

## Desktop

Shared components in `apps/desktop/src/ui/activity/`, used by the orchestrator chat and the task conversation:

- `ActivityLog`: ordered items, thinking text (existing `ThinkingBlock`, open while running) and tool rows. A tool row shows name, one-line summary, a spinner while running, a check or cross when finished, a stop mark when stopped. It expands by keyboard (`Enter` and `Space`) to the redacted result preview. Rendering is capped to the newest 200 rows with an "N earlier tool calls" line; thinking text is joined once, not per chunk.
- `ActivityStatus`: a polite live region, "Thinking · 4 s", "Running Read · 12 s", and "No activity for N s" once `idle_ms` passes 20 s. A one-second ticker re-renders only this element.
- A scroll hook that follows the newest line until the user scrolls up, then shows "Jump to latest".

Data layer.

- `mapEntries` turns `thinking_chunk`, `tool_started` and `tool_finished` into `ActivityItem`s in sequence order and merges adjacent thinking.
- While the latest turn runs, the transcript is read from the last known sequence and appended. It equals a full read and falls back to a full read on clear or replay gap (the existing `restartOnHistoryChange`).
- `useSessionActivity(sessionId, poll)` replaces `useSessionThinking` for tasks. It polls `sessions.activity` at 1 s with a cursor and falls back to `sessions.thinking` when the method is missing.

UI states.

| State | Orchestrator chat | Task chat |
|---|---|---|
| Starting | Status "Starting · N s", composer blocked with the status text | Same |
| Thinking | Thinking block open, status "Thinking · N s" | Same |
| Tool running | Running tool row with spinner, status `Running` plus the tool name and N s | Same |
| Stale | Status "No activity for N s" | Same |
| Finished | Block collapsed, rows keep check or cross | Static history from the cursor reads |
| Cancelled or failed | Open tools show stopped, thinking collapsed | Same |
| Old daemon | Thinking only plus elapsed | `sessions.thinking` text only |

Cancel reply stays in place and is not gated on activity. The wake-turn, proposal-card and update-card paths in `WorkspaceView.tsx` are not touched.

## Test plan

- Parsers: one fixture per provider under `crates/orchestrator-agents/tests/fixtures/` with real output shapes, plus pi.
- Store: ordering with thinking and replies, caps, marker, restart persistence (reopen the database), redaction at the daemon boundary.
- Daemon: `examples/fake_agent.rs` emits thinking and tool events; tests cover a normal turn, cancel mid-tool, timeout mid-tool, the live snapshot, and a planted secret that never reaches a stored row.
- Desktop: `mapEntries` interleave, the data-layer cursor append, `useSessionActivity` (cursor, missing feature, stale), component tests for the chat and the task conversation (live, finished, cancel).
