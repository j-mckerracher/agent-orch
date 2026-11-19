//! Project coordinators as domain orchestrators (DPES-1108 phase 7, migrations 0069 and 0070).
//!
//! The store's public API does the writes. A raw second connection tries the writes the
//! database itself must refuse, as another process would.

use std::fs;
use std::path::Path;

use orchestrator_core::charter::ReportVerb;
use orchestrator_core::contract::{
    AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, VerificationPolicy,
};
use orchestrator_core::task::{Attempt, AttemptState, TaskEvent};
use orchestrator_core::Project;
use orchestrator_store::{
    AuthorityError, ConversationError, CoordinatorError, CoordinatorFact, NewCoordinatorFact,
    OriginKind, SqliteStore, Store, StoreError,
};
use sqlx::migrate::Migrator;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::Connection;

const GLOBAL: &str = "conversation:global";
const PROJECT_SCOPE: &str = "conversation:project:p";

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn try_sql(path: &Path, sql: &str) -> Result<(), sqlx::Error> {
    runtime().block_on(async {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .foreign_keys(true);
        let mut connection = sqlx::SqliteConnection::connect_with(&options).await?;
        // Test-only helper: every statement passed in is a literal written in this file.
        sqlx::query(sqlx::AssertSqlSafe(sql.to_owned()))
            .execute(&mut connection)
            .await?;
        Ok::<(), sqlx::Error>(())
    })
}

fn scalars(path: &Path, statement: &'static str) -> Vec<String> {
    runtime().block_on(async {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .foreign_keys(true);
        let mut connection = sqlx::SqliteConnection::connect_with(&options)
            .await
            .unwrap();
        sqlx::query_scalar::<_, String>(statement)
            .fetch_all(&mut connection)
            .await
            .unwrap()
    })
}

fn project(store: &SqliteStore, id: &str) {
    store
        .register_project(&Project {
            id: id.into(),
            root_path: format!("/repo/{id}"),
            settings: serde_json::json!({}),
        })
        .unwrap();
}

/// A store with project `p` and its coordinator, and the two conversations proposals need.
fn open(directory: &Path) -> (SqliteStore, std::path::PathBuf) {
    let path = directory.join("store.sqlite");
    let store = SqliteStore::new(&path).unwrap();
    project(&store, "p");
    store.restore_coordinator("p").unwrap();
    for statement in [
        "INSERT OR IGNORE INTO conversations (id, target_kind) VALUES ('conversation:global', 'global')",
        "INSERT OR IGNORE INTO conversations (id, target_kind, project_id) \
         VALUES ('conversation:project:p', 'project', 'p')",
        "INSERT INTO conversation_turns (id, conversation_id, state, runtime_name, runtime_version) \
         VALUES ('turn-g', 'conversation:global', 'completed', 'test-runtime', '1.0.0')",
        "INSERT INTO conversation_turns (id, conversation_id, state, runtime_name, runtime_version) \
         VALUES ('turn-c', 'conversation:project:p', 'completed', 'test-runtime', '1.0.0')",
    ] {
        try_sql(&path, statement).unwrap();
    }
    (store, path)
}

/// An approved proposal of `kind` in `scope`, as the resolve path leaves it before it runs.
fn approved(path: &Path, id: &str, scope: &str, kind: &str, idx: u32) {
    let turn = if scope == GLOBAL { "turn-g" } else { "turn-c" };
    try_sql(
        path,
        &format!(
            "INSERT INTO action_proposals (id, scope, turn_id, idx, kind, params_json, digest, \
             subject, state) VALUES ('{id}', '{scope}', '{turn}', {idx}, '{kind}', '{{}}', \
             '{:064x}', 's-{id}', 'approved')",
            idx + 1
        ),
    )
    .unwrap();
}

fn make_task(store: &SqliteStore, id: &str, project_id: &str) {
    store
        .create(&TaskContract {
            id: id.into(),
            project_id: project_id.into(),
            parent_id: None,
            objective: "Fix the export".into(),
            kind: TaskKind::Implement,
            agent: "codex".into(),
            dependencies: Vec::new(),
            verification: VerificationPolicy::None,
            delivery: DeliveryPolicy::LocalReview,
            autonomy: AutonomyPolicy::WorkspaceWrites,
            max_retries: 0,
            max_repairs: 0,
            model_selection: None,
            work_item: None,
            permission: Default::default(),
        })
        .unwrap();
    store.transition(id, TaskEvent::Admit).unwrap();
}

