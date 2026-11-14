use orchestrator_core::approval::{
    ActionMismatch, Actor, ApprovalAction, ApprovalActionKind, ApprovalError, ApproverAllowlist,
    ParameterDigest,
};
use orchestrator_core::authorization::ActionKind;
use orchestrator_core::contract::{
    AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, VerificationPolicy,
};
use orchestrator_core::records::{Project, VerificationRun};
use orchestrator_store::{DeliveryOutcome, DeliveryStepRecord, SqliteStore, Store};
use serde_json::json;
use std::path::Path;
use tempfile::TempDir;

const CANDIDATE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn new_store() -> (TempDir, SqliteStore) {
    let directory = tempfile::tempdir().expect("temporary SQLite directory");
    let store =
        SqliteStore::new(&directory.path().join("store.sqlite")).expect("open SQLite store");
    store
        .register_project(&Project {
            id: "project-1".into(),
            root_path: "/repo".into(),
            settings: json!({}),
        })
        .expect("register project");
    store
        .create(&TaskContract {
            id: "task-1".into(),
            project_id: "project-1".into(),
            parent_id: None,
            objective: "deliver verified candidate".into(),
            kind: TaskKind::Implement,
            agent: "test-agent".into(),
            dependencies: Vec::new(),
            verification: VerificationPolicy::None,
            delivery: DeliveryPolicy::LocalReview,
            autonomy: AutonomyPolicy::WorkspaceWrites,
            max_retries: 0,
            max_repairs: 0,
            model_selection: None,
        })
        .expect("create task");
    (directory, store)
}

fn action(kind: ActionKind, digest: u8, candidate: &str) -> ApprovalAction {
    ApprovalAction::new(
        ApprovalActionKind::Delivery(kind),
        ParameterDigest::from_sha256([digest; 32]),
        candidate,
        "task-1",
    )
    .expect("valid delivery action")
}
fn check_approval(
    store: &SqliteStore,
    id: &str,
    proposed: &ApprovalAction,
) -> Result<(), ApprovalError> {
    let ApprovalActionKind::Delivery(action_kind) = proposed.kind() else {
        panic!("delivery test must use a delivery action");
    };
    store.check_delivery_approval(
        id,
        proposed.task_id(),
        action_kind,
        *proposed.parameter_digest(),
        proposed.candidate_commit().as_str(),
    )
}

fn record_success(store: &SqliteStore, id: &str, action: &ApprovalAction) {
    let record = DeliveryStepRecord {
        id,
        approval_id: "approval-1",
        verification_run_id: "verification-1",
        action,
        destination: "/repo",
        target_ref: Some("refs/heads/orchestrator/review/task-1"),
        outcome: DeliveryOutcome::ReviewRefCreated,
        diff_summary: Some("1 file changed, 1 insertion(+)"),
    };
    store
        .record_delivery_step(&record)
        .expect("record delivery step and audit event");
}

#[test]
fn delivery_step_and_audit_event_survive_reopen() {
    let (directory, store) = new_store();
    let action = action(ActionKind::LocalReviewMerge, 7, CANDIDATE);
    record_success(&store, "delivery-1", &action);
    drop(store);

    let reopened =
        SqliteStore::new(&directory.path().join("store.sqlite")).expect("reopen SQLite store");
    let steps = reopened
        .delivery_steps_for_task("task-1")
        .expect("read delivery steps");
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0]["id"], "delivery-1");
    assert_eq!(steps[0]["approval_id"], "approval-1");
    assert_eq!(steps[0]["verification_run_id"], "verification-1");
    assert_eq!(steps[0]["candidate_sha"], CANDIDATE);
    assert_eq!(steps[0]["outcome"], "review_ref_created");
    assert_eq!(steps[0]["diff_summary"], "1 file changed, 1 insertion(+)");

    let events = reopened
        .delivery_events("delivery-1")
        .expect("read delivery audit events");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["kind"], "review_ref_created");
    assert_eq!(events[0]["payload"]["outcome"], "review_ref_created");
}

