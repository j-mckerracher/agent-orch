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
- **`Verifying` tasks appear in neither list.** The snapshot models scheduler
  capacity, and the scheduler counts only `Running` tasks against the global
  and per-project limits (and only `Running` tasks own a worktree for conflict
  checks). A task whose agent has finished and whose candidate is being
  verified holds no worker slot, so it is not `running` here and it never makes
  a queued task wait. A client that compares this snapshot with `tasks.list`
  will therefore see `Verifying` tasks only in `tasks.list`; every other
  active state (`Running`, `Queued`) matches one-to-one. The snapshot does not
  expose a separate `verifying` list: that would be a contract change after
  clients began building on these fields, and `tasks.list` already carries it.
  The behavior is pinned by `verifying_tasks_are_in_neither_list_and_hold_no_capacity`.
- `waiting_reason`: `dependency` | `decision` | `global_cap` | `project_cap` |
  `approval` | `load` | `other`. It is taken from a real scheduler pass with the daemon's real
  limits, so it equals the executor's decision. The global cap is reported
  before the project cap, as the scheduler checks it first.
  - `dependency`: `waiting_on` is the unfinished dependency's task id.
  - `decision`: the task has a question open for the user (DPES-1108 phase 3).
    `waiting_detail` is `open_question`. Only the user's answer or a reconcile
    lets it start.
  - `approval`: the scheduler's start authorization was withheld. Today the
    daemon authorizes any task whose project is registered, and the store
    cannot hold tasks for an unregistered project, so this value is produced
    only if a future policy withholds authorization.
  - `load`: the machine-load gate is closed. `load` contains `meter`,
    `mean_percent`, `limit_percent`, and `reopen_percent`. Once closed, the
    gate stays closed until every gated meter's unrounded rolling average is
    at or below its reopen threshold. The displayed mean is rounded down.
    For example, disk at 89% can still block new launches after triggering
    the 90% limit: its rolling average must reach 80% or lower.
  - `other`: `waiting_detail` is `worktree_owned` (`waiting_on` is the owning
    task), `dispatch_pending` (scheduler-eligible, not proof of launch), `dispatch_blocked`
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

## Desktop queue feedback

Queued task details show queue diagnostics above the task objective, including
the reported wait reason and a next step. The workspace activity queue uses
the same explanations and labels queue order as **First in queue**, not a
launch promise. Disk-load holds tell the operator to free space on the daemon
host and show the reopen threshold, even while usage is below the trigger
limit. Task details re-read the snapshot whenever any task changes state, on
reconnection, on window focus, every 15 seconds, and through **Refresh queue
status** (which also reloads the task). A re-read keeps the current explanation
on screen until the new answer arrives. Failed reads and missing queue support
are shown explicitly rather than interpreted as an empty queue or successful
launch.

Task lists show a short wait reason beside each Queued task's state, so the
explanation does not depend on opening the task or finding the activity
sidebar: the project task list, the Coordinator's related tasks, the
Workstreams task table and **Activity** tab, and the **Queued** tab of Agents &
Sessions. Examples are "Waiting for disk space (89% used)", "All 2 worker slots
in use", "Waiting for t-q" and "Restart gate blocking launches". A list reads
`queue.snapshot` only while it shows a Queued row and never for fixture data,
and re-reads on the same triggers as task details. A failed or disconnected
read removes the summaries rather than leaving a stale reason, and a daemon
without `queue.snapshot` shows "Wait reason unavailable". The summaries are not
live regions; the task page gives the full reason and next step.

The snapshot describes scheduler eligibility, not a launch guarantee.
`dispatch_pending` therefore does not promise that the next pass will launch
an agent. It cannot establish CLI authentication or provider quota; the UI
suggests checking those and the daemon log when the wait persists without
claiming they caused the delay. No wait duration or start-time estimate is
invented from the snapshot's observation time.

This feedback is persistent, contextual, and nonmodal, following NN/g's
[visibility of system status](https://www.nngroup.com/articles/visibility-system-status/)
guidance. The [user-feedback article](https://www.nngroup.com/articles/user-feedback/)
concerns surveys rather than system status; its advice not to interrupt the
user's task is also respected.