fn coordinator_error(error: &StoreError) -> Option<CoordinatorError> {
    CoordinatorError::of(error)
}

fn route(store: &SqliteStore, path: &Path, request: &str, proposal: &str, idx: u32) {
    approved(path, proposal, GLOBAL, "coordinator.route", idx);
    store
        .create_routed_request(request, "p", "Fix the nightly export.", proposal)
        .unwrap();
}

#[test]
fn a_scope_sentence_needs_an_active_coordinator_and_one_clean_line() {
    let directory = tempfile::tempdir().unwrap();
    let (store, _) = open(directory.path());
    assert_eq!(store.coordinator_scope_sentence("p").unwrap(), None);
    store
        .set_coordinator_scope_sentence("p", Some("Payments and invoices."))
        .unwrap();
    assert_eq!(
        store.coordinator_scope_sentence("p").unwrap().as_deref(),
        Some("Payments and invoices.")
    );
    let refused = store
        .set_coordinator_scope_sentence("p", Some("two\nlines"))
        .unwrap_err();
    assert_eq!(coordinator_error(&refused), Some(CoordinatorError::BadText));
    store.set_coordinator_scope_sentence("p", None).unwrap();
    assert_eq!(store.coordinator_scope_sentence("p").unwrap(), None);

    project(&store, "q");
    let refused = store
        .set_coordinator_scope_sentence("q", Some("Docs."))
        .unwrap_err();
    assert_eq!(
        coordinator_error(&refused),
        Some(CoordinatorError::NotActive)
    );
}

#[test]
fn only_an_approved_global_route_creates_a_request_and_only_for_an_active_coordinator() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = open(directory.path());
    // A coordinator's own proposal cannot route: depth stays two.
    approved(&path, "pr-c", PROJECT_SCOPE, "coordinator.route", 0);
    let refused = store
        .create_routed_request("r-1", "p", "Work.", "pr-c")
        .unwrap_err();
    assert_eq!(
        coordinator_error(&refused),
        Some(CoordinatorError::WrongOrigin)
    );
    // Nor can a proposal of another kind.
    approved(&path, "pr-t", GLOBAL, "task.propose", 0);
    let refused = store
        .create_routed_request("r-1", "p", "Work.", "pr-t")
        .unwrap_err();
    assert_eq!(
        coordinator_error(&refused),
        Some(CoordinatorError::WrongOrigin)
    );

    route(&store, &path, "r-1", "pr-g", 1);
    let again = store
        .create_routed_request("r-1", "p", "Fix the nightly export.", "pr-g")
        .unwrap();
    assert_eq!(again.id, "r-1");
    assert!(!again.answered);
    assert_eq!(store.routed_requests("p", 10).unwrap().len(), 1);

    project(&store, "q");
    approved(&path, "pr-g2", GLOBAL, "coordinator.route", 2);
    let refused = store
        .create_routed_request("r-2", "q", "Work.", "pr-g2")
        .unwrap_err();
    assert_eq!(
        coordinator_error(&refused),
        Some(CoordinatorError::NotActive)
    );
    // The database refuses it too.
    assert!(try_sql(
        &path,
        "INSERT INTO routed_requests (id, project_id, work, origin_proposal_id) \
         VALUES ('r-3', 'q', 'Work.', 'pr-x')"
    )
    .is_err());
    // A request never changes or goes away.
    assert!(try_sql(&path, "UPDATE routed_requests SET work = 'Other.'").is_err());
    assert!(try_sql(&path, "DELETE FROM routed_requests").is_err());
}

