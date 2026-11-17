//! Pure dependency-aware worker scheduling. Updated: 2026-11-17 04:19 UTC.
//!
//! Only `Succeeded` tasks satisfy dependencies. Only `Running` tasks consume
//! worker capacity and reserve worktrees. Worktree paths compare globally;
//! nonempty branch names compare within their project. Values compare as exact
//! strings, so callers must canonicalize them. Pass each plan's last-started
//! project into the next snapshot to preserve fairness across scheduling calls.
//!
//! The machine load gate (decision N12) enters as one more input, a
//! [`StartGate`]. The caller decides whether it is open; the scheduler only
//! obeys, so nothing here reads load.

use crate::{contract::TaskContract, load_gate::LoadHold, records::Workspace, task::TaskState};
use std::{collections::BTreeMap, num::NonZeroUsize};

/// One task's scheduling inputs. The referenced contract and workspace remain
/// owned by the caller for the scheduler's lifetime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskSnapshot<'a> {
    /// Task identity, project, and dependencies from an admitted contract.
    pub contract: &'a TaskContract,
    /// Current state from the central task state machine.
    pub state: TaskState,
    /// Assigned worktree, if this task claims one. Its owner ID must match the
    /// contract ID; an empty branch path means no branch claim.
    pub workspace: Option<&'a Workspace>,
}

/// Immutable task view and fairness cursor consumed by [`Scheduler`].
#[derive(Debug, Clone, Copy)]
pub struct SchedulerSnapshot<'a> {
    /// Include states for dependencies that may satisfy a task. Missing
    /// dependency records remain unsatisfied.
    pub tasks: &'a [TaskSnapshot<'a>],
    /// Project that received the last start in the prior pass. `None` starts
    /// from the first project ID.
    pub last_started_project: Option<&'a str>,
}

/// Finite worker limits. `None` in [`SchedulerLimits::new`] means unbounded
/// and is rejected. Defaults allow two global workers and two per project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchedulerLimits {
    max_concurrent_workers: NonZeroUsize,
    max_workers_per_project: NonZeroUsize,
}

impl SchedulerLimits {
    /// Build limits. Zero and unbounded values return a typed error.
    pub fn new(
        max_concurrent_workers: Option<usize>,
        max_workers_per_project: Option<usize>,
    ) -> Result<Self, SchedulerLimitError> {
        let max_concurrent_workers =
            max_concurrent_workers.ok_or(SchedulerLimitError::UnboundedGlobal)?;
        let max_concurrent_workers =
            NonZeroUsize::new(max_concurrent_workers).ok_or(SchedulerLimitError::ZeroGlobal)?;
        let max_workers_per_project =
            max_workers_per_project.ok_or(SchedulerLimitError::UnboundedPerProject)?;
        let max_workers_per_project = NonZeroUsize::new(max_workers_per_project)
            .ok_or(SchedulerLimitError::ZeroPerProject)?;

        Ok(Self {
            max_concurrent_workers,
            max_workers_per_project,
        })
    }

    /// Maximum number of `Running` tasks across all projects.
    pub fn max_concurrent_workers(&self) -> usize {
        self.max_concurrent_workers.get()
    }

    /// Maximum number of `Running` tasks in one project.
    pub fn max_workers_per_project(&self) -> usize {
        self.max_workers_per_project.get()
    }
}

impl Default for SchedulerLimits {
    fn default() -> Self {
        Self {
            max_concurrent_workers: NonZeroUsize::new(2).expect("two is nonzero"),
            max_workers_per_project: NonZeroUsize::new(2).expect("two is nonzero"),
        }
    }
}

/// Invalid concurrency limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedulerLimitError {
    /// Global worker limit was zero.
    ZeroGlobal,
    /// Global worker limit was unbounded.
    UnboundedGlobal,
    /// Per-project worker limit was zero.
    ZeroPerProject,
    /// Per-project worker limit was unbounded.
    UnboundedPerProject,
}

impl std::fmt::Display for SchedulerLimitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroGlobal => f.write_str("global worker limit must be nonzero"),
            Self::UnboundedGlobal => f.write_str("global worker limit must be bounded"),
            Self::ZeroPerProject => f.write_str("per-project worker limit must be nonzero"),
            Self::UnboundedPerProject => f.write_str("per-project worker limit must be bounded"),
        }
    }
}

