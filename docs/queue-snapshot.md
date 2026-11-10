# `queue.snapshot` — capacity and queue read model

Read-only daemon method for the planning board item M7-05
(`docs/technical-implementation-plan.md`). It reports what the scheduler would
do with the current task records; it changes nothing and offers no pause or
stop-all. No fixed slot count is invented: capacity is exactly the scheduler
limits.

- **Method:** `queue.snapshot`
- **Capability:** `queue` (negotiated at handshake; `Client::queue_snapshot()`
  refuses locally without it and the server rejects the call otherwise).
  Authenticated by the same session token as every other method.
- **Params:** none (`null` or `{}`); anything else is `InvalidParams`.

## Result

```json
{
  "as_of_sequence": 42,
  "epoch": "…journal epoch…",
  "limits": { "max_concurrent_workers": 2, "max_workers_per_project": 2 },
  "dispatch_blocked": false,
  "running": [{ "task_id": "…", "project_id": "…", "agent": "codex" }],
  "queued": [{
    "task_id": "…", "project_id": "…",
    "position": 1,
    "waiting_reason": "global_cap",
    "waiting_on": null,
    "waiting_detail": null
  }]
}
```

- `running`: tasks in state `Running`, ordered by task id.
- `queued`: tasks in state `Queued` only (`Created` tasks are not admitted),
  ordered by `position` (nulls last) then task id.
- `waiting_reason`: `dependency` | `global_cap` | `project_cap` | `approval` |
  `other`. It is taken from a real scheduler pass with the daemon's real
  limits, so it equals the executor's decision. The global cap is reported
  before the project cap, as the scheduler checks it first.
  - `dependency`: `waiting_on` is the unfinished dependency's task id.
  - `approval`: the scheduler's start authorization was withheld. Today the
    daemon authorizes any task whose project is registered, and the store
    cannot hold tasks for an unregistered project, so this value is produced
    only if a future policy withholds authorization.
  - `other`: `waiting_detail` is `worktree_owned` (`waiting_on` is the owning
    task), `dispatch_pending` (the next pass starts it), `dispatch_blocked`
    (the restart gate blocks all launches) or `not_queued`.
  - Capacity reasons carry no `waiting_on`/`waiting_detail`.
- `position`: 1-based "up next" order. Tasks the next pass starts come first,
  then capacity-blocked tasks in the fair round-robin order (across projects,
  continuing from the executor's fairness cursor) a pass without limits would
  use. Tasks blocked for a non-capacity reason have `position: null`.
- `as_of_sequence` / `epoch`: the event sequence and journal epoch the
  snapshot reflects. The task read is bracketed by two reads of the latest
  event sequence and retried up to four times if it moved, so `tasks.list`
  read at the same sequence agrees with the snapshot. If the journal never
  settles, the earlier sequence is reported and the snapshot reflects at
  least every event up to `as_of_sequence`.
- Results contain no worktree or repository paths beyond the `project_id`
  that `tasks.list` already exposes.

Implementation: `crates/orchestrator-daemon/src/api_queue.rs`; scheduler read
accessors and `WaitingReason` in `crates/orchestrator-core/src/scheduler.rs`.