#[test]
fn a_request_stays_open_until_its_coordinator_answers_it_once() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = open(directory.path());
    route(&store, &path, "r-1", "pr-g", 0);
    // A caveat does not close it.
    approved(&path, "pr-c1", PROJECT_SCOPE, "coordinator.report", 0);
    store
        .record_coordinator_report(
            "rep-1",
            "p",
            ReportVerb::Caveat,
            Some("r-1"),
            None,
            "CI is slow today.",
            "pr-c1",
        )
        .unwrap();
    assert!(!store.routed_request("r-1").unwrap().unwrap().answered);
    // The global orchestrator cannot answer for the coordinator.
    approved(&path, "pr-g-report", GLOBAL, "coordinator.report", 1);
    let refused = store
        .record_coordinator_report(
            "rep-x",
            "p",
            ReportVerb::Answer,
            Some("r-1"),
            None,
            "Done.",
            "pr-g-report",
        )
        .unwrap_err();
    assert_eq!(
        coordinator_error(&refused),
        Some(CoordinatorError::WrongOrigin)
    );

    approved(&path, "pr-c2", PROJECT_SCOPE, "coordinator.report", 1);
    store
        .record_coordinator_report(
            "rep-2",
            "p",
            ReportVerb::Answer,
            Some("r-1"),
            Some("export"),
            "The export runs again.",
            "pr-c2",
        )
        .unwrap();
    assert!(store.routed_request("r-1").unwrap().unwrap().answered);
    // A second answer is refused, by the store and by the database.
    approved(&path, "pr-c3", PROJECT_SCOPE, "coordinator.report", 2);
    let refused = store
        .record_coordinator_report(
            "rep-3",
            "p",
            ReportVerb::Answer,
            Some("r-1"),
            None,
            "Again.",
            "pr-c3",
        )
        .unwrap_err();
    assert_eq!(
        coordinator_error(&refused),
        Some(CoordinatorError::RequestNotOpen)
    );
    assert!(try_sql(
        &path,
        "INSERT INTO coordinator_reports (id, project_id, author, verb, request_id, note, \
         origin_proposal_id) VALUES ('rep-4', 'p', 'coordinator', 'answer', 'r-1', 'X', 'pr-z')"
    )
    .is_err());
    // An answer needs a request, and a coordinator row needs its origin and a note.
    assert!(try_sql(
        &path,
        "INSERT INTO coordinator_reports (id, project_id, author, verb, note, origin_proposal_id) \
         VALUES ('rep-5', 'p', 'coordinator', 'answer', 'X', 'pr-y')"
    )
    .is_err());
    // Replaying a recorded report returns it.
    let replay = store
        .record_coordinator_report(
            "rep-2",
            "p",
            ReportVerb::Answer,
            Some("r-1"),
            Some("export"),
            "The export runs again.",
            "pr-c2",
        )
        .unwrap();
    assert_eq!(replay.verb, "answer");
    assert!(try_sql(&path, "UPDATE coordinator_reports SET note = 'Other.'").is_err());
    assert!(try_sql(&path, "DELETE FROM coordinator_reports").is_err());
    let newest = store.coordinator_reports("p", 10).unwrap();
    assert_eq!(
        newest
            .iter()
            .map(|report| report.id.as_str())
            .collect::<Vec<_>>(),
        ["rep-2", "rep-1"]
    );
}

#[test]
fn a_task_owner_is_its_own_coordinator_written_once_with_an_open_request() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = open(directory.path());
    route(&store, &path, "r-1", "pr-g", 0);
    make_task(&store, "t1", "p");
    store.set_task_coordinator("t1", "p", Some("r-1")).unwrap();
    // The same owner again changes nothing.
    store.set_task_coordinator("t1", "p", Some("r-1")).unwrap();
    assert_eq!(
        store.task_coordinator("t1").unwrap(),
        Some(("p".to_owned(), Some("r-1".to_owned())))
    );
    let refused = store.set_task_coordinator("t1", "p", None).unwrap_err();
    assert_eq!(
        coordinator_error(&refused),
        Some(CoordinatorError::WrongOwner)
    );
    assert!(try_sql(
        &path,
        "UPDATE tasks SET coordinator_project_id = NULL WHERE id = 't1'"
    )
    .is_err());

    project(&store, "q");
    make_task(&store, "t2", "q");
    let refused = store.set_task_coordinator("t2", "p", None).unwrap_err();
    assert_eq!(
        coordinator_error(&refused),
        Some(CoordinatorError::WrongOwner)
    );
    assert!(try_sql(
        &path,
        "UPDATE tasks SET coordinator_project_id = 'p' WHERE id = 't2'"
    )
    .is_err());

    make_task(&store, "t3", "p");
    let refused = store
        .set_task_coordinator("t3", "p", Some("r-unknown"))
        .unwrap_err();
    assert_eq!(
        coordinator_error(&refused),
        Some(CoordinatorError::RequestNotOpen)
    );
    store.set_task_coordinator("t3", "p", None).unwrap();
    assert_eq!(
        store.task_coordinator("t3").unwrap(),
        Some(("p".to_owned(), None))
    );
}