impl std::error::Error for SchedulerLimitError {}

/// Kind of worktree identity reported when another task owns a claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorktreeResource {
    /// Exact worktree path supplied by the caller.
    Path,
    /// Exact nonempty branch name within its project.
    Branch,
}

impl std::fmt::Display for WorktreeResource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Path => f.write_str("worktree path"),
            Self::Branch => f.write_str("branch path"),
        }
    }
}

/// Invalid scheduler input. Dependency cycles include a closed path whose
/// first and last task IDs match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchedulerError {
    /// A snapshot contains the same task ID more than once.
    DuplicateTaskId(String),
    /// A dependency cycle, including the repeated starting task.
    DependencyCycle { cycle: Vec<String> },
    /// A workspace names a different owning task than its snapshot entry.
    WorkspaceOwnerMismatch {
        task_id: String,
        workspace_task_id: String,
    },
    /// A workspace path is empty and cannot identify an exclusive resource.
    EmptyWorktreePath { task_id: String },
    /// Two running tasks already claim the same worktree path or branch path.
    DuplicateRunningClaim {
        first_task_id: String,
        second_task_id: String,
        resource: WorktreeResource,
        value: String,
    },
}

impl std::fmt::Display for SchedulerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateTaskId(task_id) => write!(f, "duplicate task ID {task_id:?}"),
            Self::DependencyCycle { cycle } => {
                f.write_str("dependency cycle: ")?;
                for (index, task_id) in cycle.iter().enumerate() {
                    if index > 0 {
                        f.write_str(" -> ")?;
                    }
                    f.write_str(task_id)?;
                }
                Ok(())
            }
            Self::WorkspaceOwnerMismatch {
                task_id,
                workspace_task_id,
            } => write!(
                f,
                "task {task_id:?} has workspace owned by {workspace_task_id:?}"
            ),
            Self::EmptyWorktreePath { task_id } => {
                write!(f, "task {task_id:?} has an empty worktree path")
            }
            Self::DuplicateRunningClaim {
                first_task_id,
                second_task_id,
                resource,
                value,
            } => write!(
                f,
                "{resource} {value:?} is claimed by running tasks {first_task_id:?} and {second_task_id:?}"
            ),
        }
    }
}

impl std::error::Error for SchedulerError {}

/// Caller-supplied authorization policy. Return `true` only when the task may
/// start in its project.
pub trait StartAuthorization {
    /// Authorize one task start for `project_id`.
    fn allows(&self, project_id: &str, task_id: &str) -> bool;
}

impl<F> StartAuthorization for F
where
    F: Fn(&str, &str) -> bool,
{
    fn allows(&self, project_id: &str, task_id: &str) -> bool {
        self(project_id, task_id)
    }
}

/// One task that could not start during a scheduling pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskRejection {
    /// Rejected task ID.
    pub task_id: String,
    /// Reason determined by state, dependencies, authorization, ownership, or
    /// capacity checks.
    pub reason: RejectionReason,
}

/// Whether the scheduler may start tasks. The caller decides, from the machine
/// load gate; the scheduler only obeys. A closed gate never touches a task
/// that is already running.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum StartGate {
    /// Tasks may start, within the worker limits.
    #[default]
    Open,
    /// No task may start. The hold names the meter that closed the gate.
    Closed(LoadHold),
}

/// Why the scheduler did not start a queued task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RejectionReason {
    /// Task state was not `Queued`.
    TaskNotQueued { state: TaskState },
    /// Dependency is absent from the snapshot or has not succeeded.
    DependencyNotSucceeded { dependency_id: String },
    /// No authorization policy was supplied or the policy denied the task.
    Unauthorized,
    /// The global worker limit is full.
    GlobalLimitReached,
    /// The per-project worker limit is full.
    ProjectLimitReached,
    /// Another running or selected task owns the same exact worktree identity.
    WorktreeOwned {
        owner_task_id: String,
        resource: WorktreeResource,
        value: String,
    },
    /// The start gate is closed because the machine is busy. Only a task that
    /// could otherwise start gets this reason.
    LoadGateClosed(LoadHold),
}

