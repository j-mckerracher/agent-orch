//! Scout reports (DPES-1105 phase 4a, migration 0044).
//!
//! These tests use the store's public API. A second raw connection reads the tables and tries
//! the writes the database itself must refuse, as another process would.

use std::fs;
use std::path::Path;

use orchestrator_core::contract::{
    AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, VerificationPolicy,
};
use orchestrator_core::records::Project;
use orchestrator_core::workstream::Workstream;
use orchestrator_store::{
    ConversationTarget, ReportScope, SqliteStore, Store, StoreError, TaskReportError, TurnOutcome,
    MAX_REPORT_BYTES,
};
use serde_json::json;
use sqlx::migrate::Migrator;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::Connection;

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

/// Runs one statement on a second connection, as another process would.
fn try_sql(path: &Path, sql: &'static str) -> Result<u64, sqlx::Error> {
    runtime().block_on(async {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .foreign_keys(true);
        let mut connection = sqlx::SqliteConnection::connect_with(&options).await?;
        Ok(sqlx::query(sql)
            .execute(&mut connection)
            .await?
            .rows_affected())
    })
}

fn scalars(path: &Path, sql: &'static str) -> Vec<String> {
    runtime().block_on(async {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .foreign_keys(true);
        let mut connection = sqlx::SqliteConnection::connect_with(&options)
            .await
            .unwrap();
        sqlx::query_scalar::<_, String>(sql)
            .fetch_all(&mut connection)
            .await
            .unwrap()
    })
}

fn contract(id: &str, project: &str) -> TaskContract {
    TaskContract {
        id: id.into(),
        project_id: project.into(),
        parent_id: None,
        objective: format!("scout for {id}"),
        kind: TaskKind::Investigate,
        agent: "codex".into(),
        dependencies: Vec::new(),
        verification: VerificationPolicy::None,
        delivery: DeliveryPolicy::LocalReview,
        autonomy: AutonomyPolicy::ApprovalRequired,
        max_retries: 0,
        max_repairs: 0,
        model_selection: None,
        permission: Default::default(),
        work_item: None,
    }
}

/// A store with projects p1 and p2, and scouts t1 and t2 in p1 and t3 in p2.
fn seeded(directory: &Path) -> (SqliteStore, std::path::PathBuf) {
    let path = directory.join("store.sqlite");
    let store = SqliteStore::new(&path).unwrap();
    for id in ["p1", "p2"] {
        store
            .register_project(&Project {
                id: id.into(),
                root_path: format!("/work/{id}"),
                settings: json!({}),
            })
            .unwrap();
    }
    for (task, project) in [("t1", "p1"), ("t2", "p1"), ("t3", "p2")] {
        store.create(&contract(task, project)).unwrap();
    }
    (store, path)
}

fn invalid_input(error: &StoreError) -> bool {
    matches!(error, StoreError::Io(io) if io.kind() == std::io::ErrorKind::InvalidInput)
}

