#[cfg(unix)]
use orchestrator_core::identity::ProcessAction;
use orchestrator_core::identity::{
    classify_exact_process_identity, plan_process_reconciliation, ExactProcessIdentity,
    ProcessClassification, ProcessIdentity, ProcessObservation, ProcessPlatform,
};
#[cfg(unix)]
use orchestrator_runtime::reconcile::observe_process;
use orchestrator_runtime::reconcile::{
    capture_exact_process_identity, capture_process_identity, observe_owner_process,
    reconcile_processes, scheduling_may_resume, ProcessSubject, ReconciliationResolution,
    RecordedProcess,
};
#[cfg(unix)]
use orchestrator_runtime::supervisor::{ChildSupervisor, LaunchRequest, SupervisionEvent};

#[test]
fn changed_start_time_is_an_identity_mismatch() {
    let expected = ProcessIdentity {
        pid: 42,
        process_group: Some(42),
        start_time_micros: Some(100_000_010),
        platform: ProcessPlatform::MacOS,
    };
    let observed = ProcessIdentity {
        start_time_micros: Some(101_000_010),
        ..expected
    };

    let plan = plan_process_reconciliation(expected, ProcessObservation::Alive(observed));
    assert_eq!(plan.classification, ProcessClassification::IdentityMismatch);
}

#[test]
fn windows_identity_compares_creation_time_at_100ns_precision() {
    let expected = ExactProcessIdentity {
        identity: ProcessIdentity {
            pid: 42,
            process_group: None,
            start_time_micros: Some(100_000_010),
            platform: ProcessPlatform::Windows,
        },
        windows_creation_time_100ns_since_unix_epoch: Some(1_000_000_102),
    };
    let reused_pid = ExactProcessIdentity {
        windows_creation_time_100ns_since_unix_epoch: Some(1_000_000_103),
        ..expected
    };
    let legacy_identity = ExactProcessIdentity {
        windows_creation_time_100ns_since_unix_epoch: None,
        ..expected
    };

    assert_eq!(
        classify_exact_process_identity(expected, expected),
        ProcessClassification::Alive
    );
    assert_eq!(
        classify_exact_process_identity(expected, reused_pid),
        ProcessClassification::IdentityMismatch
    );
    assert_eq!(
        classify_exact_process_identity(legacy_identity, legacy_identity),
        ProcessClassification::Unknown
    );
}

/// Returns a creation time that differs from `creation_time` but stays in the
/// same microsecond bucket, so it remains consistent with an unchanged
/// `start_time_micros` (`creation_time / 10`) for every 100ns remainder.
fn forged_creation_time_in_same_microsecond(creation_time: u64) -> u64 {
    creation_time - creation_time % 10 + (creation_time % 10 + 1) % 10
}

#[test]
fn forged_windows_creation_time_is_a_mismatch_for_every_100ns_remainder() {
    for remainder in 0..10_u64 {
        let creation_time = 1_700_000_000_123_456_u64 * 10 + remainder;
        let actual = ExactProcessIdentity {
            identity: ProcessIdentity {
                pid: 42,
                process_group: None,
                start_time_micros: Some(creation_time / 10),
                platform: ProcessPlatform::Windows,
            },
            windows_creation_time_100ns_since_unix_epoch: Some(creation_time),
        };
        let forged = ExactProcessIdentity {
            windows_creation_time_100ns_since_unix_epoch: Some(
                forged_creation_time_in_same_microsecond(creation_time),
            ),
            ..actual
        };

        assert_eq!(
            classify_exact_process_identity(forged, actual),
            ProcessClassification::IdentityMismatch,
            "remainder {remainder}"
        );
    }
}

#[test]
fn scheduling_gate_blocks_unresolved_or_ambiguous_records() {
    assert!(!scheduling_may_resume(
        &[],
        &[
            ReconciliationResolution::Resolved,
            ReconciliationResolution::Ambiguous
        ]
    ));
    assert!(!scheduling_may_resume(
        &[],
        &[ReconciliationResolution::Unresolved]
    ));
    assert!(scheduling_may_resume(
        &[],
        &[
            ReconciliationResolution::Resolved,
            ReconciliationResolution::Resolved
        ]
    ));
    assert!(scheduling_may_resume(&[], &[]));
}