/// Operator-facing category of a [`RejectionReason`], for read models that
/// explain why a queued task is waiting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitingReason {
    /// A dependency has not succeeded.
    Dependency,
    /// The global worker limit is full.
    GlobalCap,
    /// The per-project worker limit is full.
    ProjectCap,
    /// Start authorization has not been granted.
    Approval,
    /// The machine load gate is closed.
    Load,
    /// Anything else: wrong task state or a worktree claimed by another task.
    Other,
}

impl WaitingReason {
    /// Stable wire name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dependency => "dependency",
            Self::GlobalCap => "global_cap",
            Self::ProjectCap => "project_cap",
            Self::Approval => "approval",
            Self::Load => "load",
            Self::Other => "other",
        }
    }
}

impl RejectionReason {
    /// Category of this rejection. It is a pure projection of the scheduler's
    /// own decision, so a read model built from it cannot disagree with a
    /// scheduling pass over the same snapshot.
    pub fn waiting_reason(&self) -> WaitingReason {
        match self {
            Self::DependencyNotSucceeded { .. } => WaitingReason::Dependency,
            Self::GlobalLimitReached => WaitingReason::GlobalCap,
            Self::ProjectLimitReached => WaitingReason::ProjectCap,
            Self::Unauthorized => WaitingReason::Approval,
            Self::LoadGateClosed(_) => WaitingReason::Load,
            Self::TaskNotQueued { .. } | Self::WorktreeOwned { .. } => WaitingReason::Other,
        }
    }
}

/// Deterministic result of one scheduling pass. Started IDs follow dispatch
/// order; rejections are sorted by task ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchedulePlan {
    /// Task IDs that may start now, in round-robin dispatch order.
    pub started: Vec<String>,
    /// Every input task that did not start, with its rejection reason.
    pub rejected: Vec<TaskRejection>,
    /// Cursor to pass into the next snapshot to continue round-robin fairness.
    pub last_started_project: Option<String>,
}

struct ProjectQueue<'a> {
    project_id: &'a str,
    tasks: Vec<usize>,
    next_task: usize,
}

struct WorktreeOwner<'a> {
    task_id: &'a str,
    project_id: &'a str,
    path: &'a str,
    branch: &'a str,
}

fn worktree_conflict<'a>(
    workspace: Option<&'a Workspace>,
    project_id: &str,
    owners: &[WorktreeOwner<'a>],
) -> Option<(&'a str, WorktreeResource, &'a str)> {
    let workspace = workspace?;
    owners.iter().find_map(|owner| {
        if workspace.path == owner.path {
            Some((owner.task_id, WorktreeResource::Path, owner.path))
        } else if project_id == owner.project_id
            && !workspace.branch.is_empty()
            && workspace.branch == owner.branch
        {
            Some((owner.task_id, WorktreeResource::Branch, owner.branch))
        } else {
            None
        }
    })
}

fn worktree_owner<'a>(task: &TaskSnapshot<'a>) -> Option<WorktreeOwner<'a>> {
    let workspace = task.workspace?;
    Some(WorktreeOwner {
        task_id: &task.contract.id,
        project_id: &task.contract.project_id,
        path: &workspace.path,
        branch: &workspace.branch,
    })
}

/// Dependency-aware bounded scheduler. Construction validates task IDs,
/// dependency cycles, workspace ownership, and existing running claims.
#[derive(Debug)]
pub struct Scheduler<'a> {
    snapshot: SchedulerSnapshot<'a>,
    limits: SchedulerLimits,
    order: Vec<usize>,
    by_id: BTreeMap<&'a str, usize>,
    gate: StartGate,
}

impl<'a> Scheduler<'a> {
    /// Validate a task snapshot and its limits before scheduling.
    ///
    /// Contracts must already pass [`crate::contract::validate`]. Missing
    /// dependency records are not errors, but they cannot satisfy a task.
    /// Duplicate IDs, cycles, mismatched workspace owners, empty worktree
    /// paths, and duplicate running claims return [`SchedulerError`].
    pub fn new(
        snapshot: SchedulerSnapshot<'a>,
        limits: SchedulerLimits,
    ) -> Result<Self, SchedulerError> {
        let (order, by_id) = validate_snapshot(snapshot)?;
        Ok(Self {
            snapshot,
            limits,
            order,
            by_id,
            gate: StartGate::Open,
        })
    }

