//! M3-04 acceptance contract for the delivery store seam, covering what
//! `delivery.rs` does not pin: only an approved, current, exactly matching
//! approval authorizes delivery, and a delivery record is append-only and
//! atomic. Uses real temporary SQLite databases through the public API and no
//! platform-specific code.

use orchestrator_core::approval::{
    ActionMismatch, Actor, ApprovalAction, ApprovalActionKind, ApprovalError, ApprovalStateKind,
    ApproverAllowlist, ParameterDigest,
};
use orchestrator_core::authorization::ActionKind;
use orchestrator_core::contract::{
    AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, VerificationPolicy,
};
use orchestrator_core::records::Project;
use orchestrator_store::{DeliveryOutcome, DeliveryStepRecord, SqliteStore, Store};
use serde_json::json;
use tempfile::TempDir;

const CANDIDATE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const REVIEW_REF: &str = "refs/heads/orchestrator/review/task-1";

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

fn action(kind: ActionKind, digest: u8) -> ApprovalAction {
    ApprovalAction::new(
        ApprovalActionKind::Delivery(kind),
        ParameterDigest::from_sha256([digest; 32]),
        CANDIDATE,
        "task-1",
    )
    .expect("valid delivery action")
}

fn allowlist() -> ApproverAllowlist {
    ApproverAllowlist::new("user-1", std::iter::empty::<String>()).expect("approval allowlist")
}

fn request(store: &SqliteStore, id: &str, action: &ApprovalAction) {
    store
        .request_approval(id, action, &Actor::Agent("agent-1".into()), &allowlist())
        .expect("request delivery approval");
}

fn approve(store: &SqliteStore, id: &str) {
    store
        .approve_approval(id, &Actor::User("user-1".into()), &allowlist())
        .expect("approve delivery");
}

fn check(
    store: &SqliteStore,
    id: &str,
    task_id: &str,
    proposed: &ApprovalAction,
) -> Result<(), ApprovalError> {
    let ApprovalActionKind::Delivery(kind) = proposed.kind() else {
        panic!("delivery check requires a delivery action");
    };
    store.check_delivery_approval(
        id,
        task_id,
        kind,
        *proposed.parameter_digest(),
        proposed.candidate_commit().as_str(),
    )
}

#[test]
fn only_an_approved_current_exact_approval_authorizes_delivery() {
    let proposed = action(ActionKind::LocalReviewMerge, 1);

    let (_directory, store) = new_store();
    request(&store, "approval-pending", &proposed);
    assert_eq!(
        check(&store, "approval-pending", "task-1", &proposed),
        Err(ApprovalError::NotApproved {
            approval_id: "approval-pending".into(),
            state: ApprovalStateKind::Requested,
        })
    );

    let (_directory, store) = new_store();
    request(&store, "approval-rejected", &proposed);
    store
        .reject_approval(
            "approval-rejected",
            &Actor::User("user-1".into()),
            &allowlist(),
        )
        .expect("reject delivery");
    assert_eq!(
        check(&store, "approval-rejected", "task-1", &proposed),
        Err(ApprovalError::NotApproved {
            approval_id: "approval-rejected".into(),
            state: ApprovalStateKind::Rejected,
        })
    );

    let (_directory, store) = new_store();
    request(&store, "approval-old", &proposed);
    approve(&store, "approval-old");
    assert_eq!(check(&store, "approval-old", "task-1", &proposed), Ok(()));
    assert_eq!(
        check(&store, "approval-old", "task-2", &proposed),
        Err(ApprovalError::ActionMismatch(ActionMismatch::Task))
    );
    request(
        &store,
        "approval-new",
        &action(ActionKind::LocalReviewMerge, 2),
    );
    assert_eq!(
        check(&store, "approval-old", "task-1", &proposed),
        Err(ApprovalError::NotApproved {
            approval_id: "approval-old".into(),
            state: ApprovalStateKind::Superseded,
        }),
        "a changed action must need a new approval"
    );
}

