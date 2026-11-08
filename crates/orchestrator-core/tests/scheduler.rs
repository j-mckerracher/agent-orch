use orchestrator_core::{
    scheduler::{
        RejectionReason, SchedulePlan, Scheduler, SchedulerError, SchedulerLimits,
        SchedulerSnapshot, StartAuthorization, TaskSnapshot,
    },
    task::TaskState,
    AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, VerificationPolicy, Workspace,
};

struct Fixture {
    contract: TaskContract,
    state: TaskState,
    workspace: Option<Workspace>,
}

fn contract(id: &str, dependencies: &[&str]) -> TaskContract {
    contract_in_project(id, "project", dependencies)
}

fn contract_in_project(id: &str, project_id: &str, dependencies: &[&str]) -> TaskContract {
    TaskContract {
        id: id.to_owned(),
        project_id: project_id.to_owned(),
        parent_id: None,
        objective: "test scheduler".to_owned(),
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
    }
}

fn fixture(id: &str, project_id: &str, state: TaskState, dependencies: &[&str]) -> Fixture {
    Fixture {
        contract: contract_in_project(id, project_id, dependencies),
        state,
        workspace: None,
    }
}

fn run(
    fixtures: &[Fixture],
    limits: SchedulerLimits,
    last_started_project: Option<&str>,
    authorization: Option<&dyn StartAuthorization>,
) -> Result<SchedulePlan, SchedulerError> {
    let tasks: Vec<_> = fixtures
        .iter()
        .map(|fixture| TaskSnapshot {
            contract: &fixture.contract,
            state: fixture.state,
            workspace: fixture.workspace.as_ref(),
        })
        .collect();
    let scheduler = Scheduler::new(
        SchedulerSnapshot {
            tasks: &tasks,
            last_started_project,
        },
        limits,
    )?;
    Ok(scheduler.schedule(authorization))
}

fn allow_all(_: &str, _: &str) -> bool {
    true
}

fn rejection<'a>(plan: &'a SchedulePlan, task_id: &str) -> &'a RejectionReason {
    &plan
        .rejected
        .iter()
        .find(|rejection| rejection.task_id == task_id)
        .expect("task has a rejection")
        .reason
}

fn cycle(tasks: &[TaskContract]) -> Vec<String> {
    let snapshots: Vec<_> = tasks
        .iter()
        .map(|contract| TaskSnapshot {
            contract,
            state: TaskState::Queued,
            workspace: None,
        })
        .collect();
    let error = Scheduler::new(
        SchedulerSnapshot {
            tasks: &snapshots,
            last_started_project: None,
        },
        SchedulerLimits::default(),
    )
    .unwrap_err();

    match error {
        orchestrator_core::scheduler::SchedulerError::DependencyCycle { cycle } => cycle,
        other => panic!("expected dependency-cycle error, got {other:?}"),
    }
}

#[test]
fn rejects_self_dependency_with_named_cycle() {
    assert_eq!(cycle(&[contract("self", &["self"])]), ["self", "self"]);
}

#[test]
fn rejects_long_dependency_cycle_with_named_path() {
    let tasks = [
        contract("a", &["b"]),
        contract("b", &["c"]),
        contract("c", &["a"]),
    ];

    assert_eq!(cycle(&tasks), ["a", "b", "c", "a"]);
}

#[test]
fn rejects_zero_and_unbounded_limits() {
    use orchestrator_core::scheduler::SchedulerLimitError;

    assert_eq!(
        SchedulerLimits::new(Some(0), Some(1)),
        Err(SchedulerLimitError::ZeroGlobal)
    );
    assert_eq!(
        SchedulerLimits::new(Some(1), Some(0)),
        Err(SchedulerLimitError::ZeroPerProject)
    );
    assert_eq!(
        SchedulerLimits::new(None, Some(1)),
        Err(SchedulerLimitError::UnboundedGlobal)
    );
    assert_eq!(
        SchedulerLimits::new(Some(1), None),
        Err(SchedulerLimitError::UnboundedPerProject)
    );
}

#[test]
fn default_global_limit_starts_two_tasks() {
    let tasks = [
        fixture("a", "a-project", TaskState::Queued, &[]),
        fixture("b", "b-project", TaskState::Queued, &[]),
        fixture("c", "c-project", TaskState::Queued, &[]),
    ];

    let plan = run(&tasks, SchedulerLimits::default(), None, Some(&allow_all)).unwrap();

    assert_eq!(plan.started, ["a", "b"]);
    assert_eq!(rejection(&plan, "c"), &RejectionReason::GlobalLimitReached);
}

