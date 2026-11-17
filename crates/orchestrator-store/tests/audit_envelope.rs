//! Recorded durable_events stream mapped into mcao.audit-envelope.v1 activity projections.
//!
//! Every journal row comes from real SqliteStore APIs. Writer-owned fields (event ID, seq,
//! prev_hash, millisecond ts, actor) and trusted context (home, identity, timezone, role) are
//! synthetic test inputs: this repository has no Firmstate writer or identity binding.

use std::fs;
use std::path::Path;

use orchestrator_core::approval::{
    Actor, ApprovalAction, ApprovalActionKind, ApproverAllowlist, ParameterDigest,
};
use orchestrator_core::authorization::ActionKind;
use orchestrator_core::contract::{
    AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, VerificationPolicy,
};
use orchestrator_core::records::Project;
use orchestrator_core::task::{Attempt, AttemptState, Session, SessionState, TaskEvent};
use orchestrator_store::audit_envelope::{
    project_session_activity, ActorType, AgentRole, AuditActivity, IdentitySource, ProjectionError,
    ResolvedIdentity, TrustedContext, WriterFields,
};
use orchestrator_store::{
    CancellationOwner, DeliveryLineage, DeliveryOutcome, DeliveryStepRecord, SqliteStore, Store,
    TaskDurableEvent,
};
use serde_json::{json, Value};
use tempfile::TempDir;

const TASK: &str = "task-1";
const SESSION: &str = "session-1";
const CANDIDATE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const HOME_ID: &str = "6f1c2a4e-9b3d-4c5e-8f60-718293a4b5c6";
const USER_ID: &str =
    "hmac-sha256:0f1e2d3c4b5a69788796a5b4c3d2e1f00f1e2d3c4b5a69788796a5b4c3d2e1f0";
const SESSION_CREATED_EVENT_ID: &str = "0190a6f0-7c1d-7e2a-9b3c-0a1b2c3d4e5f";
const CANCELLATION_EVENT_ID: &str = "0190a6f0-7c1d-7e2a-9b3c-0a1b2c3d4e61";
const PREV_HASH_0123: &str =
    "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const PREV_HASH_FEDCBA: &str =
    "sha256:fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";

fn task_contract(id: &str) -> TaskContract {
    TaskContract {
        id: id.into(),
        project_id: "project-1".into(),
        parent_id: None,
        objective: "record audit lineage".into(),
        kind: TaskKind::Implement,
        agent: "codex".into(),
        dependencies: Vec::new(),
        verification: VerificationPolicy::None,
        delivery: DeliveryPolicy::LocalReview,
        autonomy: AutonomyPolicy::WorkspaceWrites,
        max_retries: 0,
        max_repairs: 0,
        model_selection: None,
        permission: Default::default(),
        work_item: None,
    }
}

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
    (directory, store)
}

fn delivery_action() -> ApprovalAction {
    ApprovalAction::new(
        ApprovalActionKind::Delivery(ActionKind::LocalReviewMerge),
        ParameterDigest::from_sha256([7; 32]),
        CANDIDATE,
        TASK,
    )
    .expect("valid delivery action")
}

fn allowlist() -> ApproverAllowlist {
    ApproverAllowlist::new("user-1", ["reviewer-1"]).expect("approver allowlist")
}