    /// Sets whether tasks may start. [`Scheduler::new`] starts open. A closed
    /// gate starts nothing and gives every task that could otherwise start
    /// [`RejectionReason::LoadGateClosed`]. Running tasks, the fairness cursor
    /// and every other rejection reason stay as they are.
    pub fn with_start_gate(mut self, gate: StartGate) -> Self {
        self.gate = gate;
        self
    }

    /// Return starts and a reason for every task that does not start.
    ///
    /// Projects take turns in project-ID order, starting after the snapshot
    /// cursor. Tasks within each project use task-ID order. The policy runs in
    /// task-ID order for queued tasks whose dependencies succeeded. `None`
    /// denies every start, and an authorization grant cannot bypass capacity
    /// or ownership checks. Global capacity takes precedence over project
    /// capacity when both limits are full. A closed start gate holds back every
    /// task that passed those checks. A deterministic, side-effect-free
    /// policy keeps the result deterministic.
    pub fn schedule(&self, authorization: Option<&dyn StartAuthorization>) -> SchedulePlan {
        let tasks = self.snapshot.tasks;
        let order = &self.order;
        let by_id = &self.by_id;

        let mut rejections = vec![None; tasks.len()];
        let mut eligible = Vec::new();
        for &index in order {
            let task = &tasks[index];
            if task.state != TaskState::Queued {
                rejections[index] = Some(RejectionReason::TaskNotQueued { state: task.state });
                continue;
            }
            if let Some(dependency_id) = task
                .contract
                .dependencies
                .iter()
                .filter(|dependency_id| match by_id.get(dependency_id.as_str()) {
                    Some(&dependency_index) => {
                        tasks[dependency_index].state != TaskState::Succeeded
                    }
                    None => true,
                })
                .min()
            {
                rejections[index] = Some(RejectionReason::DependencyNotSucceeded {
                    dependency_id: dependency_id.clone(),
                });
                continue;
            }
            if !matches!(
                authorization,
                Some(policy) if policy.allows(&task.contract.project_id, &task.contract.id)
            ) {
                rejections[index] = Some(RejectionReason::Unauthorized);
                continue;
            }
            eligible.push(index);
        }

        // A closed gate holds back every task that could have started. The
        // rest of the pass then has nothing to plan, so the cursor stays put.
        if let StartGate::Closed(hold) = self.gate {
            for &index in &eligible {
                rejections[index] = Some(RejectionReason::LoadGateClosed(hold));
            }
            eligible.clear();
        }

        let mut running = 0;
        let mut running_per_project = BTreeMap::<&str, usize>::new();
        let mut owners = Vec::new();
        for &index in order {
            let task = &tasks[index];
            if task.state != TaskState::Running {
                continue;
            }
            running += 1;
            *running_per_project
                .entry(task.contract.project_id.as_str())
                .or_default() += 1;
            if let Some(owner) = worktree_owner(task) {
                owners.push(owner);
            }
        }

        let mut project_tasks = BTreeMap::<&str, Vec<usize>>::new();
        for &index in &eligible {
            project_tasks
                .entry(tasks[index].contract.project_id.as_str())
                .or_default()
                .push(index);
        }
        let mut queues: Vec<_> = project_tasks
            .into_iter()
            .map(|(project_id, tasks)| ProjectQueue {
                project_id,
                tasks,
                next_task: 0,
            })
            .collect();
        let first_queue = self
            .snapshot
            .last_started_project
            .and_then(|last| queues.iter().position(|queue| queue.project_id > last))
            .unwrap_or(0);
        queues.rotate_left(first_queue);

        let global_limit = self.limits.max_concurrent_workers.get();
        let project_limit = self.limits.max_workers_per_project.get();
        let queue_count = queues.len();
        let mut next_queue = 0;
        let mut started = Vec::new();
        let mut started_by_index = vec![false; tasks.len()];
        let mut last_started_project = self.snapshot.last_started_project.map(str::to_owned);

        while running < global_limit && queue_count > 0 {
            let mut started_this_turn = false;
            for offset in 0..queue_count {
                let queue_index = (next_queue + offset) % queue_count;
                let queue = &mut queues[queue_index];
                let project_id = queue.project_id;
                while queue.next_task < queue.tasks.len() {
                    if running_per_project.get(project_id).copied().unwrap_or(0) >= project_limit {
                        break;
                    }

                    let task_index = queue.tasks[queue.next_task];
                    if let Some((owner_task_id, resource, value)) =
                        worktree_conflict(tasks[task_index].workspace, project_id, &owners)
                    {
                        rejections[task_index] = Some(RejectionReason::WorktreeOwned {
                            owner_task_id: owner_task_id.to_owned(),
                            resource,
                            value: value.to_owned(),
                        });
                        queue.next_task += 1;
                        continue;
                    }

                    queue.next_task += 1;
                    started_by_index[task_index] = true;
                    started.push(tasks[task_index].contract.id.clone());
                    running += 1;
                    *running_per_project.entry(project_id).or_default() += 1;
                    if let Some(owner) = worktree_owner(&tasks[task_index]) {
                        owners.push(owner);
                    }
                    last_started_project = Some(project_id.to_owned());
                    next_queue = (queue_index + 1) % queue_count;
                    started_this_turn = true;
                    break;
                }
                if started_this_turn {
                    break;
                }
            }
            if !started_this_turn {
                break;
            }
        }

        for &index in &eligible {
            if started_by_index[index] || rejections[index].is_some() {
                continue;
            }
            let project_id = tasks[index].contract.project_id.as_str();
            let reason = if running >= global_limit {
                RejectionReason::GlobalLimitReached
            } else {
                assert!(
                    running_per_project.get(project_id).copied().unwrap_or(0) >= project_limit,
                    "unstarted eligible task must have a full worker limit"
                );
                RejectionReason::ProjectLimitReached
            };
            rejections[index] = Some(reason);
        }

        let rejected = order
            .iter()
            .copied()
            .filter(|&index| !started_by_index[index])
            .map(|index| TaskRejection {
                task_id: tasks[index].contract.id.clone(),
                reason: rejections[index]
                    .take()
                    .expect("every unstarted task has a rejection reason"),
            })
            .collect();

        SchedulePlan {
            started,
            rejected,
            last_started_project,
        }
    }
}