#[test]
fn process_plan_order_is_deterministic() {
    let identity = capture_process_identity(std::process::id()).expect("capture current process");
    let records = [
        RecordedProcess {
            subject: ProcessSubject::Session {
                task_id: "task".into(),
                session_id: "session".into(),
            },
            identity,
        },
        RecordedProcess {
            subject: ProcessSubject::Attempt {
                task_id: "task".into(),
                attempt_id: "attempt".into(),
            },
            identity,
        },
    ];
    let reversed = [records[1].clone(), records[0].clone()];

    assert_eq!(
        reconcile_processes(&records),
        reconcile_processes(&reversed)
    );
}

#[cfg(unix)]
#[tokio::test]
async fn supervisor_exposes_identity_captured_at_launch() {
    let mut supervisor = ChildSupervisor::launch(&LaunchRequest {
        program: "/bin/sleep".into(),
        args: vec!["0.2".into()],
        working_dir: std::env::current_dir().expect("current directory"),
        env_names: Vec::new(),
    })
    .expect("launch child");
    let identity = supervisor.identity().expect("launch identity");
    assert_eq!(identity.pid, supervisor.pid());
    #[cfg(target_os = "macos")]
    {
        assert_eq!(identity.process_group, Some(identity.pid));
        assert!(identity.start_time_micros.is_some());
    }

    assert!(matches!(
        supervisor.wait().await.expect("wait for child"),
        SupervisionEvent::Exited { .. }
    ));
}

#[cfg(unix)]
#[test]
fn exited_child_is_interrupted_not_succeeded() {
    use std::os::unix::process::CommandExt;
    use std::process::Command;

    let mut child = Command::new("/bin/sleep")
        .arg("0.2")
        .process_group(0)
        .spawn()
        .expect("spawn short-lived process");
    let exact_identity =
        capture_exact_process_identity(child.id()).expect("capture exact child identity");
    let identity = exact_identity.identity;
    let status = child.wait().expect("wait for child");
    assert!(status.success());

    let plan = reconcile_processes(&[RecordedProcess {
        subject: ProcessSubject::Session {
            task_id: "task".into(),
            session_id: "session".into(),
        },
        identity,
    }]);

    assert_eq!(plan[0].classification, ProcessClassification::Exited);
    assert_eq!(plan[0].action, ProcessAction::MarkInterrupted);
}

#[cfg(unix)]
#[test]
fn exited_owner_is_not_hidden_by_live_process_group_parent() {
    use std::process::Command;

    let parent_pid = std::process::id();
    let parent_group = unsafe { libc::getpgid(parent_pid as i32) };
    assert!(parent_group > 0, "test parent must have a process group");

    let mut child = Command::new("/bin/sleep")
        .arg("30")
        .spawn()
        .expect("spawn child in inherited process group");
    let child_group = unsafe { libc::getpgid(child.id() as i32) };
    assert_eq!(
        child_group, parent_group,
        "child must share the live parent's group"
    );
    let exact_identity =
        capture_exact_process_identity(child.id()).expect("capture exact child identity");
    let identity = exact_identity.identity;

    child.kill().expect("kill only the child process");
    let status = child.wait().expect("reap child");
    assert!(!status.success(), "child must exit from the direct kill");

    assert_eq!(
        observe_process(identity),
        ProcessObservation::Unknown,
        "the generic observer remains group-aware for process-tree records"
    );
    assert_eq!(
        observe_owner_process(exact_identity),
        ProcessClassification::Exited,
        "a live parent in the inherited group must not keep the exact owner unresolved"
    );
}

#[test]
fn owner_observation_requires_a_creation_time() {
    let mut identity =
        capture_exact_process_identity(std::process::id()).expect("capture current process");
    identity.identity.start_time_micros = None;

    assert_eq!(
        observe_owner_process(identity),
        ProcessClassification::Unknown
    );
}