#[test]
fn configured_global_limit_counts_running_and_started_tasks() {
    let tasks = [
        fixture("running", "a-project", TaskState::Running, &[]),
        fixture("a", "a-project", TaskState::Queued, &[]),
        fixture("b", "b-project", TaskState::Queued, &[]),
    ];
    let limits = SchedulerLimits::new(Some(2), Some(2)).unwrap();

    let plan = run(&tasks, limits, None, Some(&allow_all)).unwrap();

    assert_eq!(plan.started, ["a"]);
    assert_eq!(rejection(&plan, "b"), &RejectionReason::GlobalLimitReached);
}

#[test]
fn per_project_limit_applies_to_running_and_started_tasks() {
    let tasks = [
        fixture("running-a", "project-a", TaskState::Running, &[]),
        fixture("a1", "project-a", TaskState::Queued, &[]),
        fixture("a2", "project-a", TaskState::Queued, &[]),
        fixture("b1", "project-b", TaskState::Queued, &[]),
    ];
    let limits = SchedulerLimits::new(Some(4), Some(2)).unwrap();

    let plan = run(&tasks, limits, None, Some(&allow_all)).unwrap();

    assert_eq!(plan.started, ["a1", "b1"]);
    assert_eq!(
        rejection(&plan, "a2"),
        &RejectionReason::ProjectLimitReached
    );
}

fn workspace(task_id: &str, path: &str, branch: &str) -> Workspace {
    Workspace {
        id: format!("workspace-{task_id}"),
        task_id: task_id.to_owned(),
        path: path.to_owned(),
        branch: branch.to_owned(),
        base_commit: "base".to_owned(),
    }
}

#[test]
fn round_robins_projects_before_starting_second_task_from_one_project() {
    let tasks = [
        fixture("a-1", "project-a", TaskState::Queued, &[]),
        fixture("a-2", "project-a", TaskState::Queued, &[]),
        fixture("b-1", "project-b", TaskState::Queued, &[]),
    ];
    let limits = SchedulerLimits::new(Some(2), Some(2)).unwrap();

    let plan = run(&tasks, limits, None, Some(&allow_all)).unwrap();

    assert_eq!(plan.started, ["a-1", "b-1"]);
    assert_eq!(
        rejection(&plan, "a-2"),
        &RejectionReason::GlobalLimitReached
    );
}

#[test]
fn returned_cursor_fairly_rotates_next_scheduling_pass() {
    let first_snapshot = [
        fixture("a-1", "project-a", TaskState::Queued, &[]),
        fixture("a-2", "project-a", TaskState::Queued, &[]),
        fixture("b-1", "project-b", TaskState::Queued, &[]),
    ];
    let one_worker = SchedulerLimits::new(Some(1), Some(2)).unwrap();
    let first = run(&first_snapshot, one_worker, None, Some(&allow_all)).unwrap();
    assert_eq!(first.started, ["a-1"]);

    let next_snapshot = [
        fixture("a-1", "project-a", TaskState::Succeeded, &[]),
        fixture("a-2", "project-a", TaskState::Queued, &[]),
        fixture("b-1", "project-b", TaskState::Queued, &[]),
    ];
    let next = run(
        &next_snapshot,
        one_worker,
        first.last_started_project.as_deref(),
        Some(&allow_all),
    )
    .unwrap();

    assert_eq!(next.started, ["b-1"]);
}

#[test]
fn rejects_task_when_dependency_has_not_succeeded() {
    let tasks = [
        fixture("consumer", "project-a", TaskState::Queued, &["cancelled"]),
        fixture("cancelled", "project-a", TaskState::Cancelled, &[]),
    ];

    let plan = run(&tasks, SchedulerLimits::default(), None, Some(&allow_all)).unwrap();

    assert!(plan.started.is_empty());
    assert_eq!(
        rejection(&plan, "consumer"),
        &RejectionReason::DependencyNotSucceeded {
            dependency_id: "cancelled".to_owned()
        }
    );
}

#[test]
fn rejects_second_task_claiming_same_worktree() {
    let mut tasks = [
        fixture("a", "project-a", TaskState::Queued, &[]),
        fixture("b", "project-b", TaskState::Queued, &[]),
    ];
    tasks[0].workspace = Some(workspace("a", "/work/shared", "refs/heads/a"));
    tasks[1].workspace = Some(workspace("b", "/work/shared", "refs/heads/b"));

    let plan = run(
        &tasks,
        SchedulerLimits::new(Some(2), Some(2)).unwrap(),
        None,
        Some(&allow_all),
    )
    .unwrap();

    assert_eq!(plan.started, ["a"]);
    assert_eq!(
        rejection(&plan, "b"),
        &RejectionReason::WorktreeOwned {
            owner_task_id: "a".to_owned(),
            resource: orchestrator_core::scheduler::WorktreeResource::Path,
            value: "/work/shared".to_owned(),
        }
    );
}