/// Drives dispatch, session, approval, and delivery through the store API and returns
/// task-1's journal. A second task is written afterwards to prove the query isolates tasks.
fn record_task_stream(store: &SqliteStore) -> Vec<TaskDurableEvent> {
    store.create(&task_contract(TASK)).expect("create task");
    store
        .transition(TASK, TaskEvent::Admit)
        .expect("admit task");
    store
        .add_attempt(
            TASK,
            &Attempt {
                id: "attempt-1".into(),
                task_id: TASK.into(),
                attempt_number: 1,
                state: AttemptState::Pending,
                failure_reason: None,
            },
        )
        .expect("add attempt");
    store
        .transition(TASK, TaskEvent::ExecutionStarted)
        .expect("start execution");
    store
        .update_attempt_state(TASK, "attempt-1", AttemptState::Running)
        .expect("run attempt");
    store
        .add_session(
            TASK,
            &Session {
                id: SESSION.into(),
                attempt_id: "attempt-1".into(),
                adapter: "codex".into(),
                state: SessionState::Spawning,
            },
        )
        .expect("add session");
    store
        .update_session_state(TASK, SESSION, SessionState::Running)
        .expect("run session");
    let action = delivery_action();
    store
        .request_approval(
            "approval-1",
            &action,
            &Actor::Agent("agent-1".into()),
            &allowlist(),
        )
        .expect("request approval");
    store
        .approve_approval("approval-1", &Actor::User("user-1".into()), &allowlist())
        .expect("approve");
    store
        .record_delivery_step(&DeliveryStepRecord {
            id: "delivery-1",
            approval_id: "approval-1",
            verification_run_id: "verification-1",
            action: &action,
            destination: "/repo",
            target_ref: Some("refs/heads/orchestrator/review/task-1"),
            outcome: DeliveryOutcome::ReviewRefCreated,
            diff_summary: Some("1 file changed, 1 insertion(+)"),
        })
        .expect("record delivery step");
    store
        .create(&task_contract("task-2"))
        .expect("create second task");
    store
        .transition("task-2", TaskEvent::Admit)
        .expect("admit second task");
    store
        .task_events_after(TASK, 0, 100)
        .expect("query task lineage")
}

fn trusted_context() -> TrustedContext {
    TrustedContext {
        home_id: HOME_ID.into(),
        identity: Some(ResolvedIdentity {
            user_id: USER_ID.into(),
            source: IdentitySource::GithubLogin,
        }),
        timezone: "America/Chicago".into(),
        role: AgentRole::Firstmate,
    }
}

fn writer_fields(event_id: &str, ts: &str, seq: u64, prev_hash: Option<&str>) -> WriterFields {
    WriterFields {
        event_id: event_id.into(),
        seq,
        prev_hash: prev_hash.map(str::to_owned),
        ts: ts.into(),
        actor_type: ActorType::System,
        actor_id: "orchestrator".into(),
    }
}

fn siem_fixture(name: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/audit-envelope/fixtures/siem")
        .join(name);
    let text = fs::read_to_string(&path).expect("read siem fixture");
    serde_json::from_str(&text).expect("parse siem fixture")
}

fn to_json(activity: AuditActivity) -> Value {
    serde_json::to_value(activity).expect("serialize activity")
}

#[test]
fn recorded_stream_correlates_task_lineage_in_one_query() {
    let (_directory, store) = new_store();
    let rows = record_task_stream(&store);
    let kinds: Vec<&str> = rows.iter().map(|row| row.kind.as_str()).collect();
    assert_eq!(
        kinds,
        [
            "task.created",
            "task.transition",
            "attempt.created",
            "task.transition",
            "attempt.state_changed",
            "session.created",
            "session.state_changed",
            "approval.requested",
            "approval.approved",
            "delivery.review_ref_created",
        ]
    );
    assert!(rows.iter().all(|row| row.task_id == TASK));
    assert_eq!(rows[2].payload["attempt_id"], "attempt-1");
    assert_eq!(rows[5].payload["session_id"], SESSION);
    assert_eq!(rows[8].payload["approval_id"], "approval-1");
    assert_eq!(
        rows[9].delivery,
        Some(DeliveryLineage {
            approval_id: "approval-1".into(),
            verification_run_id: "verification-1".into(),
            candidate_sha: CANDIDATE.into(),
        })
    );
    let after_attempt = store
        .task_events_after(TASK, rows[2].sequence, 100)
        .expect("page after attempt");
    assert_eq!(after_attempt, rows[3..].to_vec());
}

#[test]
fn only_session_rows_project_and_the_rest_refuse_with_their_kind() {
    let (_directory, store) = new_store();
    let rows = record_task_stream(&store);
    let contract = task_contract(TASK);
    let context = trusted_context();
    let fields = writer_fields(
        SESSION_CREATED_EVENT_ID,
        "2026-01-03T04:05:06.789Z",
        0,
        None,
    );
    for row in &rows {
        let result = project_session_activity(row, &contract, &context, &fields);
        if row.session_id.is_some() {
            assert!(result.is_ok(), "{} must project", row.kind);
        } else {
            assert_eq!(
                result,
                Err(ProjectionError::SessionRequired {
                    kind: row.kind.clone()
                })
            );
        }
    }
}