#[test]
fn report_only_step_persists_report_and_audit_event() {
    let (_directory, store) = new_store();
    let action = action(ActionKind::ReportPublish, 10, CANDIDATE);
    let record = DeliveryStepRecord {
        id: "delivery-report",
        approval_id: "approval-report",
        verification_run_id: "verification-report",
        action: &action,
        destination: "/repo",
        target_ref: None,
        outcome: DeliveryOutcome::Reported,
        diff_summary: Some("1 file changed, 1 insertion(+)"),
    };
    store
        .record_delivery_step(&record)
        .expect("persist report-only step");

    let steps = store
        .delivery_steps_for_task("task-1")
        .expect("read report-only step");
    assert_eq!(steps[0]["delivery_mode"], "report_only");
    assert_eq!(steps[0]["action_kind"], "report_publish");
    assert_eq!(steps[0]["outcome"], "reported");
    assert_eq!(steps[0]["candidate_sha"], CANDIDATE);
    assert_eq!(steps[0]["diff_summary"], "1 file changed, 1 insertion(+)");
    let events = store
        .delivery_events("delivery-report")
        .expect("read report audit event");
    assert_eq!(events[0]["kind"], "reported");
}
#[test]
fn refused_delivery_step_and_audit_event_are_retained() {
    let (_directory, store) = new_store();
    let action = action(ActionKind::ReportPublish, 8, CANDIDATE);
    let record = DeliveryStepRecord {
        id: "delivery-refused",
        approval_id: "missing-approval",
        verification_run_id: "verification-1",
        action: &action,
        destination: "/repo",
        target_ref: None,
        outcome: DeliveryOutcome::Refused {
            detail: "approval was not found".into(),
        },
        diff_summary: None,
    };
    store
        .record_delivery_step(&record)
        .expect("persist refused outcome");

    let steps = store
        .delivery_steps_for_task("task-1")
        .expect("read refused step");
    assert_eq!(steps[0]["outcome"], "refused");
    assert_eq!(steps[0]["detail"], "approval was not found");
    let events = store
        .delivery_events("delivery-refused")
        .expect("read refused event");
    assert_eq!(events[0]["kind"], "refused");
    assert_eq!(events[0]["payload"]["detail"], "approval was not found");
}

#[test]
fn delivery_step_and_audit_event_roll_back_together() {
    let (directory, store) = new_store();
    let action = action(ActionKind::LocalReviewMerge, 9, CANDIDATE);
    create_failure_trigger(&directory.path().join("store.sqlite"));

    let record = DeliveryStepRecord {
        id: "delivery-fail",
        approval_id: "approval-1",
        verification_run_id: "verification-1",
        action: &action,
        destination: "/repo",
        target_ref: Some("refs/heads/orchestrator/review/task-1"),
        outcome: DeliveryOutcome::ReviewRefCreated,
        diff_summary: Some("1 file changed"),
    };
    assert!(store.record_delivery_step(&record).is_err());
    assert!(store
        .delivery_steps_for_task("task-1")
        .expect("read after rollback")
        .is_empty());
    assert!(store.delivery_events("delivery-fail").is_err());
}

fn create_failure_trigger(path: &Path) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build SQLite runtime");
    runtime.block_on(async {
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(false)
            .foreign_keys(true)
            .busy_timeout(std::time::Duration::from_secs(5));
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .expect("connect to test database");
        sqlx::query(
            "CREATE TRIGGER reject_delivery_events BEFORE INSERT ON delivery_step_events \
             BEGIN SELECT RAISE(ABORT, 'forced delivery event failure'); END",
        )
        .execute(&pool)
        .await
        .expect("create failure trigger");
        pool.close().await;
    });
}

fn verification(id: &str, commit: &str, status: &str) -> VerificationRun {
    VerificationRun {
        id: id.into(),
        task_id: "task-1".into(),
        commit: commit.into(),
        status: status.into(),
        log_path: None,
    }
}

#[test]
fn verification_revalidation_requires_exact_candidate_and_passed_run() {
    let (_directory, store) = new_store();
    let run = verification("verification-1", CANDIDATE, "passed");
    store
        .record_verification_result(&run, 100, 200, &json!({"status": "passed"}))
        .expect("persist verifier run");

    assert!(store
        .delivery_verification_matches("task-1", "verification-1", CANDIDATE)
        .expect("check matching run"));
    assert!(!store
        .delivery_verification_matches(
            "task-1",
            "verification-1",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        )
        .expect("reject changed candidate"));
    assert!(!store
        .delivery_verification_matches("task-1", "missing-run", CANDIDATE)
        .expect("reject missing run"));
}