/// A task a coordinator starts on its own cites the open routed request it serves as its
/// origin (DPES-1108 integration): only the request the task's owner row names, and only while
/// it has no answer.
#[test]
fn a_routed_request_is_the_origin_only_of_a_task_that_serves_it_while_it_is_open() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = open(directory.path());
    route(&store, &path, "r-1", "pr-g1", 0);
    route(&store, &path, "r-2", "pr-g2", 1);
    make_task(&store, "t1", "p");
    store.set_task_coordinator("t1", "p", Some("r-1")).unwrap();
    let origin = store
        .record_task_routed_origin("t1", "r-1", "pr-c1")
        .unwrap();
    assert_eq!(origin.kind, OriginKind::RoutedRequest);
    assert_eq!(origin.origin_ref, "r-1");
    // The same record again returns it; another origin for the task is refused.
    assert_eq!(
        store
            .record_task_routed_origin("t1", "r-1", "pr-c1")
            .unwrap(),
        origin
    );
    let refused = store
        .record_task_origin("t1", "turn-c", "pr-c1")
        .unwrap_err();
    assert_eq!(
        AuthorityError::of(&refused),
        Some(AuthorityError::AlreadyRecorded)
    );
    assert_eq!(store.task_origin("t1").unwrap(), Some(origin));

    // A request the task does not serve, or no request at all, is no origin.
    make_task(&store, "t2", "p");
    store.set_task_coordinator("t2", "p", None).unwrap();
    for request in ["r-1", "r-2"] {
        let refused = store
            .record_task_routed_origin("t2", request, "pr-c2")
            .unwrap_err();
        assert_eq!(
            AuthorityError::of(&refused),
            Some(AuthorityError::NotOpenRequest)
        );
    }

    // An answered request is closed, so it is no origin either.
    make_task(&store, "t3", "p");
    store.set_task_coordinator("t3", "p", Some("r-2")).unwrap();
    approved(&path, "pr-c3", PROJECT_SCOPE, "coordinator.report", 0);
    store
        .record_coordinator_report(
            "rep-1",
            "p",
            ReportVerb::Answer,
            Some("r-2"),
            None,
            "Done.",
            "pr-c3",
        )
        .unwrap();
    let refused = store
        .record_task_routed_origin("t3", "r-2", "pr-c4")
        .unwrap_err();
    assert_eq!(
        AuthorityError::of(&refused),
        Some(AuthorityError::NotOpenRequest)
    );
    assert_eq!(store.task_origin("t3").unwrap(), None);
}

#[test]
fn daemon_facts_land_only_for_owned_tasks_once_and_carry_the_request() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = open(directory.path());
    route(&store, &path, "r-1", "pr-g", 0);
    make_task(&store, "t1", "p");
    make_task(&store, "t2", "p");
    store.set_task_coordinator("t1", "p", Some("r-1")).unwrap();
    let fact = |id: &str, task: &str| NewCoordinatorFact {
        id: id.into(),
        project_id: "p".into(),
        fact: CoordinatorFact::TaskFailed,
        task_id: task.into(),
        key: None,
    };
    assert_eq!(
        store
            .record_coordinator_facts(&[fact("f-1", "t1"), fact("f-2", "t2")])
            .unwrap(),
        1
    );
    assert_eq!(
        store
            .record_coordinator_facts(&[fact("f-1", "t1")])
            .unwrap(),
        0
    );
    let reports = store.coordinator_reports("p", 10).unwrap();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].author, "daemon");
    assert_eq!(reports[0].verb, "task_failed");
    assert_eq!(reports[0].request_id.as_deref(), Some("r-1"));
    // A daemon fact carries no text.
    assert_eq!(reports[0].note, None);

    let unwoken = store.unwoken_coordinator_reports(10).unwrap();
    assert_eq!(unwoken.len(), 1);
    assert_eq!(store.unwoken_routed_requests(10).unwrap().len(), 1);
    store
        .mark_coordinator_wakes_written(&["f-1".to_owned()], &["r-1".to_owned()])
        .unwrap();
    assert!(store.unwoken_coordinator_reports(10).unwrap().is_empty());
    assert!(store.unwoken_routed_requests(10).unwrap().is_empty());
}