#[test]
fn owner_observation_classifies_alive_and_reused_pid_by_creation_time() {
    let identity =
        capture_exact_process_identity(std::process::id()).expect("capture current process");
    let Some(start_time_micros) = identity.identity.start_time_micros else {
        assert_eq!(
            observe_owner_process(identity),
            ProcessClassification::Unknown
        );
        return;
    };

    assert_eq!(
        observe_owner_process(identity),
        ProcessClassification::Alive
    );
    let reused_pid = ExactProcessIdentity {
        identity: ProcessIdentity {
            start_time_micros: Some(start_time_micros + 1),
            ..identity.identity
        },
        windows_creation_time_100ns_since_unix_epoch: identity
            .windows_creation_time_100ns_since_unix_epoch
            .map(|time| time + 10),
    };
    // Windows creation time is immutable, so a mismatch proves PID reuse. On
    // Unix the recorded group is still alive here, so a start-time change is
    // wall-clock drift and must stay unproven (see the drift tests below).
    #[cfg(windows)]
    assert_eq!(
        observe_owner_process(reused_pid),
        ProcessClassification::Exited
    );
    #[cfg(unix)]
    assert_eq!(
        observe_owner_process(reused_pid),
        ProcessClassification::Unknown
    );
}

#[cfg(unix)]
fn owner_with_shifted_start(process_group: Option<u32>) -> ExactProcessIdentity {
    let identity =
        capture_exact_process_identity(std::process::id()).expect("capture current process");
    ExactProcessIdentity {
        identity: ProcessIdentity {
            process_group,
            start_time_micros: identity
                .identity
                .start_time_micros
                .map(|time| time + 1_000_000),
            ..identity.identity
        },
        ..identity
    }
}

#[cfg(unix)]
#[test]
fn wall_clock_start_drift_with_live_recorded_group_is_not_exited() {
    let live_group = capture_exact_process_identity(std::process::id())
        .expect("capture current process")
        .identity
        .process_group;
    assert!(live_group.is_some());
    assert_eq!(
        observe_owner_process(owner_with_shifted_start(live_group)),
        ProcessClassification::Unknown,
        "a live recorded group means the start-time change is not proof of PID reuse"
    );
}

#[cfg(unix)]
#[test]
fn start_mismatch_without_a_recorded_group_is_not_exited() {
    assert_eq!(
        observe_owner_process(owner_with_shifted_start(None)),
        ProcessClassification::Unknown,
        "no recorded group means death cannot be proven"
    );
}

#[cfg(unix)]
#[test]
fn start_mismatch_with_provably_dead_recorded_group_is_exited() {
    use std::os::unix::process::CommandExt;
    use std::process::Command;

    let mut child = Command::new("/bin/sleep")
        .arg("0.1")
        .process_group(0)
        .spawn()
        .expect("spawn group leader");
    let dead_group = child.id();
    child.wait().expect("reap group leader");
    assert!(!orchestrator_runtime::supervisor::process_group_alive(
        dead_group
    ));
    assert_eq!(
        observe_owner_process(owner_with_shifted_start(Some(dead_group))),
        ProcessClassification::Exited
    );
}

#[cfg(unix)]
#[test]
fn live_owner_with_changed_process_group_is_not_exited() {
    let identity =
        capture_exact_process_identity(std::process::id()).expect("capture current process");
    assert!(identity.identity.start_time_micros.is_some());
    let recorded_group = identity.identity.process_group.expect("unix process group");
    let moved_group = ExactProcessIdentity {
        identity: ProcessIdentity {
            process_group: Some(recorded_group.wrapping_add(1)),
            ..identity.identity
        },
        ..identity
    };

    assert_eq!(
        observe_owner_process(moved_group),
        ProcessClassification::Alive,
        "same pid and start time is the same live owner regardless of group"
    );
}

