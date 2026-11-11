use orchestrator_core::approval::{
    Actor, ApprovalAction, ApprovalActionKind, ApprovalState, ApproverAllowlist, ParameterDigest,
};
use orchestrator_core::authorization::ActionKind;
use orchestrator_core::contract::{
    AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, VerificationPolicy,
};
use orchestrator_core::records::Project;
use orchestrator_store::{ReviewError, SqliteStore, Store, MAX_FEEDBACK_BYTES};
use serde_json::json;

const COMMIT_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const COMMIT_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn new_store() -> (tempfile::TempDir, SqliteStore) {
    let directory = tempfile::tempdir().unwrap();
    let store = SqliteStore::new(&directory.path().join("store.sqlite")).unwrap();
    for id in ["p1", "p2"] {
        store
            .register_project(&Project {
                id: id.into(),
                root_path: format!("/work/{id}"),
                settings: json!({}),
            })
            .unwrap();
    }
    store
        .create(&TaskContract {
            id: "t1".into(),
            project_id: "p1".into(),
            parent_id: None,
            objective: "Review a candidate".into(),
            kind: TaskKind::Implement,
            agent: "codex".into(),
            dependencies: Vec::new(),
            verification: VerificationPolicy::None,
            delivery: DeliveryPolicy::LocalReview,
            autonomy: AutonomyPolicy::WorkspaceWrites,
            max_retries: 0,
            max_repairs: 0,
            model_selection: None,
        })
        .unwrap();
    (directory, store)
}

fn action(digest: u8, commit: &str) -> ApprovalAction {
    ApprovalAction::new(
        ApprovalActionKind::Delivery(ActionKind::PullRequestCreate),
        ParameterDigest::from_sha256([digest; 32]),
        commit,
        "t1",
    )
    .unwrap()
}

fn allowlist() -> ApproverAllowlist {
    ApproverAllowlist::new("user-1", ["reviewer-1"]).unwrap()
}

fn user() -> Actor {
    Actor::User("user-1".into())
}

fn agent() -> Actor {
    Actor::Agent("agent-1".into())
}

fn request(store: &SqliteStore, id: &str, digest: u8, commit: &str) {
    store
        .request_approval(id, &action(digest, commit), &agent(), &allowlist())
        .unwrap();
}

#[test]
fn coordinator_entries_are_ordered_paged_and_project_scoped() {
    let (_directory, store) = new_store();
    let validation = json!({"status": "valid", "authority": "none"});
    store
        .append_coordinator_entry("e1", "p1", "message", "user", "hello", None)
        .unwrap();
    store
        .append_coordinator_entry("e2", "p2", "message", "user", "other", None)
        .unwrap();
    let proposal = store
        .append_coordinator_entry(
            "e3",
            "p1",
            "proposal",
            "coordinator",
            "{}",
            Some(&validation),
        )
        .unwrap();
    assert_eq!(proposal.validation, Some(validation));

    let all = store.coordinator_entries_after("p1", 0, 10).unwrap();
    assert_eq!(
        all.iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>(),
        ["e1", "e3"]
    );
    assert!(all[0].sequence < all[1].sequence);
    let page = store
        .coordinator_entries_after("p1", all[0].sequence, 10)
        .unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].id, "e3");
    assert_eq!(
        store.coordinator_entries_after("p1", 0, 1).unwrap().len(),
        1
    );
    assert!(store.coordinator_entries_after("p1", -1, 1).is_err());
}

#[test]
fn coordinator_entries_enforce_schema_invariants_and_survive_reopen() {
    let (directory, store) = new_store();
    let validation = json!({"status": "valid"});
    assert!(store
        .append_coordinator_entry("e1", "missing", "message", "user", "x", None)
        .is_err());
    assert!(store
        .append_coordinator_entry("e2", "p1", "proposal", "coordinator", "{}", None)
        .is_err());
    assert!(store
        .append_coordinator_entry("e3", "p1", "message", "user", "x", Some(&validation))
        .is_err());
    assert!(store
        .append_coordinator_entry("e4", "p1", "message", "root", "x", None)
        .is_err());
    assert!(store
        .append_coordinator_entry("e5", "p1", "command", "user", "x", None)
        .is_err());
    store
        .append_coordinator_entry("e6", "p1", "message", "user", "x", None)
        .unwrap();
    assert!(store
        .append_coordinator_entry("e6", "p1", "message", "user", "again", None)
        .is_err());
    drop(store);
    let reopened = SqliteStore::new(&directory.path().join("store.sqlite")).unwrap();
    let entries = reopened.coordinator_entries_after("p1", 0, 10).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].body, "x");
}

