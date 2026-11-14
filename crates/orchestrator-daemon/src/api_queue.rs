//! Read-only capacity and queue snapshot (`queue.snapshot`).
//!
//! Contract (plan: `docs/technical-implementation-plan.md`, board item M7-05;
//! full description in `docs/queue-snapshot.md`):
//!
//! * Method `queue.snapshot`, capability `queue`, authenticated like every
//!   other method. Params: none (`null` or `{}`); anything else is
//!   `InvalidParams`.
//! * Result:
//!   `{ as_of_sequence, epoch, limits{max_concurrent_workers,
//!   max_workers_per_project}, dispatch_blocked, running[{task_id, project_id,
//!   agent}], queued[{task_id, project_id, position|null, waiting_reason,
//!   waiting_on|null, waiting_detail|null}] }`.
//! * `waiting_reason` is `dependency`, `global_cap`, `project_cap`, `approval`
//!   or `other`; it comes from a real scheduler pass over the same task
//!   records and the daemon's real limits, so it matches what the executor
//!   decides. Only `Queued` tasks appear; `Created` tasks are not admitted.
//! * `position` is the 1-based "up next" order: tasks the scheduler starts on
//!   the next pass come first, then capacity-blocked tasks in the fair
//!   (round-robin over projects, continuing from the executor's fairness
//!   cursor) order a pass without capacity limits would start them. Tasks that
//!   cannot start for a reason other than capacity have `position: null`.
//!   `queued` is sorted by position (nulls last), then task id.
//! * `as_of_sequence` is the latest event sequence the snapshot reflects. The
//!   task list is read between two reads of that sequence and retried when it
//!   moved, so `tasks.list` read at the same sequence agrees with it. If the
//!   journal keeps moving, the earlier sequence is reported: the snapshot then
//!   reflects at least every event up to `as_of_sequence`.
//! * Results omit worktree paths. The scheduler's capacity is the only slot
//!   model; no fixed slot count is invented.

use std::collections::{BTreeMap, BTreeSet};

use orchestrator_core::scheduler::{
    RejectionReason, SchedulerLimits, SchedulerSnapshot, WaitingReason,
};
use orchestrator_store::TaskRecord;

use super::*;

const CONSISTENT_READ_ATTEMPTS: usize = 4;

#[derive(Serialize)]
struct QueueLimits {
    max_concurrent_workers: usize,
    max_workers_per_project: usize,
}

#[derive(Serialize)]
struct RunningEntry {
    task_id: String,
    project_id: String,
    agent: String,
}

#[derive(Serialize)]
struct QueuedEntry {
    task_id: String,
    project_id: String,
    position: Option<usize>,
    waiting_reason: &'static str,
    waiting_on: Option<String>,
    waiting_detail: Option<&'static str>,
}

#[derive(Serialize)]
pub(super) struct QueueSnapshot {
    as_of_sequence: i64,
    epoch: String,
    limits: QueueLimits,
    dispatch_blocked: bool,
    running: Vec<RunningEntry>,
    queued: Vec<QueuedEntry>,
}

pub(super) struct Reading {
    pub(super) sequence: i64,
    pub(super) tasks: Vec<TaskRecord>,
    pub(super) workspaces: Vec<Workspace>,
    projects: Vec<Project>,
}

impl DaemonApi {
    pub(super) fn queue_snapshot(&self, params: Value) -> Result<Value, RpcError> {
        require_empty_params(&params)?;
        let reading = self.consistent_reading()?;
        let (cursor, dispatch_blocked) = self
            .executor
            .as_ref()
            .map_or((None, false), |executor| executor.dispatch_state());
        let registered: BTreeSet<&str> = reading
            .projects
            .iter()
            .map(|project| project.id.as_str())
            .collect();
        // Same rule as the executor: a task may start while its project is registered.
        let authorization = |project_id: &str, _task_id: &str| registered.contains(project_id);
        let snapshot = build_snapshot(
            &reading,
            self.store.store_epoch().map_err(map_store_error)?,
            self.config.limits,
            cursor.as_deref(),
            dispatch_blocked,
            &authorization,
        )?;
        serde_json::to_value(snapshot).map_err(|_| RpcError::Internal)
    }

    pub(super) fn consistent_reading(&self) -> Result<Reading, RpcError> {
        settle(
            CONSISTENT_READ_ATTEMPTS,
            || self.store.latest_event_sequence().map_err(map_store_error),
            |sequence| {
                Ok(Reading {
                    sequence,
                    tasks: self.store.list().map_err(map_store_error)?,
                    workspaces: self.store.list_workspaces().map_err(map_store_error)?,
                    projects: self.store.list_projects().map_err(map_store_error)?,
                })
            },
        )
    }
}