#[test]
fn an_owned_task_is_supervised_by_its_coordinator_and_an_idle_coordinator_gets_no_check() {
    let directory = tempfile::tempdir().unwrap();
    let (store, _) = open(directory.path());
    make_task(&store, "t1", "p");
    store.set_task_coordinator("t1", "p", None).unwrap();
    // Queued work runs nothing, so no scope holds work yet.
    assert!(store.supervision_load().unwrap().is_empty());
    assert_eq!(
        store.task_wake_facts("t1").unwrap().unwrap().scope,
        PROJECT_SCOPE
    );
    store
        .add_attempt(
            "t1",
            &Attempt {
                id: "t1-a1".into(),
                task_id: "t1".into(),
                attempt_number: 1,
                state: AttemptState::Pending,
                failure_reason: None,
                summary: None,
                changed_files: None,
            },
        )
        .unwrap();
    store.transition("t1", TaskEvent::ExecutionStarted).unwrap();
    let loads = store.supervision_load().unwrap();
    // The coordinator holds the work; Global does not count what it routed away.
    assert_eq!(loads.len(), 1, "{loads:?}");
    assert_eq!(loads[0].scope, PROJECT_SCOPE);
    assert_eq!(loads[0].work_units, 1);
    let summaries = store.coordinator_summaries().unwrap();
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].open_tasks, 1);
    assert_eq!(summaries[0].open_requests, 0);
}

#[test]
fn a_coordinator_with_open_work_cannot_be_removed() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = open(directory.path());
    route(&store, &path, "r-1", "pr-g", 0);
    let open_work = |store: &SqliteStore| match store.remove_coordinator("p").unwrap_err() {
        ConversationError::Store(error) => coordinator_error(&error),
        other => panic!("unexpected {other:?}"),
    };
    assert_eq!(open_work(&store), Some(CoordinatorError::OpenWork));
    // The database holds the same rule.
    assert!(try_sql(
        &path,
        "UPDATE coordinator_scopes SET removed = 1 WHERE project_id = 'p'"
    )
    .is_err());

    approved(&path, "pr-c", PROJECT_SCOPE, "coordinator.report", 0);
    store
        .record_coordinator_report(
            "rep-1",
            "p",
            ReportVerb::Answer,
            Some("r-1"),
            None,
            "Done.",
            "pr-c",
        )
        .unwrap();
    make_task(&store, "t1", "p");
    store.set_task_coordinator("t1", "p", None).unwrap();
    assert_eq!(open_work(&store), Some(CoordinatorError::OpenWork));

    store.transition("t1", TaskEvent::CancelRequested).unwrap();
    let change = store.remove_coordinator("p").unwrap();
    assert!(change.removed);
    assert!(store.coordinator_scope("p").unwrap().removed);
    // The tasks of a removed coordinator return to the global orchestrator.
    assert_eq!(store.task_wake_facts("t1").unwrap().unwrap().scope, GLOBAL);
}

