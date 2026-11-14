//! Restart reconciliation gate shared by the CLI and the daemon.
//!
//! Scheduling may resume only after [`reconcile_before_scheduling`] succeeds.
//! The gate is fail-closed: every unresolved record becomes a blocker string
//! and the whole call returns `Err`. A session whose process is proven gone
//! has its recorded workspace inspected first: Foreign or Unknown evidence, or
//! Partial/Ambiguous (or Confirmed-on-unsucceeded) delivery evidence, refuses
//! scheduling and leaves the records unfinished for re-inspection; Dirty,
//! Missing or BranchMoved keeps the worktree and takes the failure transition.
//! Tasks whose run owner is still observed alive are skipped for the process,
//! attempt and workspace checks because that owner finishes them itself.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use orchestrator_core::identity::ProcessClassification;
use orchestrator_core::records::Workspace;
use orchestrator_core::task::{AttemptState, SessionState, TaskEvent, TaskState};
use orchestrator_diagnostics::Diagnostics;
use orchestrator_git::delivery::{check_worktree_safe_to_delete, DeliveryAction};
use orchestrator_git::delivery_reconcile::{reconcile_delivery_steps, RecordedDeliveryStep};
use orchestrator_git::reconcile::{
    reconcile_workspaces, RecordedWorkspace, WorkspaceClassification,
};
use orchestrator_git::Repository;
use orchestrator_runtime::reconcile::{
    classify_interrupted_task, delivery_evidence, observe_owner_process, reconcile_processes,
    scheduling_may_resume, workspace_evidence, DeliveryEvidence, ProcessAction, ProcessSubject,
    ReconciliationResolution, RecordedProcess, TaskEvidence, WorkspaceEvidence,
};
use orchestrator_store::{CancellationStatus, SqliteStore, Store};

/// Reconciles every unfinished record against actual process, Git and delivery
/// state. `Ok` means scheduling may resume; `Err` carries every blocker.
pub fn reconcile_before_scheduling(store: &SqliteStore) -> Result<(), String> {
    reconcile_before_scheduling_with(store, &Diagnostics::stderr())
}