#[cfg(windows)]
#[test]
fn windows_owner_observation_uses_creation_time_not_only_pid() {
    let identity = capture_exact_process_identity(std::process::id())
        .expect("capture current Windows process");
    assert_eq!(identity.identity.platform, ProcessPlatform::Windows);
    assert!(identity.identity.start_time_micros.is_some());
    let creation_time = identity
        .windows_creation_time_100ns_since_unix_epoch
        .expect("capture exact Windows creation time");
    assert_eq!(
        observe_owner_process(identity),
        ProcessClassification::Alive
    );

    let reused_pid = ExactProcessIdentity {
        windows_creation_time_100ns_since_unix_epoch: Some(
            forged_creation_time_in_same_microsecond(creation_time),
        ),
        ..identity
    };
    assert_eq!(
        observe_owner_process(reused_pid),
        ProcessClassification::Exited
    );

    let legacy_identity = ExactProcessIdentity {
        windows_creation_time_100ns_since_unix_epoch: None,
        ..identity
    };
    assert_eq!(
        observe_owner_process(legacy_identity),
        ProcessClassification::Unknown
    );

    let invalid_pid = ExactProcessIdentity {
        identity: ProcessIdentity {
            pid: 0,
            ..identity.identity
        },
        ..identity
    };
    assert_eq!(
        observe_owner_process(invalid_pid),
        ProcessClassification::Unknown
    );
}

mod task_classification {
    use orchestrator_core::identity::ProcessClassification as P;
    use orchestrator_core::task::{AttemptState, TaskState};
    use orchestrator_runtime::reconcile::{
        classify_interrupted_task, DeliveryEvidence as D, ReconciliationResolution, TaskEvidence,
        TaskRecovery as R, WorkspaceEvidence as W,
    };

    fn evidence(process: P, workspace: W, delivery: D) -> TaskEvidence {
        TaskEvidence {
            task: TaskState::Running,
            attempt: AttemptState::Running,
            process,
            workspace,
            delivery,
        }
    }

    #[test]
    fn exited_owner_with_matching_workspace_and_no_delivery_is_recoverable() {
        let e = evidence(P::Exited, W::Matching, D::NotApplied);
        assert_eq!(classify_interrupted_task(e), R::Recoverable);
        assert_eq!(
            R::Recoverable.resolution(),
            ReconciliationResolution::Resolved
        );
    }

    #[test]
    fn live_owner_blocks_the_task_without_blocking_scheduling() {
        let e = evidence(P::Alive, W::Matching, D::NotApplied);
        assert_eq!(classify_interrupted_task(e), R::Blocked);
        assert_eq!(R::Blocked.resolution(), ReconciliationResolution::Resolved);
    }

    #[test]
    fn unproven_owner_requires_inspection_and_blocks_scheduling() {
        for process in [P::Unknown, P::IdentityMismatch] {
            let e = evidence(process, W::Matching, D::NotApplied);
            assert_eq!(classify_interrupted_task(e), R::RequiresInspection);
        }
        assert_eq!(
            R::RequiresInspection.resolution(),
            ReconciliationResolution::Unresolved
        );
    }

    #[test]
    fn exit_without_recorded_success_or_confirmed_delivery_is_never_completed() {
        let e = evidence(P::Exited, W::Matching, D::NotApplied);
        assert_ne!(classify_interrupted_task(e), R::Completed);
    }

    const ALL_TASKS: [TaskState; 8] = [
        TaskState::Created,
        TaskState::Queued,
        TaskState::Running,
        TaskState::Verifying,
        TaskState::Succeeded,
        TaskState::Failed,
        TaskState::VerificationFailed,
        TaskState::Cancelled,
    ];

    fn with_task(task: TaskState, workspace: W, delivery: D) -> TaskEvidence {
        let mut e = evidence(P::Exited, workspace, delivery);
        e.task = task;
        e
    }

    #[test]
    fn unsafe_workspace_outranks_every_completion_shortcut() {
        for task in ALL_TASKS {
            for delivery in [D::None, D::NotApplied, D::Confirmed] {
                for workspace in [W::Dirty, W::Foreign, W::Unknown] {
                    let e = with_task(task, workspace, delivery);
                    assert_eq!(classify_interrupted_task(e), R::RequiresInspection, "{e:?}");
                }
            }
        }
    }

