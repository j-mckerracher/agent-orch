use orchestrator_core::approval::{
    ActionMismatch, Actor, ApprovalAction, ApprovalActionKind, ApprovalError, ApprovalEvent,
    ApprovalState, ApprovalStateKind, ApproverAllowlist, ParameterDigest,
};
use orchestrator_core::authorization::ActionKind;
use orchestrator_core::contract::{
    AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, VerificationPolicy,
};
use orchestrator_core::records::Project;
use orchestrator_store::{SqliteStore, Store};

const COMMIT_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const COMMIT_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn new_store() -> (tempfile::TempDir, SqliteStore) {
    let directory = tempfile::tempdir().unwrap();
    let store = SqliteStore::new(&directory.path().join("store.sqlite")).unwrap();
    store.register_project(&project()).unwrap();
    store.create(&contract()).unwrap();
    (directory, store)
}

fn project() -> Project {
    Project {
        id: "p1".into(),
        root_path: "/work/project".into(),
        settings: serde_json::json!({"branch": "main"}),
    }
}

fn contract() -> TaskContract {
    TaskContract {
        id: "t1".into(),
        project_id: "p1".into(),
        parent_id: None,
        objective: "Approve a candidate action".into(),
        kind: TaskKind::Implement,
        agent: "codex".into(),
        dependencies: Vec::new(),
        verification: VerificationPolicy::Commands {
            commands: vec![vec!["cargo".into(), "test".into()]],
        },
        delivery: DeliveryPolicy::LocalReview,
        autonomy: AutonomyPolicy::WorkspaceWrites,
        max_retries: 1,
        max_repairs: 1,
    }
}

fn action(digest: u8, commit: &str, kind: ApprovalActionKind) -> ApprovalAction {
    ApprovalAction::new(
        kind,
        ParameterDigest::from_sha256([digest; 32]),
        commit,
        "t1",
    )
    .unwrap()
}

fn delivery_action(digest: u8, commit: &str) -> ApprovalAction {
    action(
        digest,
        commit,
        ApprovalActionKind::Delivery(ActionKind::PullRequestCreate),
    )
}

fn allowlist() -> ApproverAllowlist {
    ApproverAllowlist::new("user-1", ["reviewer-1"]).unwrap()
}

fn agent() -> Actor {
    Actor::Agent("agent-1".into())
}

fn user() -> Actor {
    Actor::User("user-1".into())
}

fn reviewer() -> Actor {
    Actor::Approved("reviewer-1".into())
}

#[test]
fn request_approve_and_check_round_trip() {
    let (_directory, store) = new_store();
    let policy = allowlist();
    let proposed = delivery_action(1, COMMIT_A);

    let requested = store
        .request_approval("approval-1", &proposed, &agent(), &policy)
        .unwrap();
    assert_eq!(requested.state, ApprovalState::Requested);

    let approved = store
        .approve_approval("approval-1", &user(), &policy)
        .unwrap();
    assert_eq!(approved.state, ApprovalState::Approved { by: user() });
    assert_eq!(store.check_approval("approval-1", &proposed), Ok(()));
    assert_eq!(
        store.approval_events("approval-1").unwrap(),
        vec![
            ApprovalEvent::Requested { by: agent() },
            ApprovalEvent::Approved { by: user() },
        ]
    );
}

#[test]
fn changed_parameters_fail_check_and_require_reapproval() {
    let (_directory, store) = new_store();
    let policy = allowlist();
    let original = delivery_action(2, COMMIT_A);
    let changed = delivery_action(3, COMMIT_A);

    store
        .request_approval("approval-old", &original, &agent(), &policy)
        .unwrap();
    store
        .approve_approval("approval-old", &user(), &policy)
        .unwrap();
    assert_eq!(
        store.check_approval("approval-old", &changed),
        Err(ApprovalError::ActionMismatch(ActionMismatch::Parameters))
    );

    store
        .request_approval("approval-new", &changed, &agent(), &policy)
        .unwrap();
    assert_eq!(
        store.get_approval("approval-old").unwrap().state,
        ApprovalState::Superseded {
            approved_by: Some(user())
        }
    );
    assert_eq!(
        store.check_approval("approval-old", &original),
        Err(ApprovalError::NotApproved {
            approval_id: "approval-old".into(),
            state: ApprovalStateKind::Superseded,
        })
    );
    store
        .approve_approval("approval-new", &reviewer(), &policy)
        .unwrap();
    assert_eq!(store.check_approval("approval-new", &changed), Ok(()));
}
#[test]
fn changed_candidate_commit_fails_check_and_supersedes_old_approval() {
    let (_directory, store) = new_store();
    let policy = allowlist();
    let original = delivery_action(4, COMMIT_A);
    let changed = delivery_action(4, COMMIT_B);

    store
        .request_approval("approval-old", &original, &agent(), &policy)
        .unwrap();
    store
        .approve_approval("approval-old", &user(), &policy)
        .unwrap();
    assert_eq!(
        store.check_approval("approval-old", &changed),
        Err(ApprovalError::ActionMismatch(
            ActionMismatch::CandidateCommit
        ))
    );

    store
        .request_approval("approval-new", &changed, &agent(), &policy)
        .unwrap();
    assert_eq!(
        store.get_approval("approval-old").unwrap().state.kind(),
        ApprovalStateKind::Superseded
    );
}