#[test]
fn recorded_session_rows_match_released_golden_fixtures() {
    let (_directory, store) = new_store();
    let rows = record_task_stream(&store);
    let contract = task_contract(TASK);
    let cases = [
        (
            5,
            "0190a6f0-7c1d-7e2a-9b3c-0a1b2c3d4e5f",
            "2026-01-03T04:05:06.789Z",
            0,
            None,
            "session-created.json",
        ),
        (
            6,
            "0190a6f0-7c1d-7e2a-9b3c-0a1b2c3d4e60",
            "2026-01-03T04:05:07.000Z",
            1,
            Some(PREV_HASH_0123),
            "session-state-changed.json",
        ),
        (
            6,
            "0190a6f0-7c1d-7e2a-9b3c-0a1b2c3d4e62",
            "2026-01-03T05:59:59.999Z",
            3,
            Some(PREV_HASH_0123),
            "midnight-before.json",
        ),
        (
            6,
            "0190a6f0-7c1d-7e2a-9b3c-0a1b2c3d4e63",
            "2026-01-03T06:00:00.000Z",
            4,
            Some(PREV_HASH_FEDCBA),
            "midnight-after.json",
        ),
    ];
    for (index, event_id, ts, seq, prev_hash, fixture) in cases {
        let fields = writer_fields(event_id, ts, seq, prev_hash);
        let activity =
            project_session_activity(&rows[index], &contract, &trusted_context(), &fields)
                .expect("recorded session row projects");
        assert_eq!(to_json(activity), siem_fixture(fixture), "{fixture}");
    }
}

#[test]
fn projection_refuses_unresolved_identity_and_malformed_inputs() {
    let (_directory, store) = new_store();
    let rows = record_task_stream(&store);
    let row = &rows[5];
    let good = trusted_context();
    let contract = task_contract(TASK);
    let ts = "2026-01-03T04:05:06.789Z";
    let good_fields = || writer_fields(SESSION_CREATED_EVENT_ID, ts, 0, None);

    let unresolved = TrustedContext {
        identity: None,
        ..good.clone()
    };
    assert_eq!(
        project_session_activity(row, &contract, &unresolved, &good_fields()),
        Err(ProjectionError::IdentityUnresolved)
    );

    let v7_home = TrustedContext {
        home_id: "6f1c2a4e-9b3d-7c5e-8f60-718293a4b5c6".into(),
        ..good.clone()
    };
    assert_eq!(
        project_session_activity(row, &contract, &v7_home, &good_fields()),
        Err(ProjectionError::InvalidHomeId)
    );

    let malformed_user = TrustedContext {
        identity: Some(ResolvedIdentity {
            user_id: "hmac-sha256:NOT-HEX".into(),
            source: IdentitySource::GithubLogin,
        }),
        ..good.clone()
    };
    assert_eq!(
        project_session_activity(row, &contract, &malformed_user, &good_fields()),
        Err(ProjectionError::InvalidUserId)
    );

    let bad_zone = TrustedContext {
        timezone: "Mars/Olympus".into(),
        ..good.clone()
    };
    assert_eq!(
        project_session_activity(row, &contract, &bad_zone, &good_fields()),
        Err(ProjectionError::InvalidTimezone("Mars/Olympus".into()))
    );

    let v4_event = writer_fields("0190a6f0-7c1d-4e2a-9b3c-0a1b2c3d4e5f", ts, 0, None);
    assert_eq!(
        project_session_activity(row, &contract, &good, &v4_event),
        Err(ProjectionError::InvalidEventId)
    );

    let missing_prev = writer_fields(SESSION_CREATED_EVENT_ID, ts, 1, None);
    assert_eq!(
        project_session_activity(row, &contract, &good, &missing_prev),
        Err(ProjectionError::InvalidPrevHash(1))
    );

    let genesis_with_prev = writer_fields(SESSION_CREATED_EVENT_ID, ts, 0, Some(PREV_HASH_0123));
    assert_eq!(
        project_session_activity(row, &contract, &good, &genesis_with_prev),
        Err(ProjectionError::InvalidPrevHash(0))
    );

    let impossible_date = writer_fields(
        SESSION_CREATED_EVENT_ID,
        "2026-02-30T04:05:06.789Z",
        0,
        None,
    );
    assert_eq!(
        project_session_activity(row, &contract, &good, &impossible_date),
        Err(ProjectionError::InvalidTimestamp)
    );

    let no_millis = writer_fields(SESSION_CREATED_EVENT_ID, "2026-01-03T04:05:06Z", 0, None);
    assert_eq!(
        project_session_activity(row, &contract, &good, &no_millis),
        Err(ProjectionError::InvalidTimestamp)
    );

    let foreign_user_actor = WriterFields {
        actor_type: ActorType::User,
        actor_id: "someone-else".into(),
        ..good_fields()
    };
    assert_eq!(
        project_session_activity(row, &contract, &good, &foreign_user_actor),
        Err(ProjectionError::InvalidActor)
    );

    let empty_harness = TaskContract {
        agent: String::new(),
        ..task_contract(TASK)
    };
    assert_eq!(
        project_session_activity(row, &empty_harness, &good, &good_fields()),
        Err(ProjectionError::InvalidAgent)
    );

    assert_eq!(
        project_session_activity(row, &task_contract("task-2"), &good, &good_fields()),
        Err(ProjectionError::TaskMismatch {
            contract: "task-2".into(),
            task: TASK.into(),
        })
    );
}