    #[test]
    fn confirmed_delivery_completes_only_a_succeeded_task_with_a_clean_known_workspace() {
        for task in ALL_TASKS {
            for workspace in [W::Matching, W::Missing, W::BranchMoved] {
                let expected = match (task, workspace) {
                    (TaskState::Succeeded, W::Matching | W::Missing) => R::Completed,
                    _ => R::RequiresInspection,
                };
                let e = with_task(task, workspace, D::Confirmed);
                assert_eq!(classify_interrupted_task(e), expected, "{e:?}");
            }
        }
    }

    #[test]
    fn succeeded_without_delivery_steps_completes_only_with_a_matching_workspace() {
        for (workspace, expected) in [
            (W::Matching, R::Completed),
            (W::Missing, R::RequiresInspection),
            (W::BranchMoved, R::RequiresInspection),
        ] {
            let e = with_task(TaskState::Succeeded, workspace, D::None);
            assert_eq!(classify_interrupted_task(e), expected, "{e:?}");
        }
    }

    #[test]
    fn succeeded_with_a_recorded_unapplied_delivery_is_not_proven_complete() {
        for workspace in [W::Matching, W::Missing, W::BranchMoved] {
            let e = with_task(TaskState::Succeeded, workspace, D::NotApplied);
            assert_eq!(classify_interrupted_task(e), R::RequiresInspection, "{e:?}");
        }
    }

    #[test]
    fn terminal_failures_without_delivery_stay_blocked() {
        for task in [
            TaskState::Failed,
            TaskState::VerificationFailed,
            TaskState::Cancelled,
        ] {
            for delivery in [D::None, D::NotApplied] {
                for workspace in [W::Matching, W::Missing, W::BranchMoved] {
                    let e = with_task(task, workspace, delivery);
                    assert_eq!(classify_interrupted_task(e), R::Blocked, "{e:?}");
                }
            }
        }
    }

    #[test]
    fn active_tasks_without_delivery_keep_recovery_semantics() {
        for task in [
            TaskState::Created,
            TaskState::Queued,
            TaskState::Running,
            TaskState::Verifying,
        ] {
            for delivery in [D::None, D::NotApplied] {
                let matching = with_task(task, W::Matching, delivery);
                assert_eq!(classify_interrupted_task(matching), R::Recoverable);
                let missing = with_task(task, W::Missing, delivery);
                assert_eq!(classify_interrupted_task(missing), R::Blocked);
                let mut moved = with_task(task, W::BranchMoved, delivery);
                assert_eq!(classify_interrupted_task(moved), R::RequiresInspection);
                moved.attempt = AttemptState::CandidateReady;
                assert_eq!(classify_interrupted_task(moved), R::Recoverable);
            }
        }
    }

    #[test]
    fn partial_or_ambiguous_delivery_requires_inspection() {
        for delivery in [D::Partial, D::Ambiguous] {
            let e = evidence(P::Exited, W::Matching, delivery);
            assert_eq!(classify_interrupted_task(e), R::RequiresInspection);
        }
    }

    #[test]
    fn uncommitted_or_unlanded_work_is_preserved_for_inspection() {
        for workspace in [W::Dirty, W::Foreign, W::Unknown] {
            let e = evidence(P::Exited, workspace, D::NotApplied);
            assert_eq!(classify_interrupted_task(e), R::RequiresInspection);
        }
        let moved = evidence(P::Exited, W::BranchMoved, D::NotApplied);
        assert_eq!(classify_interrupted_task(moved), R::RequiresInspection);
    }

    #[test]
    fn candidate_commit_on_moved_branch_is_recoverable_for_verification() {
        let mut e = evidence(P::Exited, W::BranchMoved, D::NotApplied);
        e.task = TaskState::Verifying;
        e.attempt = AttemptState::CandidateReady;
        assert_eq!(classify_interrupted_task(e), R::Recoverable);
    }