#[test]
fn migrations_0069_and_0070_apply_to_a_0068_style_database_and_keep_every_row() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("store.sqlite");
    let legacy_dir = directory.path().join("legacy-migrations");
    fs::create_dir(&legacy_dir).unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    for entry in fs::read_dir(&source).unwrap() {
        let entry = entry.unwrap().path();
        let name = entry.file_name().unwrap().to_str().unwrap().to_owned();
        let version: u32 = name.split('_').next().unwrap().parse().unwrap();
        if version < 69 {
            fs::copy(&entry, legacy_dir.join(name)).unwrap();
        }
    }
    runtime().block_on(async {
        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .foreign_keys(true);
        let mut connection = sqlx::SqliteConnection::connect_with(&options)
            .await
            .unwrap();
        Migrator::new(legacy_dir.as_path())
            .await
            .unwrap()
            .run(&mut connection)
            .await
            .unwrap();
        connection.close().await.unwrap();
    });
    for statement in [
        "INSERT INTO projects (id, root_path, settings) VALUES ('p1', '/work/p1', '{}')",
        "INSERT INTO tasks (id, project_id, parent_id, state, contract_json, version) \
         VALUES ('t1', 'p1', NULL, '\"Queued\"', '{}', 1)",
        "INSERT INTO coordinator_scopes (project_id, removed) VALUES ('p1', 0)",
        "INSERT INTO conversations (id, target_kind) VALUES ('conversation:global', 'global')",
        "INSERT INTO conversation_turns (id, conversation_id, state, runtime_name, runtime_version) \
         VALUES ('turn-1', 'conversation:global', 'completed', 'test-runtime', '1.0.0')",
        "INSERT INTO action_proposals (id, scope, turn_id, idx, kind, params_json, digest, subject) \
         VALUES ('pr1', 'conversation:global', 'turn-1', 0, 'task.message', '{}', \
                 '0000000000000000000000000000000000000000000000000000000000000001', 's')",
        "INSERT INTO wake_records (id, scope, event_kind, subject, dedupe_key, notice, \
         needs_model, state) VALUES ('w1', 'conversation:global', 'worker_blocked', 't1', \
         'wait:t1:1', 'Task t1 waits.', 1, 'pending')",
        "INSERT INTO decision_holds (id, subject_kind, subject_id, set_by, question, options_json) \
         VALUES ('q1', 'question', 't1', 'worker', 'Which?', '[]')",
    ] {
        try_sql(&path, statement).unwrap();
    }
    let dump = |path: &Path| -> Vec<Vec<String>> {
        [
            "SELECT json_array(rowid, id, kind, state, grant_tried) FROM action_proposals \
             ORDER BY rowid",
            "SELECT json_array(rowid, id, event_kind, state, notice) FROM wake_records \
             ORDER BY rowid",
            "SELECT json_array(rowid, id, subject_kind, state) FROM decision_holds ORDER BY rowid",
            "SELECT json_array(rowid, id, state) FROM tasks ORDER BY rowid",
            "SELECT json_array(project_id, removed) FROM coordinator_scopes ORDER BY project_id",
        ]
        .into_iter()
        .map(|statement| scalars(path, statement))
        .collect()
    };
    let before = dump(&path);
    let new_kind = "INSERT INTO action_proposals \
         (id, scope, turn_id, idx, kind, params_json, digest, subject) \
         VALUES ('pr2', 'conversation:global', 'turn-1', 1, 'coordinator.route', '{}', \
                 '0000000000000000000000000000000000000000000000000000000000000002', 's')";
    let new_wake = "INSERT INTO wake_records (id, scope, event_kind, subject, dedupe_key, \
         notice, needs_model, state) VALUES ('w2', 'conversation:global', \
         'coordinator_report', 'rep-1', 'coordinator_report:rep-1', 'A report.', 1, 'pending')";
    assert!(try_sql(&path, new_kind).is_err());
    assert!(try_sql(&path, new_wake).is_err());

    let _store = SqliteStore::new(&path).unwrap();
    assert_eq!(dump(&path), before);
    try_sql(&path, new_kind).unwrap();
    try_sql(&path, new_wake).unwrap();
    // The old rules came back with the rebuilt tables.
    assert!(try_sql(&path, "DELETE FROM action_proposals WHERE id = 'pr1'").is_err());
    assert!(try_sql(&path, "UPDATE action_proposals SET kind = 'task.retry'").is_err());
    assert!(try_sql(&path, "DELETE FROM wake_records").is_err());
    assert!(try_sql(&path, "UPDATE wake_records SET event_kind = 'task_failed'").is_err());
    assert!(try_sql(
        &path,
        "INSERT INTO decision_holds (id, subject_kind, subject_id, until_date, until_ms, set_by) \
         VALUES ('h2', 'proposal', 'missing', '2026-11-18', 1795046399999, 'user')"
    )
    .is_err());
    // An existing task has no owner, and the coordinator row has no scope sentence.
    assert_eq!(
        scalars(
            &path,
            "SELECT json_array(coordinator_project_id, routed_request_id) FROM tasks"
        ),
        ["[null,null]"]
    );
    assert_eq!(
        scalars(
            &path,
            "SELECT json_array(scope_sentence) FROM coordinator_scopes"
        ),
        ["[null]"]
    );
}