#[test]
fn migration_0044_applies_to_a_0043_database_and_keeps_every_row() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("store.sqlite");
    let legacy_dir = directory.path().join("legacy-migrations");
    fs::create_dir(&legacy_dir).unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    for entry in fs::read_dir(&source).unwrap() {
        let entry = entry.unwrap().path();
        let version: u32 = entry
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .split('_')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        if version <= 43 {
            fs::copy(&entry, legacy_dir.join(entry.file_name().unwrap())).unwrap();
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
        let newest: i64 = sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations")
            .fetch_one(&mut connection)
            .await
            .unwrap();
        assert_eq!(newest, 43);
        for sql in [
            "INSERT INTO projects (id, root_path, settings) VALUES ('old', '/work/old', '{}')",
            "INSERT INTO conversations (id, target_kind) VALUES ('conversation:global', 'global')",
            "INSERT INTO conversation_turns \
             (id, conversation_id, state, runtime_name, runtime_version) \
             VALUES ('turn-old', 'conversation:global', 'completed', 'test-runtime', '1.0.0')",
            "UPDATE orchestrator_settings SET load_cpu_percent = 70",
        ] {
            sqlx::query(sql).execute(&mut connection).await.unwrap();
        }
        connection.close().await.unwrap();
    });

    let store = SqliteStore::new(&path).unwrap();
    assert_eq!(
        scalars(
            &path,
            "SELECT CAST(success AS TEXT) FROM _sqlx_migrations WHERE version = 44"
        ),
        vec!["1"]
    );
    assert_eq!(
        scalars(&path, "SELECT json_array(id, root_path) FROM projects"),
        vec![r#"["old","/work/old"]"#]
    );
    assert_eq!(
        scalars(
            &path,
            "SELECT json_array(id, read_report) FROM conversation_turns"
        ),
        vec![r#"["turn-old",0]"#]
    );
    assert_eq!(
        scalars(&path, "SELECT CAST(COUNT(*) AS TEXT) FROM task_reports"),
        vec!["0"]
    );
    assert_eq!(store.orchestrator_settings().unwrap().load.cpu, 70);
    assert_eq!(store.task_report("anything").unwrap(), None);
}

#[test]
fn a_report_is_captured_once_and_never_overwritten() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = seeded(directory.path());

    assert!(store
        .capture_task_report("t1", "a1", "first finding", false, "2026-01-01T00:00:00Z")
        .unwrap());
    assert!(!store
        .capture_task_report("t1", "a2", "second finding", true, "2026-01-02T00:00:00Z")
        .unwrap());

    let report = store.task_report("t1").unwrap().unwrap();
    assert_eq!(report.task_id, "t1");
    assert_eq!(report.attempt_id, "a1");
    assert_eq!(report.body, "first finding");
    assert!(!report.truncated);
    assert_eq!(report.captured_at, "2026-01-01T00:00:00Z");
    assert_eq!(report.relayed_at, None);
    assert_eq!(store.task_report("t2").unwrap(), None);

    // One durable event, from the capture that stored the report, and no text in it.
    assert_eq!(
        scalars(
            &path,
            "SELECT json_array(task_id, kind, payload) FROM durable_events \
             WHERE kind = 'task.report_ready'"
        ),
        vec![r#"["t1","task.report_ready","{\"task_id\":\"t1\"}"]"#]
    );

    // A report belongs to a stored task and to exactly one.
    assert!(matches!(
        store.capture_task_report("missing", "a1", "text", false, "2026-01-01T00:00:00Z"),
        Err(StoreError::RecordNotFound { entity: "task", .. })
    ));
    assert!(try_sql(
        &path,
        "INSERT INTO task_reports (task_id, attempt_id, body, captured_at) \
         VALUES ('t1', 'a9', 'again', '2026-01-03T00:00:00Z')"
    )
    .is_err());
    for (attempt, body, time) in [("", "text", "now"), ("a1", "", "now"), ("a1", "text", "")] {
        let refused = store
            .capture_task_report("t2", attempt, body, false, time)
            .unwrap_err();
        assert!(invalid_input(&refused));
    }
    assert_eq!(
        store
            .capture_task_report("t2", "", "text", false, "now")
            .unwrap_err()
            .to_string(),
        StoreError::from(TaskReportError::EmptyAttempt).to_string()
    );
    assert_eq!(store.task_report("t2").unwrap(), None);
}

#[test]
fn a_report_over_the_cap_is_cut_and_says_so() {
    let directory = tempfile::tempdir().unwrap();
    let (store, _path) = seeded(directory.path());

    // 'e' with an acute accent is two bytes, so the cap falls inside a character when the
    // text starts with one ASCII byte.
    let text = format!("x{}", "\u{e9}".repeat(MAX_REPORT_BYTES));
    assert!(store
        .capture_task_report("t1", "a1", &text, false, "2026-01-01T00:00:00Z")
        .unwrap());
    let cut = store.task_report("t1").unwrap().unwrap();
    assert!(cut.truncated);
    assert!(cut.body.len() <= MAX_REPORT_BYTES);
    assert!(cut.body.len() >= MAX_REPORT_BYTES - 3);
    assert!(text.starts_with(&cut.body));

    // Text at exactly the cap is kept whole; a cut the caller made first is still recorded.
    let exact = "y".repeat(MAX_REPORT_BYTES);
    store
        .capture_task_report("t2", "a1", &exact, false, "2026-01-01T00:00:00Z")
        .unwrap();
    let whole = store.task_report("t2").unwrap().unwrap();
    assert_eq!((whole.body, whole.truncated), (exact, false));
    store
        .capture_task_report("t3", "a1", "short", true, "2026-01-01T00:00:00Z")
        .unwrap();
    assert!(store.task_report("t3").unwrap().unwrap().truncated);
}

#[test]
fn a_report_can_be_marked_relayed_once() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = seeded(directory.path());

    assert!(matches!(
        store.mark_task_report_relayed("t1", "2026-01-02T00:00:00Z"),
        Err(StoreError::RecordNotFound {
            entity: "task report",
            ..
        })
    ));
    store
        .capture_task_report("t1", "a1", "finding", false, "2026-01-01T00:00:00Z")
        .unwrap();
    assert!(store
        .mark_task_report_relayed("t1", "2026-01-02T00:00:00Z")
        .unwrap());
    assert!(!store
        .mark_task_report_relayed("t1", "2026-01-03T00:00:00Z")
        .unwrap());
    let report = store.task_report("t1").unwrap().unwrap();
    assert_eq!(report.relayed_at.as_deref(), Some("2026-01-02T00:00:00Z"));
    assert_eq!(report.body, "finding");

    // The database refuses a second change from another connection, and a clear of it.
    assert!(try_sql(&path, "UPDATE task_reports SET relayed_at = 'later'").is_err());
    assert!(try_sql(&path, "UPDATE task_reports SET relayed_at = NULL").is_err());
    assert!(try_sql(&path, "DELETE FROM task_reports").is_err());
    assert_eq!(
        scalars(&path, "SELECT relayed_at FROM task_reports"),
        vec!["2026-01-02T00:00:00Z"]
    );
}