/// Same gate, reporting through `diagnostics` so records that name a workspace
/// path or a blocker pass the redactor like every other daemon line.
pub fn reconcile_before_scheduling_with(
    store: &SqliteStore,
    diagnostics: &Diagnostics,
) -> Result<(), String> {
    // Read the live owners before the task snapshot. An owner that finishes
    // after this call has already closed or is still skipped here, and the
    // later task read can only be newer than the owner set. Reading tasks
    // first could pair a stale mid-finish task with an owner that has since
    // closed, which reports a spurious blocker or a terminal-session error.
    let finishing = tasks_with_live_owner(store)?;
    let tasks = store
        .list()
        .map_err(|error| format!("cannot list tasks for reconciliation: {error}"))?;
    let mut workspaces_by_task: BTreeMap<String, Vec<Workspace>> = BTreeMap::new();
    for workspace in store
        .list_workspaces()
        .map_err(|error| format!("cannot list workspaces for reconciliation: {error}"))?
    {
        workspaces_by_task
            .entry(workspace.task_id.clone())
            .or_default()
            .push(workspace);
    }
    let mut delivery_steps: BTreeMap<String, Vec<serde_json::Value>> = BTreeMap::new();
    for step in store
        .all_delivery_steps()
        .map_err(|error| format!("cannot read delivery journal for reconciliation: {error}"))?
    {
        let task_id = step
            .get("task_id")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        if let Some(task_id) = task_id {
            delivery_steps.entry(task_id).or_default().push(step);
        }
    }
    let projects: BTreeMap<&str, &str> = tasks
        .iter()
        .map(|task| (task.contract.id.as_str(), task.contract.project_id.as_str()))
        .collect();
    let mut delivery_by_task: BTreeMap<String, DeliveryEvidence> = BTreeMap::new();
    let mut delivery_blocked = BTreeSet::new();
    let mut processes = Vec::new();
    let mut owners = BTreeMap::new();
    let mut other_records = Vec::new();
    let mut blockers = Vec::new();

    for task in &tasks {
        // A persisted delivery journal is read for every task, settled, live
        // owned or not. Partial or ambiguous delivery, or a confirmed delivery
        // on a task that is not Succeeded, blocks scheduling. Nothing here
        // applies, removes or repairs a delivery. Consistent Confirmed/
        // NotApplied does not block. This runs before the live-owner skip below
        // because that exemption covers only the process, attempt and workspace
        // checks, not unresolved delivery.
        if let Some(steps) = delivery_steps.get(&task.contract.id) {
            let (evidence, detail) = match inspect_task_delivery(&task.contract.project_id, steps) {
                Ok(evidence) => (evidence, String::new()),
                Err(error) => (DeliveryEvidence::Ambiguous, format!(" ({error})")),
            };
            delivery_by_task.insert(task.contract.id.clone(), evidence);
            if delivery_blocks(task.state, evidence) {
                other_records.push(ReconciliationResolution::Unresolved);
                delivery_blocked.insert(task.contract.id.clone());
                blockers.push(format!(
                    "task {0} has {evidence:?} delivery evidence while {1:?}{detail}; the \
                     delivery journal, review refs, worktree and history are preserved. Next \
                     step: run `agent-orch inspect {0}` and review the recorded delivery steps \
                     and `refs/heads/orchestrator/review/` refs by hand. No override exists \
                     and nothing here applies or removes a delivery, so scheduling stays \
                     blocked until the journal and Git agree",
                    task.contract.id, task.state
                ));
            }
        }
        // A run owner finishes without the data lock, so its session, attempt
        // and task writes land one at a time. While its exact process is
        // observed Alive, those intermediate states are not corruption and the
        // owner, not this reconciliation, will complete them.
        if finishing.contains(&task.contract.id) {
            continue;
        }
        let unfinished = task
            .sessions
            .iter()
            .filter(|session| !session.state.is_terminal())
            .collect::<Vec<_>>();
        if task.state == TaskState::Cancelled
            && unfinished.is_empty()
            && task.sessions.iter().rev().any(|session| {
                task.attempts.iter().any(|attempt| {
                    attempt.id == session.attempt_id && attempt.state == AttemptState::Cancelled
                })
            })
            && store
                .cancellation_status_for_task(&task.contract.id)
                .map_err(|error| {
                    format!(
                        "cannot verify cancellation for task {}: {error}",
                        task.contract.id
                    )
                })?
                != Some(CancellationStatus::Confirmed)
        {
            other_records.push(ReconciliationResolution::Ambiguous);
            blockers.push(format!(
                "task {} is terminal Cancelled without run-owner confirmation; needs review",
                task.contract.id
            ));
        }
        if task.state == TaskState::Running && unfinished.is_empty() {
            other_records.push(ReconciliationResolution::Ambiguous);
            blockers.push(format!(
                "task {} is Running without an unfinished session",
                task.contract.id
            ));
        } else {
            // An attempt that never finished (Pending or Running) must have an
            // unfinished session; otherwise nobody owns it and its owner cannot
            // be proven gone. A CandidateReady attempt is a settled candidate
            // awaiting review, and a task with no attempt is just planned.
            for attempt in task.attempts.iter().filter(|attempt| {
                matches!(attempt.state, AttemptState::Pending | AttemptState::Running)
                    && !unfinished
                        .iter()
                        .any(|session| session.attempt_id == attempt.id)
            }) {
                other_records.push(ReconciliationResolution::Ambiguous);
                blockers.push(format!(
                    "attempt {}/{} is {:?} but has no unfinished session, so no owner or process \
                     identity can be proven gone; the task, worktree and history are preserved. \
                     Next step: run `agent-orch inspect {0}`; no override exists",
                    task.contract.id, attempt.id, attempt.state
                ));
            }
        }
        for session in unfinished {
            let subject = ProcessSubject::Session {
                task_id: task.contract.id.clone(),
                session_id: session.id.clone(),
            };
            let Some(attempt) = task
                .attempts
                .iter()
                .find(|attempt| attempt.id == session.attempt_id)
            else {
                other_records.push(ReconciliationResolution::Ambiguous);
                blockers.push(format!(
                    "session {}/{} references missing attempt {}",
                    task.contract.id, session.id, session.attempt_id
                ));
                continue;
            };
            match store
                .session_identity(&task.contract.id, &session.id)
                .map_err(|error| {
                    format!(
                        "cannot read identity for session {}/{}: {error}",
                        task.contract.id, session.id
                    )
                })? {
                Some(identity) => {
                    owners.insert(
                        subject.clone(),
                        (attempt.id.clone(), task.state, attempt.state),
                    );
                    processes.push(RecordedProcess { subject, identity });
                }
                None => {
                    other_records.push(ReconciliationResolution::Unresolved);
                    blockers.push(format!(
                        "session {0}/{1} has no recorded process identity (a legacy record or a \
                         run interrupted before identity capture; current runs fail instead of \
                         creating one). Next step: run `agent-orch inspect {0}` to read the \
                         recorded agent PID and preserved worktree, confirm by hand that the \
                         agent and its child processes are gone (stop them if not), and keep the \
                         worktree and logs. No override exists for this record, so scheduling \
                         stays blocked on this data directory until it is resolved or the \
                         directory is retired",
                        task.contract.id, session.id
                    ));
                }
            }
        }
    }

    let decisions = reconcile_processes(&processes);
    for decision in &decisions {
        let ProcessSubject::Session {
            task_id,
            session_id,
        } = &decision.subject
        else {
            other_records.push(ReconciliationResolution::Ambiguous);
            blockers.push(format!("unexpected process record {:?}", decision.subject));
            continue;
        };
        let Some((attempt_id, task_state, attempt_state)) = owners.get(&decision.subject) else {
            other_records.push(ReconciliationResolution::Ambiguous);
            blockers.push(format!(
                "session {task_id}/{session_id} has no owner record"
            ));
            continue;
        };
        match decision.action {
            ProcessAction::LeaveRunning
                if *task_state == TaskState::Running && *attempt_state == AttemptState::Running => {
            }
            ProcessAction::LeaveRunning => {
                other_records.push(ReconciliationResolution::Ambiguous);
                blockers.push(format!(
                    "session {task_id}/{session_id} is alive while task is {task_state:?} and attempt is {attempt_state:?}"
                ));
            }
            ProcessAction::MarkInterrupted => {
                // Inspect the recorded workspace and delivery BEFORE any state
                // write. A blocking inspection leaves the unfinished session,
                // attempt and task untouched, so the next run re-inspects the
                // same evidence instead of finding a settled-looking record.
                if delivery_blocked.contains(task_id) {
                    continue;
                }
                let workspace = inspect_task_workspaces(
                    projects.get(task_id.as_str()).copied().unwrap_or_default(),
                    workspaces_by_task
                        .get(task_id.as_str())
                        .map_or(&[][..], Vec::as_slice),
                );
                if matches!(
                    workspace,
                    WorkspaceEvidence::Foreign | WorkspaceEvidence::Unknown
                ) {
                    other_records.push(ReconciliationResolution::Unresolved);
                    blockers.push(format!(
                        "interrupted session {task_id}/{session_id} has {workspace:?} workspace \
                         evidence (the recorded worktree is missing from the store, is not a \
                         registered worktree of the project repository, or could not be \
                         inspected); its session, attempt, task, worktree and history are \
                         preserved unchanged and will be re-inspected on the next run. Next \
                         step: run `agent-orch inspect {task_id}`, save anything in the \
                         recorded worktree path, then restore the registered worktree or move \
                         the foreign path aside. No override exists"
                    ));
                    continue;
                }
                let recovery = classify_interrupted_task(TaskEvidence {
                    task: *task_state,
                    attempt: *attempt_state,
                    process: decision.classification,
                    workspace,
                    delivery: delivery_by_task
                        .get(task_id.as_str())
                        .copied()
                        .unwrap_or(DeliveryEvidence::None),
                });
                // Gate policy, not a classifier override: a proven-exited task
                // with a Dirty, Missing or BranchMoved known workspace keeps
                // its worktree and takes the existing failure resolution, so
                // one ordinary crash cannot block every later run. The advisory
                // classification below never means success, cleanup or delivery.
                // The worktree is preserved and this is never recorded as success.
                diagnostics.warn(
                    "engine",
                    "reconcile_interrupted_task",
                    &[
                        ("task_id", task_id.as_str()),
                        ("session_id", session_id.as_str()),
                        ("workspace", &format!("{workspace:?}")),
                        ("advisory_classification", &format!("{recovery:?}")),
                    ],
                );
                store
                    .update_session_state(task_id, session_id, SessionState::Exited)
                    .map_err(|error| {
                        format!("cannot mark session {task_id}/{session_id} exited: {error}")
                    })?;
                match (*task_state, *attempt_state) {
                    (TaskState::Running, AttemptState::Running) => {
                        store
                            .update_attempt_state(task_id, attempt_id, AttemptState::Failed)
                            .map_err(|error| {
                                format!("cannot interrupt attempt {attempt_id}: {error}")
                            })?;
                        store
                            .transition(task_id, TaskEvent::ExecutionFailed)
                            .map_err(|error| {
                                format!("cannot fail interrupted task {task_id}: {error}")
                            })?;
                    }
                    (TaskState::Running, AttemptState::Cancelled) => {
                        other_records.push(ReconciliationResolution::Ambiguous);
                        blockers.push(format!(
                            "task {task_id} has a cancelled attempt without run-owner \
                             confirmation; refusing to promote it to Cancelled"
                        ));
                    }
                    (TaskState::Cancelled, AttemptState::Cancelled) => {
                        let confirmed =
                            store
                                .cancellation_status_for_task(task_id)
                                .map_err(|error| {
                                    format!(
                                        "cannot verify cancellation for task {task_id}: {error}"
                                    )
                                })?
                                == Some(CancellationStatus::Confirmed);
                        if !confirmed {
                            other_records.push(ReconciliationResolution::Ambiguous);
                            blockers.push(format!(
                                "task {task_id} was marked Cancelled without run-owner \
                                 confirmation; needs review"
                            ));
                        }
                    }
                    (TaskState::Failed, AttemptState::Failed) => {}
                    _ => {
                        other_records.push(ReconciliationResolution::Ambiguous);
                        blockers.push(format!(
                            "interrupted session {task_id}/{session_id} has task state {task_state:?} and attempt state {attempt_state:?}"
                        ));
                    }
                }
            }
            ProcessAction::MarkNeedsReview => {
                other_records.push(ReconciliationResolution::Unresolved);
                blockers.push(format!(
                    "session {task_id}/{session_id} needs review: {:?}",
                    decision.classification
                ));
            }
        }
    }

    for blocker in reconcile_cancellation_owners(store)? {
        other_records.push(ReconciliationResolution::Unresolved);
        blockers.push(blocker);
    }

    if scheduling_may_resume(&decisions, &other_records) {
        return Ok(());
    }
    for blocker in &blockers {
        diagnostics.warn(
            "engine",
            "reconcile_blocking_record",
            &[("blocker", blocker.as_str())],
        );
    }
    Err(format!(
        "cannot schedule while reconciliation records are unresolved: {}",
        blockers.join("; ")
    ))
}