    #[test]
    fn missing_workspace_blocks_the_task() {
        let e = evidence(P::Exited, W::Missing, D::NotApplied);
        assert_eq!(classify_interrupted_task(e), R::Blocked);
    }
}

mod git_evidence {
    use orchestrator_git::delivery::{
        UnsafeToDeleteReason as UnsafeReason, WorktreeDeletionSafety as Safety,
    };
    use orchestrator_git::delivery_reconcile::{
        DeliveryClassification as C, DeliveryStepDecision, RecordedDeliveryStep,
    };
    use orchestrator_git::reconcile::{WorkspaceClassification as G, WorkspaceDecision};
    use orchestrator_runtime::reconcile::{
        delivery_evidence, workspace_evidence, DeliveryEvidence as D, WorkspaceEvidence as W,
    };

    fn workspace(classification: G) -> WorkspaceDecision {
        WorkspaceDecision {
            id: "w".into(),
            path: "/tmp/w".into(),
            branch: "b".into(),
            base_commit: "c".into(),
            classification,
        }
    }

    fn step(classification: C) -> DeliveryStepDecision {
        DeliveryStepDecision {
            record: RecordedDeliveryStep {
                id: "s".into(),
                task_id: None,
                action: None,
                candidate_sha: None,
                destination: None,
                target_ref: None,
            },
            classification,
            observed_ref: None,
        }
    }

    #[test]
    fn workspace_classification_maps_one_to_one_when_clean() {
        let clean = Safety::SafeToDelete;
        for (git, expected) in [
            (G::PresentAndMatching, W::Matching),
            (G::BranchMoved, W::BranchMoved),
            (G::Foreign, W::Foreign),
            (G::Missing, W::Missing),
        ] {
            assert_eq!(
                workspace_evidence(Some(&workspace(git)), Some(&clean)),
                expected
            );
        }
    }

    #[test]
    fn missing_workspace_needs_no_safety_inspection() {
        assert_eq!(
            workspace_evidence(Some(&workspace(G::Missing)), None),
            W::Missing
        );
    }

    #[test]
    fn uninspected_or_unrecorded_workspace_fails_closed() {
        assert_eq!(workspace_evidence(None, None), W::Unknown);
        for git in [G::PresentAndMatching, G::BranchMoved] {
            assert_eq!(workspace_evidence(Some(&workspace(git)), None), W::Unknown);
        }
    }

    #[test]
    fn dirty_or_primary_checkout_overrides_a_matching_workspace() {
        let dirty = Safety::Unsafe(UnsafeReason::Dirty(" M f".into()));
        let primary = Safety::Unsafe(UnsafeReason::PrimaryCheckout);
        let matching = workspace(G::PresentAndMatching);
        assert_eq!(workspace_evidence(Some(&matching), Some(&dirty)), W::Dirty);
        assert_eq!(
            workspace_evidence(Some(&matching), Some(&primary)),
            W::Foreign
        );
    }

    #[test]
    fn unlanded_head_on_a_matching_workspace_is_branch_moved() {
        let unlanded = Safety::Unsafe(UnsafeReason::NotReachable {
            head: "h".into(),
            base_ref: "main".into(),
        });
        assert_eq!(
            workspace_evidence(Some(&workspace(G::PresentAndMatching)), Some(&unlanded)),
            W::BranchMoved
        );
    }

    #[test]
    fn delivery_steps_summarise_conservatively() {
        assert_eq!(delivery_evidence(&[]), D::None);
        assert_eq!(delivery_evidence(&[step(C::NotApplied)]), D::NotApplied);
        assert_eq!(
            delivery_evidence(&[step(C::Confirmed), step(C::Confirmed)]),
            D::Confirmed
        );
        assert_eq!(
            delivery_evidence(&[step(C::Confirmed), step(C::NotApplied)]),
            D::Partial
        );
        assert_eq!(
            delivery_evidence(&[step(C::Confirmed), step(C::Ambiguous)]),
            D::Ambiguous
        );
    }
}