#[test]
fn unknown_payload_keys_do_not_change_the_projection() {
    let (_directory, store) = new_store();
    let mut row = record_task_stream(&store).swap_remove(5);
    row.payload["future_extension"] = json!({"tolerated": true});
    let fields = writer_fields(
        SESSION_CREATED_EVENT_ID,
        "2026-01-03T04:05:06.789Z",
        0,
        None,
    );
    let activity =
        project_session_activity(&row, &task_contract(TASK), &trusted_context(), &fields)
            .expect("unknown payload keys are ignored");
    assert_eq!(to_json(activity), siem_fixture("session-created.json"));
}

#[test]
fn released_cancellation_row_matches_golden_fixture() {
    let (_directory, store) = new_store();
    record_task_stream(&store);
    let owner = CancellationOwner::new(
        TASK.into(),
        "attempt-1".into(),
        SESSION.into(),
        "0123456789abcdef0123456789abcdef".into(),
    )
    .expect("valid cancellation owner");
    store
        .register_cancellation_owner(&owner)
        .expect("register cancellation owner");
    store
        .update_session_state(TASK, SESSION, SessionState::Aborted)
        .expect("abort session");
    store
        .update_attempt_state(TASK, "attempt-1", AttemptState::Cancelled)
        .expect("cancel attempt");
    store
        .manually_resolve_cancellation_owner(&owner, true, None, "operator confirmed exit")
        .expect("manually release cancellation owner");
    let rows = store
        .task_events_after(TASK, 0, 100)
        .expect("query task lineage");
    let row = rows.last().expect("release is the last journal row");
    assert_eq!(row.kind, "cancellation.owner_manually_released");
    let fields = WriterFields {
        actor_type: ActorType::User,
        actor_id: USER_ID.into(),
        ..writer_fields(
            CANCELLATION_EVENT_ID,
            "2026-01-03T04:05:08.000Z",
            2,
            Some(PREV_HASH_FEDCBA),
        )
    };
    let activity = project_session_activity(row, &task_contract(TASK), &trusted_context(), &fields)
        .expect("cancellation release projects");
    assert_eq!(
        to_json(activity),
        siem_fixture("cancellation-released.json")
    );
}

#[test]
fn unmapped_kind_and_empty_session_refuse_without_a_partial_event() {
    let (_directory, store) = new_store();
    // The journal writes no session-scoped kind outside the released mapping (session.created,
    // session.state_changed, cancellation.owner_manually_released). These boundary inputs alter a
    // real session row's kind and session to reach both refusals.
    let mut row = record_task_stream(&store).swap_remove(5);
    let contract = task_contract(TASK);
    let context = trusted_context();
    let fields = writer_fields(
        SESSION_CREATED_EVENT_ID,
        "2026-01-03T04:05:06.789Z",
        0,
        None,
    );
    row.kind = "tool.invoked".into();
    assert_eq!(
        project_session_activity(&row, &contract, &context, &fields),
        Err(ProjectionError::UnmappedKind("tool.invoked".into()))
    );
    row.kind = "session.created".into();
    row.session_id = Some(String::new());
    assert_eq!(
        project_session_activity(&row, &contract, &context, &fields),
        Err(ProjectionError::SessionRequired {
            kind: "session.created".into(),
        })
    );
}