/// Read-only observation of a task's recorded workspaces through the Git
/// reconciliation adapters. Missing records, an unusable repository or any
/// failed inspection is `Unknown`; the worst workspace decides.
fn inspect_task_workspaces(project_id: &str, recorded: &[Workspace]) -> WorkspaceEvidence {
    let Ok(repository) = Repository::new(Path::new(project_id)) else {
        return WorkspaceEvidence::Unknown;
    };
    if recorded.is_empty() {
        return WorkspaceEvidence::Unknown;
    }
    let inputs: Vec<_> = recorded
        .iter()
        .map(|workspace| RecordedWorkspace {
            id: workspace.id.clone(),
            path: PathBuf::from(&workspace.path),
            branch: workspace.branch.clone(),
            base_commit: workspace.base_commit.clone(),
        })
        .collect();
    let Ok(plan) = reconcile_workspaces(&repository, &inputs) else {
        return WorkspaceEvidence::Unknown;
    };
    let rank = |evidence| match evidence {
        WorkspaceEvidence::Foreign => 5,
        WorkspaceEvidence::Unknown => 4,
        WorkspaceEvidence::Dirty => 3,
        WorkspaceEvidence::BranchMoved => 2,
        WorkspaceEvidence::Missing => 1,
        WorkspaceEvidence::Matching => 0,
    };
    plan.workspaces
        .iter()
        .map(|decision| {
            let present = matches!(
                decision.classification,
                WorkspaceClassification::PresentAndMatching | WorkspaceClassification::BranchMoved
            );
            let safety = present
                .then(|| {
                    check_worktree_safe_to_delete(
                        &repository,
                        &decision.path,
                        &decision.base_commit,
                    )
                    .ok()
                })
                .flatten();
            workspace_evidence(Some(decision), safety.as_ref())
        })
        .max_by_key(|evidence| rank(*evidence))
        .unwrap_or(WorkspaceEvidence::Unknown)
}