#[test]
fn outcome_that_does_not_match_a_delivery_action_persists_nothing() {
    let (_directory, store) = new_store();
    let policy_change = ApprovalAction::new(
        ApprovalActionKind::PolicyChange,
        ParameterDigest::from_sha256([4; 32]),
        CANDIDATE,
        "task-1",
    )
    .expect("valid policy action");
    let report = action(ActionKind::ReportPublish, 5);
    let review = action(ActionKind::LocalReviewMerge, 6);
    let pull_request = action(ActionKind::PullRequestCreate, 7);

    for (index, (action, outcome)) in [
        (&review, DeliveryOutcome::Reported),
        (&report, DeliveryOutcome::ReviewRefCreated),
        (&report, DeliveryOutcome::ReviewRefAlreadyMatches),
        (&pull_request, DeliveryOutcome::ReviewRefCreated),
        (
            &pull_request,
            DeliveryOutcome::Refused {
                detail: "pull-request delivery is not supported".into(),
            },
        ),
        (
            &policy_change,
            DeliveryOutcome::Refused {
                detail: "policy change is not a delivery".into(),
            },
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let id = format!("delivery-invalid-{index}");
        let result = store.record_delivery_step(&DeliveryStepRecord {
            id: &id,
            approval_id: "approval-1",
            verification_run_id: "verification-1",
            action,
            destination: "/repo",
            target_ref: None,
            outcome,
            diff_summary: None,
        });
        assert!(result.is_err(), "{id} must be rejected");
        assert!(store.delivery_events(&id).is_err(), "{id} wrote an event");
    }
    assert!(store.delivery_steps_for_task("task-1").unwrap().is_empty());
    assert!(store.all_delivery_steps().unwrap().is_empty());
}

#[test]
fn delivery_step_is_append_only_and_a_reused_id_changes_nothing() {
    let (directory, store) = new_store();
    let review = action(ActionKind::LocalReviewMerge, 8);
    let created = DeliveryStepRecord {
        id: "delivery-1",
        approval_id: "approval-1",
        verification_run_id: "verification-1",
        action: &review,
        destination: "/repo",
        target_ref: Some(REVIEW_REF),
        outcome: DeliveryOutcome::ReviewRefCreated,
        diff_summary: Some("1 file changed"),
    };
    store.record_delivery_step(&created).expect("first record");

    let reused = DeliveryStepRecord {
        outcome: DeliveryOutcome::Refused {
            detail: "rewritten".into(),
        },
        diff_summary: None,
        ..created
    };
    assert!(store.record_delivery_step(&reused).is_err());

    let second = DeliveryStepRecord {
        id: "delivery-2",
        outcome: DeliveryOutcome::Refused {
            detail: "stale verification".into(),
        },
        diff_summary: None,
        ..reused
    };
    store.record_delivery_step(&second).expect("distinct id");

    drop(store);
    let store =
        SqliteStore::new(&directory.path().join("store.sqlite")).expect("reopen SQLite store");
    let steps = store.delivery_steps_for_task("task-1").unwrap();
    assert_eq!(steps.len(), 2);
    assert_eq!(steps[0]["id"], "delivery-1");
    assert_eq!(steps[0]["outcome"], "review_ref_created");
    assert_eq!(steps[0]["detail"], serde_json::Value::Null);
    assert_eq!(steps[0]["diff_summary"], "1 file changed");
    assert_eq!(steps[0]["target_ref"], REVIEW_REF);
    assert_eq!(steps[1]["id"], "delivery-2");
    assert_eq!(steps[1]["outcome"], "refused");
    assert_eq!(steps[1]["detail"], "stale verification");
    let events = store.delivery_events("delivery-1").unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["kind"], "review_ref_created");
    assert_eq!(store.delivery_events("delivery-2").unwrap().len(), 1);
}
