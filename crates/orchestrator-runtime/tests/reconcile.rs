use orchestrator_core::identity::{
    plan_process_reconciliation, ProcessAction, ProcessClassification, ProcessIdentity,
    ProcessObservation, ProcessPlatform,
};
use orchestrator_runtime::reconcile::{
    capture_process_identity, reconcile_processes, scheduling_may_resume, ProcessSubject,
    ReconciliationResolution, RecordedProcess,
};
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
    let identity = capture_process_identity(child.id()).expect("capture child identity");
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