/// Read-only comparison of a task's persisted delivery steps with Git refs.
fn inspect_task_delivery(
    project_id: &str,
    steps: &[serde_json::Value],
) -> Result<DeliveryEvidence, String> {
    let repository = Repository::new(Path::new(project_id))
        .map_err(|error| format!("cannot open repository {project_id}: {error}"))?;
    let text = |step: &serde_json::Value, key: &str| {
        step.get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    };
    let recorded: Vec<_> = steps
        .iter()
        .map(|step| RecordedDeliveryStep {
            id: text(step, "id").unwrap_or_default(),
            task_id: text(step, "task_id"),
            action: match text(step, "delivery_mode").as_deref() {
                Some("report_only") => Some(DeliveryAction::ReportOnly),
                Some("local_review") => Some(DeliveryAction::LocalReview),
                _ => None,
            },
            candidate_sha: text(step, "candidate_sha"),
            destination: text(step, "destination").map(PathBuf::from),
            target_ref: text(step, "target_ref"),
        })
        .collect();
    let plan = reconcile_delivery_steps(&repository, &recorded)
        .map_err(|error| format!("cannot inspect delivery refs: {error}"))?;
    Ok(delivery_evidence(&plan.steps))
}

/// Partial or ambiguous delivery blocks, as does a confirmed delivery on a
/// task that is not Succeeded; consistent Confirmed/NotApplied does not.
pub fn delivery_blocks(task: TaskState, evidence: DeliveryEvidence) -> bool {
    match evidence {
        DeliveryEvidence::Partial | DeliveryEvidence::Ambiguous => true,
        DeliveryEvidence::Confirmed => task != TaskState::Succeeded,
        DeliveryEvidence::None | DeliveryEvidence::NotApplied => false,
    }
}