#[test]
fn rejects_second_task_claiming_same_branch_path() {
    let mut tasks = [
        fixture("a", "project-a", TaskState::Queued, &[]),
        fixture("b", "project-a", TaskState::Queued, &[]),
    ];
    tasks[0].workspace = Some(workspace("a", "/work/a", "refs/heads/shared"));
    tasks[1].workspace = Some(workspace("b", "/work/b", "refs/heads/shared"));

    let plan = run(
        &tasks,
        SchedulerLimits::new(Some(2), Some(2)).unwrap(),
        None,
        Some(&allow_all),
    )
    .unwrap();

    assert_eq!(
        rejection(&plan, "b"),
        &RejectionReason::WorktreeOwned {
            owner_task_id: "a".to_owned(),
            resource: orchestrator_core::scheduler::WorktreeResource::Branch,
            value: "refs/heads/shared".to_owned(),
        }
    );
}

#[test]
fn missing_authorization_policy_denies_start() {
    let tasks = [fixture("a", "project-a", TaskState::Queued, &[])];

    let plan = run(&tasks, SchedulerLimits::default(), None, None).unwrap();

    assert!(plan.started.is_empty());
    assert_eq!(rejection(&plan, "a"), &RejectionReason::Unauthorized);
}

#[test]
fn authorization_can_deny_task_while_capacity_is_free() {
    let tasks = [fixture("a", "project-a", TaskState::Queued, &[])];
    let deny = |_: &str, _: &str| false;

    let plan = run(&tasks, SchedulerLimits::default(), None, Some(&deny)).unwrap();

    assert!(plan.started.is_empty());
    assert_eq!(rejection(&plan, "a"), &RejectionReason::Unauthorized);
}

#[test]
fn scheduling_is_deterministic_under_task_and_dependency_permutations() {
    let first_snapshot = [
        fixture("a", "project-a", TaskState::Queued, &["done-b", "done-a"]),
        fixture("done-a", "project-z", TaskState::Succeeded, &[]),
        fixture("done-b", "project-y", TaskState::Succeeded, &[]),
        fixture("b", "project-b", TaskState::Queued, &[]),
    ];
    let permuted_snapshot = [
        fixture("b", "project-b", TaskState::Queued, &[]),
        fixture("done-b", "project-y", TaskState::Succeeded, &[]),
        fixture("a", "project-a", TaskState::Queued, &["done-a", "done-b"]),
        fixture("done-a", "project-z", TaskState::Succeeded, &[]),
    ];

    let first = run(
        &first_snapshot,
        SchedulerLimits::default(),
        None,
        Some(&allow_all),
    )
    .unwrap();
    let permuted = run(
        &permuted_snapshot,
        SchedulerLimits::default(),
        None,
        Some(&allow_all),
    )
    .unwrap();

    assert_eq!(first, permuted);
}

#[test]
fn rejects_duplicate_claims_already_owned_by_running_tasks() {
    let mut tasks = [
        fixture("a", "project-a", TaskState::Running, &[]),
        fixture("b", "project-a", TaskState::Running, &[]),
    ];
    tasks[0].workspace = Some(workspace("a", "/work/a", "refs/heads/shared"));
    tasks[1].workspace = Some(workspace("b", "/work/b", "refs/heads/shared"));

    assert_eq!(
        run(&tasks, SchedulerLimits::default(), None, Some(&allow_all),),
        Err(SchedulerError::DuplicateRunningClaim {
            first_task_id: "a".to_owned(),
            second_task_id: "b".to_owned(),
            resource: orchestrator_core::scheduler::WorktreeResource::Branch,
            value: "refs/heads/shared".to_owned(),
        })
    );
}

#[test]
fn missing_dependency_cannot_start_task() {
    let tasks = [fixture(
        "consumer",
        "project-a",
        TaskState::Queued,
        &["missing"],
    )];

    let plan = run(&tasks, SchedulerLimits::default(), None, Some(&allow_all)).unwrap();

    assert!(plan.started.is_empty());
    assert_eq!(
        rejection(&plan, "consumer"),
        &RejectionReason::DependencyNotSucceeded {
            dependency_id: "missing".to_owned(),
        }
    );
}

#[test]
fn same_branch_name_in_different_projects_does_not_conflict() {
    let mut tasks = [
        fixture("a", "project-a", TaskState::Queued, &[]),
        fixture("b", "project-b", TaskState::Queued, &[]),
    ];
    tasks[0].workspace = Some(workspace("a", "/work/a", "refs/heads/shared"));
    tasks[1].workspace = Some(workspace("b", "/work/b", "refs/heads/shared"));

    let plan = run(
        &tasks,
        SchedulerLimits::new(Some(2), Some(2)).unwrap(),
        None,
        Some(&allow_all),
    )
    .unwrap();

    assert_eq!(plan.started, ["a", "b"]);
    assert!(plan.rejected.is_empty());
}