#[test]
fn a_report_text_cannot_be_updated_after_capture() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = seeded(directory.path());
    store
        .capture_task_report("t1", "a1", "finding", false, "2026-01-01T00:00:00Z")
        .unwrap();

    for sql in [
        "UPDATE task_reports SET body = 'edited'",
        "UPDATE task_reports SET truncated = 1",
        "UPDATE task_reports SET attempt_id = 'a2'",
        "UPDATE task_reports SET captured_at = 'later'",
        "UPDATE task_reports SET task_id = 't2'",
    ] {
        assert!(try_sql(&path, sql).is_err(), "{sql} was accepted");
    }
    assert_eq!(
        scalars(&path, "SELECT json_array(task_id, attempt_id, body, truncated, captured_at) FROM task_reports"),
        vec![r#"["t1","a1","finding",0,"2026-01-01T00:00:00Z"]"#]
    );
    assert_eq!(store.task_report("t1").unwrap().unwrap().body, "finding");
}

#[test]
fn a_report_survives_a_conversation_clear() {
    let directory = tempfile::tempdir().unwrap();
    let (store, _path) = seeded(directory.path());
    store
        .capture_task_report("t1", "a1", "finding", false, "2026-01-01T00:00:00Z")
        .unwrap();
    store
        .begin_conversation_turn(
            &ConversationTarget::Global,
            "turn-1",
            "hello",
            "test-runtime",
            "1.0.0",
        )
        .unwrap();
    store
        .finish_conversation_turn("turn-1", &TurnOutcome::Completed)
        .unwrap();

    store
        .clear_conversation_history(&ConversationTarget::Global)
        .unwrap();

    let report = store.task_report("t1").unwrap().unwrap();
    assert_eq!(report.body, "finding");
}

#[test]
fn reports_list_by_project_and_by_workstream_without_their_text() {
    let directory = tempfile::tempdir().unwrap();
    let (store, _path) = seeded(directory.path());
    store
        .create_workstream(&Workstream::new("w1", "Initiative", "").unwrap(), "user")
        .unwrap();
    store.link_task("w1", "t2", None).unwrap();
    for (task, time) in [
        ("t1", "2026-01-01T00:00:00Z"),
        ("t2", "2026-01-02T00:00:00Z"),
        ("t3", "2026-01-03T00:00:00Z"),
    ] {
        store
            .capture_task_report(task, "a1", "finding", false, time)
            .unwrap();
    }
    store
        .mark_task_report_relayed("t1", "2026-01-04T00:00:00Z")
        .unwrap();

    let in_project = store
        .list_task_reports(ReportScope::Project("p1"), 10)
        .unwrap();
    let ids: Vec<_> = in_project.iter().map(|r| r.task_id.as_str()).collect();
    assert_eq!(ids, ["t2", "t1"]);
    assert_eq!(
        in_project[1].relayed_at.as_deref(),
        Some("2026-01-04T00:00:00Z")
    );
    assert_eq!(in_project[0].relayed_at, None);

    let in_workstream = store
        .list_task_reports(ReportScope::Workstream("w1"), 10)
        .unwrap();
    assert_eq!(in_workstream.len(), 1);
    assert_eq!(in_workstream[0].task_id, "t2");

    assert_eq!(
        store
            .list_task_reports(ReportScope::Project("p1"), 1)
            .unwrap()
            .len(),
        1
    );
    assert!(store
        .list_task_reports(ReportScope::Project("nobody"), 10)
        .unwrap()
        .is_empty());
}