fn validate_snapshot<'a>(
    snapshot: SchedulerSnapshot<'a>,
) -> Result<(Vec<usize>, BTreeMap<&'a str, usize>), SchedulerError> {
    let tasks = snapshot.tasks;
    let mut order: Vec<_> = (0..tasks.len()).collect();
    order.sort_unstable_by(|left, right| tasks[*left].contract.id.cmp(&tasks[*right].contract.id));

    let mut by_id = BTreeMap::<&'a str, usize>::new();
    for &index in &order {
        let task = &tasks[index];
        let task_id = task.contract.id.as_str();
        if by_id.insert(task_id, index).is_some() {
            return Err(SchedulerError::DuplicateTaskId(task_id.to_owned()));
        }
        if let Some(workspace) = task.workspace {
            if workspace.task_id != task.contract.id {
                return Err(SchedulerError::WorkspaceOwnerMismatch {
                    task_id: task.contract.id.clone(),
                    workspace_task_id: workspace.task_id.clone(),
                });
            }
            if workspace.path.is_empty() {
                return Err(SchedulerError::EmptyWorktreePath {
                    task_id: task.contract.id.clone(),
                });
            }
        }
    }

    let mut dependencies = vec![Vec::new(); tasks.len()];
    for (index, task) in tasks.iter().enumerate() {
        for dependency in &task.contract.dependencies {
            if let Some(&dependency_index) = by_id.get(dependency.as_str()) {
                dependencies[index].push(dependency_index);
            }
        }
        dependencies[index].sort_unstable_by(|left, right| {
            tasks[*left].contract.id.cmp(&tasks[*right].contract.id)
        });
        dependencies[index].dedup();
    }
    if let Some(cycle) = find_cycle(&dependencies, &order) {
        return Err(SchedulerError::DependencyCycle {
            cycle: cycle
                .into_iter()
                .map(|index| tasks[index].contract.id.clone())
                .collect(),
        });
    }

    let mut paths = BTreeMap::<&str, usize>::new();
    let mut branches = BTreeMap::<(&str, &str), usize>::new();
    for &index in &order {
        let task = &tasks[index];
        if task.state != TaskState::Running {
            continue;
        }
        let Some(workspace) = task.workspace else {
            continue;
        };
        if let Some(&owner) = paths.get(workspace.path.as_str()) {
            return Err(duplicate_claim(
                tasks,
                owner,
                index,
                WorktreeResource::Path,
                &workspace.path,
            ));
        }
        paths.insert(workspace.path.as_str(), index);
        if !workspace.branch.is_empty() {
            let branch_key = (task.contract.project_id.as_str(), workspace.branch.as_str());
            if let Some(&owner) = branches.get(&branch_key) {
                return Err(duplicate_claim(
                    tasks,
                    owner,
                    index,
                    WorktreeResource::Branch,
                    &workspace.branch,
                ));
            }
            branches.insert(branch_key, index);
        }
    }

    Ok((order, by_id))
}