/// Reads with `read(sequence_before)` until the journal sequence is the same
/// before and after, at most `attempts` times; if it never settles, the last
/// reading is returned with the sequence it started at.
pub(super) fn settle<T>(
    attempts: usize,
    mut latest_sequence: impl FnMut() -> Result<i64, RpcError>,
    mut read: impl FnMut(i64) -> Result<T, RpcError>,
) -> Result<T, RpcError> {
    let mut last = None;
    for _ in 0..attempts {
        let before = latest_sequence()?;
        let reading = read(before)?;
        if before == latest_sequence()? {
            return Ok(reading);
        }
        last = Some(reading);
    }
    last.ok_or(RpcError::Internal)
}

pub(super) fn build_snapshot(
    reading: &Reading,
    epoch: String,
    limits: SchedulerLimits,
    cursor: Option<&str>,
    dispatch_blocked: bool,
    authorization: &dyn Fn(&str, &str) -> bool,
) -> Result<QueueSnapshot, RpcError> {
    let snapshots: Vec<_> = reading
        .tasks
        .iter()
        .map(|task| TaskSnapshot {
            contract: &task.contract,
            state: task.state,
            workspace: reading
                .workspaces
                .iter()
                .rev()
                .find(|workspace| workspace.task_id == task.contract.id),
        })
        .collect();
    let plan_with = |limits: SchedulerLimits| {
        Scheduler::new(
            SchedulerSnapshot {
                tasks: &snapshots,
                last_started_project: cursor,
            },
            limits,
        )
        .map(|scheduler| scheduler.schedule(Some(&authorization)))
        .map_err(|_| RpcError::Internal)
    };
    let real = plan_with(limits)?;
    let unbounded =
        SchedulerLimits::new(Some(usize::MAX), Some(usize::MAX)).map_err(|_| RpcError::Internal)?;
    let order = plan_with(unbounded)?;

    // Tasks that start now come first; capacity-blocked ones follow in the
    // fair order they would start once capacity frees up.
    let upcoming = real.started.iter().chain(
        order
            .started
            .iter()
            .filter(|task_id| !real.started.contains(task_id)),
    );
    let positions: BTreeMap<&str, usize> = upcoming
        .enumerate()
        .map(|(index, task_id)| (task_id.as_str(), index + 1))
        .collect();
    let rejections: BTreeMap<&str, &RejectionReason> = real
        .rejected
        .iter()
        .map(|rejection| (rejection.task_id.as_str(), &rejection.reason))
        .collect();
    let pending: BTreeSet<&str> = real.started.iter().map(String::as_str).collect();

    let mut running: Vec<RunningEntry> = reading
        .tasks
        .iter()
        .filter(|task| task.state == TaskState::Running)
        .map(|task| RunningEntry {
            task_id: task.contract.id.clone(),
            project_id: task.contract.project_id.clone(),
            agent: task.contract.agent.clone(),
        })
        .collect();
    running.sort_by(|left, right| left.task_id.cmp(&right.task_id));

    let mut queued: Vec<QueuedEntry> = reading
        .tasks
        .iter()
        .filter(|task| task.state == TaskState::Queued)
        .map(|task| {
            let task_id = task.contract.id.as_str();
            let (reason, waiting_on, detail) = match rejections.get(task_id) {
                Some(rejection) => explain(rejection),
                None if pending.contains(task_id) && dispatch_blocked => {
                    (WaitingReason::Other, None, Some("dispatch_blocked"))
                }
                None => (WaitingReason::Other, None, Some("dispatch_pending")),
            };
            QueuedEntry {
                task_id: task.contract.id.clone(),
                project_id: task.contract.project_id.clone(),
                position: positions.get(task_id).copied(),
                waiting_reason: reason.as_str(),
                waiting_on,
                waiting_detail: detail,
            }
        })
        .collect();
    queued.sort_by(|left, right| {
        let key =
            |entry: &QueuedEntry| (entry.position.unwrap_or(usize::MAX), entry.task_id.clone());
        key(left).cmp(&key(right))
    });

    Ok(QueueSnapshot {
        as_of_sequence: reading.sequence,
        epoch,
        limits: QueueLimits {
            max_concurrent_workers: limits.max_concurrent_workers(),
            max_workers_per_project: limits.max_workers_per_project(),
        },
        dispatch_blocked,
        running,
        queued,
    })
}

fn explain(reason: &RejectionReason) -> (WaitingReason, Option<String>, Option<&'static str>) {
    let category = reason.waiting_reason();
    match reason {
        RejectionReason::DependencyNotSucceeded { dependency_id } => {
            (category, Some(dependency_id.clone()), None)
        }
        RejectionReason::Unauthorized => (category, None, Some("project_not_registered")),
        RejectionReason::WorktreeOwned { owner_task_id, .. } => (
            category,
            Some(owner_task_id.clone()),
            Some("worktree_owned"),
        ),
        RejectionReason::TaskNotQueued { .. } => (category, None, Some("not_queued")),
        RejectionReason::GlobalLimitReached | RejectionReason::ProjectLimitReached => {
            (category, None, None)
        }
    }
}