#[test]
fn agent_cannot_approve_its_own_request() {
    let (_directory, store) = new_store();
    let policy = allowlist();
    let proposed = delivery_action(5, COMMIT_A);

    store
        .request_approval("approval-1", &proposed, &agent(), &policy)
        .unwrap();
    assert_eq!(
        store.approve_approval("approval-1", &agent(), &policy),
        Err(ApprovalError::UnauthorizedActor {
            actor_id: "agent-1".into(),
        })
    );
    assert_eq!(
        store.get_approval("approval-1").unwrap().state,
        ApprovalState::Requested
    );
}

#[test]
fn rejected_approval_cannot_transition_to_approved() {
    let (_directory, store) = new_store();
    let policy = allowlist();
    let proposed = delivery_action(6, COMMIT_A);

    store
        .request_approval("approval-1", &proposed, &agent(), &policy)
        .unwrap();
    store
        .reject_approval("approval-1", &user(), &policy)
        .unwrap();
    assert_eq!(
        store.approve_approval("approval-1", &reviewer(), &policy),
        Err(ApprovalError::IllegalTransition {
            from: Some(ApprovalStateKind::Rejected),
            event: orchestrator_core::approval::ApprovalEventKind::Approved,
        })
    );
}

#[test]
fn superseded_approval_cannot_be_reused() {
    let (_directory, store) = new_store();
    let policy = allowlist();
    let original = delivery_action(7, COMMIT_A);
    let replacement = delivery_action(8, COMMIT_A);

    store
        .request_approval("approval-old", &original, &agent(), &policy)
        .unwrap();
    store
        .approve_approval("approval-old", &user(), &policy)
        .unwrap();
    store
        .request_approval("approval-new", &replacement, &agent(), &policy)
        .unwrap();

    assert_eq!(
        store.check_approval("approval-old", &original),
        Err(ApprovalError::NotApproved {
            approval_id: "approval-old".into(),
            state: ApprovalStateKind::Superseded,
        })
    );
}

#[test]
fn policy_change_requires_approved_policy_approval() {
    let (_directory, store) = new_store();
    let policy = allowlist();
    let change = action(9, COMMIT_A, ApprovalActionKind::PolicyChange);

    assert_eq!(
        store.request_approval("agent-policy", &change, &agent(), &policy),
        Err(ApprovalError::UnauthorizedPolicyChange {
            actor_id: "agent-1".into(),
        })
    );
    assert_eq!(
        store.check_approval("missing-policy", &change),
        Err(ApprovalError::NotFound {
            approval_id: "missing-policy".into(),
        })
    );

    store
        .request_approval("policy-approval", &change, &user(), &policy)
        .unwrap();
    assert_eq!(
        store.check_approval("policy-approval", &change),
        Err(ApprovalError::NotApproved {
            approval_id: "policy-approval".into(),
            state: ApprovalStateKind::Requested,
        })
    );
    store
        .approve_approval("policy-approval", &reviewer(), &policy)
        .unwrap();
    assert_eq!(store.check_approval("policy-approval", &change), Ok(()));
}

#[test]
fn approval_state_and_event_roll_back_together() {
    let (directory, store) = new_store();
    let policy = allowlist();
    let proposed = delivery_action(10, COMMIT_A);
    store
        .request_approval("approval-1", &proposed, &agent(), &policy)
        .unwrap();
    let original_events = store.approval_events("approval-1").unwrap();

    create_failure_trigger(&directory.path().join("store.sqlite"));
    assert!(matches!(
        store.approve_approval("approval-1", &user(), &policy),
        Err(ApprovalError::Persistence(_))
    ));
    assert_eq!(
        store.get_approval("approval-1").unwrap().state,
        ApprovalState::Requested
    );
    assert_eq!(
        store.approval_events("approval-1").unwrap(),
        original_events
    );
}

fn create_failure_trigger(path: &std::path::Path) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
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
            .unwrap();
        sqlx::query(
            "CREATE TRIGGER reject_approval_events BEFORE INSERT ON approval_events \
             BEGIN SELECT RAISE(ABORT, 'forced approval event failure'); END",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool.close().await;
    });
}

#[test]
fn approvals_and_events_survive_reopen() {
    let (directory, store) = new_store();
    let path = directory.path().join("store.sqlite");
    let policy = allowlist();
    let proposed = delivery_action(11, COMMIT_A);
    store
        .request_approval("approval-1", &proposed, &agent(), &policy)
        .unwrap();
    store
        .approve_approval("approval-1", &reviewer(), &policy)
        .unwrap();
    let expected = store.get_approval("approval-1").unwrap();
    let expected_events = store.approval_events("approval-1").unwrap();
    drop(store);

    let reopened = SqliteStore::new(&path).unwrap();
    assert_eq!(reopened.get_approval("approval-1").unwrap(), expected);
    assert_eq!(
        reopened.approval_events("approval-1").unwrap(),
        expected_events
    );
    assert_eq!(reopened.check_approval("approval-1", &proposed), Ok(()));
}
