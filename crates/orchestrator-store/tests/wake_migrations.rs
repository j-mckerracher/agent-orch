//! Migrations 0040 and 0041 (DPES-1105 phase 3) applied to a database that already holds data.
//!
//! The database is built the way 0039 left it, with data present, and the store then opens it
//! and applies both migrations.

use std::fs;
use std::path::Path;

use orchestrator_core::supervision::{MonitoringMode, WakeLimits};
use orchestrator_store::{SqliteStore, TurnTrigger};
use sqlx::migrate::Migrator;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::Connection;

const PROPOSAL_DUMP: &str = "SELECT json_array(rowid, id, scope, turn_id, idx, kind, params_json, \
    digest, subject, depends_on, state, result_ref, error_code, created_at, updated_at) \
    FROM action_proposals ORDER BY rowid";

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
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

/// Builds the database as 0039 left it: only migrations up to 0039 applied, a changed load
/// limit, two proposals and three durable events. Returns the proposals as dumped.
fn database_at_0039(directory: &Path) -> Vec<String> {
    let legacy_dir = directory.join("legacy-migrations");
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
        if version <= 39 {
            fs::copy(&entry, legacy_dir.join(entry.file_name().unwrap())).unwrap();
        }
    }
    runtime().block_on(async {
        let options = SqliteConnectOptions::new()
            .filename(directory.join("store.sqlite"))
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
        assert_eq!(newest, 39);
        for sql in [
            "INSERT INTO projects (id, root_path, settings) VALUES ('p1', '/work/p1', '{}')",
            "INSERT INTO tasks (id, project_id, state, contract_json, version) \
             VALUES ('task-a', 'p1', 'Queued', '{}', 1), ('task-b', 'p1', 'Queued', '{}', 1)",
            "UPDATE orchestrator_settings SET load_cpu_percent = 70",
            "INSERT INTO conversations (id, target_kind) VALUES ('conversation:global', 'global')",
            "INSERT INTO conversation_turns (id, conversation_id, state, runtime_name, runtime_version) \
             VALUES ('t1', 'conversation:global', 'completed', 'test-runtime', '1.0.0')",
            "INSERT INTO durable_events (task_id, kind, payload) VALUES ('task-a', 'task.transition', '{}')",
            "INSERT INTO durable_events (task_id, kind, payload) VALUES ('task-a', 'task.transition', '{}')",
            "INSERT INTO durable_events (task_id, kind, payload) VALUES ('task-b', 'task.transition', '{}')",
        ] {
            sqlx::query(sql).execute(&mut connection).await.unwrap();
        }
        for (idx, id) in ["proposal-a", "proposal-b"].iter().enumerate() {
            sqlx::query(
                "INSERT INTO action_proposals \
                 (id, scope, turn_id, idx, kind, params_json, digest, subject) \
                 VALUES (?, 'conversation:global', 't1', ?, 'workstream.create', ?, ?, ?)",
            )
            .bind(*id)
            .bind(i64::try_from(idx).unwrap())
            .bind(format!(r#"{{"kind":"workstream.create","title":"{id}"}}"#))
            .bind(format!("{:064x}", idx + 1))
            .bind(format!("Create {id}"))
            .execute(&mut connection)
            .await
            .unwrap();
        }
        let proposals = sqlx::query_scalar::<_, String>(PROPOSAL_DUMP)
            .fetch_all(&mut connection)
            .await
            .unwrap();
        connection.close().await.unwrap();
        proposals
    })
}

#[test]
fn migrations_0040_and_0041_apply_on_a_copy_of_the_0039_schema() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("store.sqlite");
    let proposals = database_at_0039(directory.path());
    assert!(scalars(
        &path,
        "SELECT name FROM sqlite_master WHERE name = 'wake_records'"
    )
    .is_empty());

    let store = SqliteStore::new(&path).unwrap();
    assert_eq!(
        scalars(
            &path,
            "SELECT CAST(version AS TEXT) || ':' || CAST(success AS TEXT) \
             FROM _sqlx_migrations WHERE version IN (40, 41) ORDER BY version"
        ),
        vec!["40:1", "41:1"]
    );

    // The load limit and the proposals keep their values; the new columns hold defaults.
    let settings = store.orchestrator_settings().unwrap();
    assert_eq!(
        (settings.load.cpu, settings.load.memory, settings.load.disk),
        (70, 85, 90)
    );
    assert_eq!(settings.mode, MonitoringMode::On);
    assert_eq!(
        (
            settings.supervision.base_interval_secs,
            settings.supervision.min_interval_secs,
            settings.supervision.attempt_wall_minutes
        ),
        (600, 60, 120)
    );
    assert_eq!(settings.wake_limits, WakeLimits::default());
    assert_eq!(
        scalars(
            &path,
            "SELECT CAST(COUNT(*) AS TEXT) FROM orchestrator_settings"
        ),
        vec!["1"]
    );
    assert_eq!(scalars(&path, PROPOSAL_DUMP), proposals);

    // The cursor starts at the newest event, so history is never replayed.
    assert_eq!(store.wake_cursor().unwrap(), 3);
    assert_eq!(
        scalars(
            &path,
            "SELECT CAST(MAX(sequence) AS TEXT) FROM durable_events"
        ),
        vec!["3"]
    );

    // A turn from before the migration reads as a user turn.
    let turn = store.conversation_turn("t1").unwrap().unwrap();
    assert_eq!(turn.trigger, TurnTrigger::User);
}