#[test]
fn approval_parameters_commit_with_the_approval() {
    let (_directory, store) = new_store();
    let parameters = json!({"action": "local_review", "base_sha": COMMIT_A});
    store
        .request_approval_with_parameters(
            "a1",
            &action(1, COMMIT_A),
            &agent(),
            &allowlist(),
            &parameters,
        )
        .unwrap();
    assert_eq!(store.approval_parameters("a1").unwrap(), Some(parameters));
    request(&store, "a2", 2, COMMIT_A);
    assert_eq!(store.approval_parameters("a2").unwrap(), None);
    assert_eq!(store.approval_parameters("missing").unwrap(), None);

    let records = store.approval_records_for_task("t1").unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(
        records
            .iter()
            .find(|approval| approval.id == "a1")
            .unwrap()
            .state,
        ApprovalState::Superseded { approved_by: None }
    );
}

#[test]
fn request_changes_records_feedback_without_deciding_the_approval() {
    let (_directory, store) = new_store();
    request(&store, "a1", 1, COMMIT_A);
    let change = store
        .request_changes(
            "c1",
            "a1",
            COMMIT_A,
            "Please split this",
            &user(),
            &allowlist(),
        )
        .unwrap();
    assert_eq!(change.approval_id, "a1");
    assert_eq!(change.task_id, "t1");
    assert_eq!(change.candidate_commit, COMMIT_A);
    assert_eq!(change.requested_by, "user-1");
    assert_eq!(
        store.change_request_for_approval("a1").unwrap(),
        Some(change)
    );
    assert_eq!(store.change_request_for_approval("a2").unwrap(), None);
    assert_eq!(
        store.get_approval("a1").unwrap().state,
        ApprovalState::Requested
    );

    let events = store.stream_events_after(0, 100).unwrap();
    let event = events
        .iter()
        .find(|event| event.kind == "review.changes_requested")
        .expect("durable event");
    assert_eq!(event.task_id, "t1");
    assert_eq!(event.payload["approval_id"], "a1");
    assert!(!event.payload.to_string().contains("Please split this"));
}

#[test]
fn request_changes_refuses_invalid_input_and_unauthorized_actors() {
    let (_directory, store) = new_store();
    request(&store, "a1", 1, COMMIT_A);
    let policy = allowlist();
    assert!(matches!(
        store.request_changes("c1", "a1", COMMIT_A, "x", &agent(), &policy),
        Err(ReviewError::Approval(_))
    ));
    assert!(matches!(
        store.request_changes(
            "c2",
            "a1",
            COMMIT_A,
            "x",
            &Actor::User("stranger".into()),
            &policy
        ),
        Err(ReviewError::Approval(_))
    ));
    assert!(matches!(
        store.request_changes("c3", "a1", COMMIT_B, "x", &user(), &policy),
        Err(ReviewError::CandidateMismatch)
    ));
    assert!(matches!(
        store.request_changes("c4", "a1", COMMIT_A, " \n", &user(), &policy),
        Err(ReviewError::InvalidFeedback)
    ));
    let too_long = "x".repeat(MAX_FEEDBACK_BYTES + 1);
    assert!(matches!(
        store.request_changes("c5", "a1", COMMIT_A, &too_long, &user(), &policy),
        Err(ReviewError::InvalidFeedback)
    ));
    assert!(matches!(
        store.request_changes("c6", "missing", COMMIT_A, "x", &user(), &policy),
        Err(ReviewError::Approval(_))
    ));
    assert_eq!(store.change_request_for_approval("a1").unwrap(), None);
    assert!(store
        .stream_events_after(0, 100)
        .unwrap()
        .iter()
        .all(|event| event.kind != "review.changes_requested"));
    let exact = "x".repeat(MAX_FEEDBACK_BYTES);
    store
        .request_changes("c7", "a1", COMMIT_A, &exact, &user(), &policy)
        .unwrap();
}

#[test]
fn request_changes_is_single_use_and_needs_a_pending_approval() {
    let (_directory, store) = new_store();
    let policy = allowlist();
    request(&store, "a1", 1, COMMIT_A);
    store
        .request_changes("c1", "a1", COMMIT_A, "first", &user(), &policy)
        .unwrap();
    assert!(matches!(
        store.request_changes("c2", "a1", COMMIT_A, "second", &user(), &policy),
        Err(ReviewError::AlreadyRequested { .. })
    ));

    request(&store, "a2", 2, COMMIT_A);
    store.approve_approval("a2", &user(), &policy).unwrap();
    assert!(matches!(
        store.request_changes("c3", "a2", COMMIT_A, "late", &user(), &policy),
        Err(ReviewError::Approval(_))
    ));

    request(&store, "a3", 3, COMMIT_A);
    assert!(matches!(
        store.request_changes("c4", "a2", COMMIT_A, "superseded", &user(), &policy),
        Err(ReviewError::Approval(_))
    ));
    assert_eq!(store.change_request_for_approval("a2").unwrap(), None);
}