fn duplicate_claim(
    tasks: &[TaskSnapshot<'_>],
    first: usize,
    second: usize,
    resource: WorktreeResource,
    value: &str,
) -> SchedulerError {
    SchedulerError::DuplicateRunningClaim {
        first_task_id: tasks[first].contract.id.clone(),
        second_task_id: tasks[second].contract.id.clone(),
        resource,
        value: value.to_owned(),
    }
}

fn find_cycle(dependencies: &[Vec<usize>], roots: &[usize]) -> Option<Vec<usize>> {
    let mut state = vec![0_u8; dependencies.len()];
    let mut stack = Vec::<(usize, usize)>::new();

    for &root in roots {
        if state[root] != 0 {
            continue;
        }
        state[root] = 1;
        stack.push((root, 0));

        while !stack.is_empty() {
            let exhausted = {
                let (task, next_dependency) = stack.last().expect("DFS stack is nonempty");
                *next_dependency == dependencies[*task].len()
            };
            if exhausted {
                let (task, _) = stack.pop().expect("DFS stack is nonempty");
                state[task] = 2;
                continue;
            }

            let dependency = {
                let (task, next_dependency) = stack.last_mut().expect("DFS stack is nonempty");
                let dependency = dependencies[*task][*next_dependency];
                *next_dependency += 1;
                dependency
            };
            match state[dependency] {
                0 => {
                    state[dependency] = 1;
                    stack.push((dependency, 0));
                }
                1 => {
                    let cycle_start = stack
                        .iter()
                        .position(|(task, _)| *task == dependency)
                        .expect("gray task is on the DFS stack");
                    return Some(
                        stack[cycle_start..]
                            .iter()
                            .map(|(task, _)| *task)
                            .chain(std::iter::once(dependency))
                            .collect(),
                    );
                }
                _ => {}
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        contract::{AutonomyPolicy, DeliveryPolicy, TaskKind, VerificationPolicy},
        load_gate::LoadMeter,
    };

    const HOLD: LoadHold = LoadHold {
        meter: LoadMeter::Cpu,
        mean_percent: 91,
        limit_percent: 80,
    };
    const CLOSED: StartGate = StartGate::Closed(HOLD);

    fn contract(id: &str, project_id: &str, dependencies: &[&str]) -> TaskContract {
        TaskContract {
            id: id.to_owned(),
            project_id: project_id.to_owned(),
            parent_id: None,
            objective: "test the start gate".to_owned(),
            kind: TaskKind::Implement,
            agent: "test".to_owned(),
            dependencies: dependencies
                .iter()
                .map(|dependency| (*dependency).to_owned())
                .collect(),
            verification: VerificationPolicy::None,
            delivery: DeliveryPolicy::LocalReview,
            autonomy: AutonomyPolicy::WorkspaceWrites,
            max_retries: 0,
            max_repairs: 0,
            model_selection: None,
            permission: Default::default(),
        }
    }

    fn allow_all(_: &str, _: &str) -> bool {
        true
    }

    /// One pass over `tasks`. `None` leaves the gate to `Scheduler::new` alone.
    fn pass(
        tasks: &[(TaskContract, TaskState)],
        limits: SchedulerLimits,
        gate: Option<StartGate>,
        last_started_project: Option<&str>,
        authorization: &dyn StartAuthorization,
    ) -> SchedulePlan {
        let snapshots: Vec<_> = tasks
            .iter()
            .map(|(contract, state)| TaskSnapshot {
                contract,
                state: *state,
                workspace: None,
            })
            .collect();
        let scheduler = Scheduler::new(
            SchedulerSnapshot {
                tasks: &snapshots,
                last_started_project,
            },
            limits,
        )
        .expect("snapshot is valid");
        let scheduler = match gate {
            Some(gate) => scheduler.with_start_gate(gate),
            None => scheduler,
        };
        scheduler.schedule(Some(authorization))
    }

    fn reason<'a>(plan: &'a SchedulePlan, task_id: &str) -> &'a RejectionReason {
        &plan
            .rejected
            .iter()
            .find(|rejection| rejection.task_id == task_id)
            .expect("task has a rejection")
            .reason
    }

    #[test]
    fn a_closed_start_gate_starts_nothing_and_rejects_each_eligible_task_with_the_hold() {
        let tasks = [
            (contract("a", "project-a", &[]), TaskState::Queued),
            (contract("b", "project-b", &[]), TaskState::Queued),
            (contract("c", "project-a", &[]), TaskState::Queued),
        ];

        let open = pass(&tasks, SchedulerLimits::default(), None, None, &allow_all);
        assert_eq!(open.started, ["a", "b"]);

        let closed = pass(
            &tasks,
            SchedulerLimits::default(),
            Some(CLOSED),
            None,
            &allow_all,
        );
        assert!(closed.started.is_empty());
        assert_eq!(closed.rejected.len(), 3);
        for task_id in ["a", "b", "c"] {
            assert_eq!(
                reason(&closed, task_id),
                &RejectionReason::LoadGateClosed(HOLD)
            );
        }
    }

    #[test]
    fn a_closed_start_gate_leaves_running_tasks_and_the_cursor_untouched() {
        let tasks = [
            (contract("run", "project-a", &[]), TaskState::Running),
            (contract("a", "project-a", &[]), TaskState::Queued),
            (contract("b", "project-b", &[]), TaskState::Queued),
        ];
        let limits = SchedulerLimits::default();

        // Open, the cursor moves on to project-b and one task starts.
        let open = pass(&tasks, limits, None, Some("project-a"), &allow_all);
        assert_eq!(open.started, ["b"]);
        assert_eq!(open.last_started_project.as_deref(), Some("project-b"));

        // Closed, nothing starts and the cursor is the one the caller passed in.
        for cursor in [None, Some("project-a")] {
            let closed = pass(&tasks, limits, Some(CLOSED), cursor, &allow_all);
            assert!(closed.started.is_empty());
            assert_eq!(closed.last_started_project.as_deref(), cursor);
            // The running task is reported as it is without a gate: not queued, not stopped.
            assert_eq!(
                reason(&closed, "run"),
                &RejectionReason::TaskNotQueued {
                    state: TaskState::Running
                }
            );
            assert_eq!(reason(&open, "run"), reason(&closed, "run"));
        }
    }

    #[test]
    fn an_open_start_gate_gives_the_same_plan_as_before() {
        let tasks = [
            (contract("run", "project-a", &[]), TaskState::Running),
            (contract("a-1", "project-a", &[]), TaskState::Queued),
            (contract("a-2", "project-a", &[]), TaskState::Queued),
            (contract("b-1", "project-b", &["done"]), TaskState::Queued),
            (contract("b-2", "project-b", &["a-1"]), TaskState::Queued),
            (contract("c-1", "project-c", &[]), TaskState::Queued),
            (contract("c-2", "project-c", &[]), TaskState::Queued),
            (contract("done", "project-d", &[]), TaskState::Succeeded),
            (contract("new", "project-d", &[]), TaskState::Created),
            (contract("u-1", "project-u", &[]), TaskState::Queued),
        ];
        let deny_project_u = |project_id: &str, _: &str| project_id != "project-u";
        let limits = SchedulerLimits::new(Some(3), Some(2)).unwrap();

        for cursor in [None, Some("project-b")] {
            let before = pass(&tasks, limits, None, cursor, &deny_project_u);
            let open = pass(
                &tasks,
                limits,
                Some(StartGate::Open),
                cursor,
                &deny_project_u,
            );
            let default = pass(
                &tasks,
                limits,
                Some(StartGate::default()),
                cursor,
                &deny_project_u,
            );
            assert_eq!(open, before);
            assert_eq!(default, before);
            // The plan is not trivial: it starts tasks and rejects for several reasons.
            assert!(!before.started.is_empty());
            assert!(before.rejected.len() > 3);
            assert!(!before
                .rejected
                .iter()
                .any(|rejection| matches!(rejection.reason, RejectionReason::LoadGateClosed(_))));
        }
    }

    #[test]
    fn the_default_start_gate_is_open() {
        assert_eq!(StartGate::default(), StartGate::Open);
    }

    #[test]
    fn a_task_that_is_not_queued_or_not_authorized_keeps_its_own_reason_under_a_closed_gate() {
        let tasks = [
            (contract("created", "project-a", &[]), TaskState::Created),
            (
                contract("cancelled", "project-a", &[]),
                TaskState::Cancelled,
            ),
            (contract("done", "project-a", &[]), TaskState::Succeeded),
            (
                contract("waiting", "project-a", &["cancelled"]),
                TaskState::Queued,
            ),
            (contract("denied", "project-u", &[]), TaskState::Queued),
            (contract("ready", "project-a", &["done"]), TaskState::Queued),
        ];
        let deny_project_u = |project_id: &str, _: &str| project_id != "project-u";

        let plan = pass(
            &tasks,
            SchedulerLimits::default(),
            Some(CLOSED),
            None,
            &deny_project_u,
        );

        assert!(plan.started.is_empty());
        assert_eq!(
            reason(&plan, "created"),
            &RejectionReason::TaskNotQueued {
                state: TaskState::Created
            }
        );
        assert_eq!(
            reason(&plan, "cancelled"),
            &RejectionReason::TaskNotQueued {
                state: TaskState::Cancelled
            }
        );
        assert_eq!(
            reason(&plan, "done"),
            &RejectionReason::TaskNotQueued {
                state: TaskState::Succeeded
            }
        );
        assert_eq!(
            reason(&plan, "waiting"),
            &RejectionReason::DependencyNotSucceeded {
                dependency_id: "cancelled".to_owned()
            }
        );
        assert_eq!(reason(&plan, "denied"), &RejectionReason::Unauthorized);
        assert_eq!(
            reason(&plan, "ready"),
            &RejectionReason::LoadGateClosed(HOLD)
        );
    }

    #[test]
    fn a_closed_start_gate_reports_the_load_reason_even_when_the_worker_limits_are_full() {
        let tasks = [
            (contract("run-1", "project-a", &[]), TaskState::Running),
            (contract("run-2", "project-b", &[]), TaskState::Running),
            (contract("queued", "project-c", &[]), TaskState::Queued),
        ];
        let limits = SchedulerLimits::new(Some(2), Some(2)).unwrap();

        // The slot reason comes back the moment the gate opens.
        let open = pass(&tasks, limits, None, None, &allow_all);
        assert_eq!(
            reason(&open, "queued"),
            &RejectionReason::GlobalLimitReached
        );

        let closed = pass(&tasks, limits, Some(CLOSED), None, &allow_all);
        assert_eq!(
            reason(&closed, "queued"),
            &RejectionReason::LoadGateClosed(HOLD)
        );
    }

    #[test]
    fn the_load_gate_rejection_is_the_load_waiting_reason() {
        let reason = RejectionReason::LoadGateClosed(HOLD);
        assert_eq!(reason.waiting_reason(), WaitingReason::Load);
        assert_eq!(WaitingReason::Load.as_str(), "load");
    }
}
