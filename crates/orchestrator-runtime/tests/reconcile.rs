use orchestrator_runtime::reconcile::{
    classify_identity, reconcile_processes, scheduling_may_resume, ProcessAction,
    ProcessClassification, ProcessIdentity, ProcessStartTime, ProcessSubject,
    ReconciliationResolution, RecordedProcess,
};

#[test]
fn changed_start_time_is_an_identity_mismatch() {
    let expected = ProcessIdentity {
        pid: 42,
        process_group: Some(42),
        start_time: Some(ProcessStartTime {
            seconds: 100,
            microseconds: 10,
        }),
    };
    let observed = ProcessIdentity {
        start_time: Some(ProcessStartTime {
            seconds: 101,
            microseconds: 10,
        }),
        ..expected
    };

    assert_eq!(
        classify_identity(expected, observed),
        ProcessClassification::IdentityMismatch
    );
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
    let identity = ProcessIdentity::capture(std::process::id()).expect("capture current process");
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
#[test]
fn exited_child_is_interrupted_not_succeeded() {
    use std::os::unix::process::CommandExt;
    use std::process::Command;

    let mut child = Command::new("/bin/sleep")
        .arg("0.2")
        .process_group(0)
        .spawn()
        .expect("spawn short-lived process");
    let identity = ProcessIdentity::capture(child.id()).expect("capture child identity");
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