#[test]
fn approval_is_blocked_by_a_change_request_but_rejection_is_not() {
    let (_directory, store) = new_store();
    let policy = allowlist();
    request(&store, "a1", 1, COMMIT_A);
    store
        .request_changes("c1", "a1", COMMIT_A, "fix it", &user(), &policy)
        .unwrap();
    assert!(matches!(
        store.approve_reviewed_approval("a1", &user(), &policy),
        Err(ReviewError::ChangesRequested { .. })
    ));
    assert_eq!(
        store.get_approval("a1").unwrap().state,
        ApprovalState::Requested
    );
    let rejected = store.reject_approval("a1", &user(), &policy).unwrap();
    assert_eq!(rejected.state, ApprovalState::Rejected);
}

#[test]
fn reviewed_approval_matches_plain_approval_without_changes() {
    let (_directory, store) = new_store();
    let policy = allowlist();
    request(&store, "a1", 1, COMMIT_A);
    assert!(matches!(
        store.approve_reviewed_approval("a1", &agent(), &policy),
        Err(ReviewError::Approval(_))
    ));
    let approved = store
        .approve_reviewed_approval("a1", &user(), &policy)
        .unwrap();
    assert_eq!(approved.state, ApprovalState::Approved { by: user() });
    assert!(matches!(
        store.approve_reviewed_approval("a1", &user(), &policy),
        Err(ReviewError::Approval(_))
    ));
    assert!(matches!(
        store.approve_reviewed_approval("missing", &user(), &policy),
        Err(ReviewError::Approval(_))
    ));
}

#[test]
fn concurrent_approve_and_request_changes_never_both_succeed() {
    for round in 0..8 {
        let (_directory, store) = new_store();
        let policy = allowlist();
        request(&store, "a1", 1, COMMIT_A);
        let (approved, changed) = std::thread::scope(|scope| {
            let approve = scope.spawn(|| store.approve_reviewed_approval("a1", &user(), &policy));
            let change = scope
                .spawn(|| store.request_changes("c1", "a1", COMMIT_A, "fix", &user(), &policy));
            (approve.join().unwrap(), change.join().unwrap())
        });
        assert!(
            approved.is_err() || changed.is_err(),
            "round {round}: both succeeded"
        );
        let state = store.get_approval("a1").unwrap().state;
        let has_change = store.change_request_for_approval("a1").unwrap().is_some();
        assert_eq!(
            matches!(state, ApprovalState::Approved { .. }),
            approved.is_ok()
        );
        assert_eq!(has_change, changed.is_ok());
    }
}

#[test]
fn plain_approve_approval_cannot_bypass_a_change_request() {
    let (_directory, store) = new_store();
    let policy = allowlist();
    request(&store, "a1", 1, COMMIT_A);
    store
        .request_changes("c1", "a1", COMMIT_A, "fix it", &user(), &policy)
        .unwrap();
    assert!(matches!(
        store.approve_approval("a1", &user(), &policy),
        Err(orchestrator_core::approval::ApprovalError::IllegalTransition { .. })
    ));
    assert_eq!(
        store.get_approval("a1").unwrap().state,
        ApprovalState::Requested
    );
}

#[test]
fn reviewed_approval_authorizes_before_looking_at_the_approval() {
    let (_directory, store) = new_store();
    let policy = allowlist();
    let stranger = Actor::User("stranger".into());
    // A missing approval would report NotFound if authorization did not run first.
    assert!(matches!(
        store.approve_reviewed_approval("missing", &stranger, &policy),
        Err(ReviewError::Approval(
            orchestrator_core::approval::ApprovalError::UnauthorizedActor { .. }
        ))
    ));
    // A change request would report ChangesRequested if authorization did not run first.
    request(&store, "a1", 1, COMMIT_A);
    store
        .request_changes("c1", "a1", COMMIT_A, "fix it", &user(), &policy)
        .unwrap();
    assert!(matches!(
        store.approve_reviewed_approval("a1", &stranger, &policy),
        Err(ReviewError::Approval(
            orchestrator_core::approval::ApprovalError::UnauthorizedActor { .. }
        ))
    ));
}