/// Task IDs whose open cancellation owner has an exact identity observed Alive.
/// Dead, mismatched, unknown, or missing identities are excluded so they stay
/// fail-closed.
fn tasks_with_live_owner(store: &SqliteStore) -> Result<BTreeSet<String>, String> {
    let owners = store
        .open_cancellation_owners()
        .map_err(|error| format!("cannot list cancellation owners for reconciliation: {error}"))?;
    Ok(owners
        .into_iter()
        .filter(|owner| {
            owner.exact_process_identity().map(observe_owner_process)
                == Some(ProcessClassification::Alive)
        })
        .map(|owner| owner.task_id)
        .collect())
}

/// Retires owner generations whose exact process is observed exited and
/// returns a blocker for every owner whose identity cannot be resolved.
pub fn reconcile_cancellation_owners(store: &SqliteStore) -> Result<Vec<String>, String> {
    let owners = store
        .open_cancellation_owners()
        .map_err(|error| format!("cannot list cancellation owners for reconciliation: {error}"))?;
    let mut blockers = Vec::new();
    for owner in owners {
        let identity = owner.exact_process_identity();
        let classification = identity.map(observe_owner_process);
        match classification {
            Some(ProcessClassification::Exited) => store
                .retire_stale_cancellation_owner(&owner)
                .map_err(|error| {
                    format!(
                        "cannot retire stale cancellation owner for task {} session {}: {error}",
                        owner.task_id, owner.session_id
                    )
                })?,
            Some(ProcessClassification::IdentityMismatch | ProcessClassification::Unknown)
            | None => {
                blockers.push(format!(
                    "cancellation owner for task {} session {} generation {} has unresolved \
                         exact process identity ({classification:?}); automatic recovery is \
                         fail-closed. After confirming the owner and managed process tree have \
                         exited and the target attempt/session are terminal, run `agent-orch \
                         recover-owner {} --attempt-id {} --session-id {} --generation {} \
                         --confirm-processes-exited --reason \"<reason>\"`",
                    owner.task_id,
                    owner.session_id,
                    owner.generation(),
                    owner.task_id,
                    owner.attempt_id,
                    owner.session_id,
                    owner.generation()
                ));
            }
            _ => {}
        }
    }
    Ok(blockers)
}