#[test]
fn verification_revalidation_rejects_nonpassed_run() {
    let (_directory, store) = new_store();
    let run = verification("verification-failed", CANDIDATE, "failed");
    store
        .record_verification_result(&run, 100, 200, &json!({"status": "failed"}))
        .expect("persist failed verifier run");

    assert!(!store
        .delivery_verification_matches("task-1", "verification-failed", CANDIDATE)
        .expect("reject failed run"));
}
fn approve_action(store: &SqliteStore, id: &str, action: &ApprovalAction) {
    let allowlist =
        ApproverAllowlist::new("user-1", std::iter::empty::<String>()).expect("approval allowlist");
    store
        .request_approval(id, action, &Actor::Agent("agent-1".into()), &allowlist)
        .expect("request delivery approval");
    store
        .approve_approval(id, &Actor::User("user-1".into()), &allowlist)
        .expect("approve delivery");
}

#[test]
fn delivery_refuses_missing_approval() {
    let (_directory, store) = new_store();
    let proposed = action(ActionKind::LocalReviewMerge, 1, CANDIDATE);

    assert!(matches!(
        check_approval(&store, "missing-approval", &proposed),
        Err(ApprovalError::NotFound { .. })
    ));
}

#[test]
fn delivery_refuses_changed_parameter_digest() {
    let (_directory, store) = new_store();
    let approved = action(ActionKind::LocalReviewMerge, 1, CANDIDATE);
    let changed = action(ActionKind::LocalReviewMerge, 2, CANDIDATE);
    approve_action(&store, "approval-parameters", &approved);

    assert_eq!(
        check_approval(&store, "approval-parameters", &changed),
        Err(ApprovalError::ActionMismatch(ActionMismatch::Parameters))
    );
}

#[test]
fn delivery_refuses_changed_candidate_sha() {
    let (_directory, store) = new_store();
    let approved = action(ActionKind::LocalReviewMerge, 3, CANDIDATE);
    let changed = action(
        ActionKind::LocalReviewMerge,
        3,
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    );
    approve_action(&store, "approval-candidate", &approved);

    assert_eq!(
        check_approval(&store, "approval-candidate", &changed),
        Err(ApprovalError::ActionMismatch(
            ActionMismatch::CandidateCommit
        ))
    );
}

#[test]
fn delivery_refuses_changed_action_kind() {
    let (_directory, store) = new_store();
    let approved = action(ActionKind::LocalReviewMerge, 4, CANDIDATE);
    let changed = action(ActionKind::ReportPublish, 4, CANDIDATE);
    approve_action(&store, "approval-kind", &approved);

    assert_eq!(
        check_approval(&store, "approval-kind", &changed),
        Err(ApprovalError::ActionMismatch(ActionMismatch::Kind))
    );
}
#[test]
fn all_delivery_steps_survive_reopen() {
    let (directory, store) = new_store();
    record_success(
        &store,
        "delivery-1",
        &action(ActionKind::LocalReviewMerge, 7, CANDIDATE),
    );
    store
        .create(&TaskContract {
            id: "task-2".into(),
            project_id: "project-1".into(),
            parent_id: None,
            objective: "deliver second candidate".into(),
            kind: TaskKind::Implement,
            agent: "test-agent".into(),
            dependencies: Vec::new(),
            verification: VerificationPolicy::None,
            delivery: DeliveryPolicy::LocalReview,
            autonomy: AutonomyPolicy::WorkspaceWrites,
            max_retries: 0,
            max_repairs: 0,
            model_selection: None,
        })
        .expect("create second task");
    let second_action = ApprovalAction::new(
        ApprovalActionKind::Delivery(ActionKind::LocalReviewMerge),
        ParameterDigest::from_sha256([11; 32]),
        CANDIDATE,
        "task-2",
    )
    .expect("valid second delivery action");
    store
        .record_delivery_step(&DeliveryStepRecord {
            id: "delivery-2",
            approval_id: "approval-2",
            verification_run_id: "verification-2",
            action: &second_action,
            destination: "/repo",
            target_ref: Some("refs/heads/orchestrator/review/task-2"),
            outcome: DeliveryOutcome::ReviewRefCreated,
            diff_summary: None,
        })
        .expect("persist second delivery step");
    drop(store);

    let reopened =
        SqliteStore::new(&directory.path().join("store.sqlite")).expect("reopen SQLite store");
    let steps = reopened
        .all_delivery_steps()
        .expect("read delivery steps for every task");

    assert_eq!(steps.len(), 2);
    assert_eq!(steps[0]["id"], "delivery-1");
    assert_eq!(steps[0]["task_id"], "task-1");
    assert_eq!(steps[1]["id"], "delivery-2");
    assert_eq!(steps[1]["task_id"], "task-2");
}
