//! Soft archive of finished tasks (DPES-1106, migration 0062). The store's public API does the
//! writes; a second raw connection reads the rows and the durable events back.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use orchestrator_core::Project;
use orchestrator_store::{ArchiveError, SqliteStore, StoreError};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::Connection;

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

/// Runs one statement on a second connection with `binds` bound in order.
fn sql(path: &Path, statement: &str, binds: &[&str]) {
    runtime().block_on(async {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .foreign_keys(true);
        let mut connection = sqlx::SqliteConnection::connect_with(&options)
            .await
            .unwrap();
        // Test-only helper: every statement passed in is a literal written in this file.
        let mut query = sqlx::query(sqlx::AssertSqlSafe(statement.to_owned()));
        for value in binds {
            query = query.bind(*value);
        }
        query.execute(&mut connection).await.unwrap();
    });
}

/// The first column of every row `statement` returns, read on a second connection.
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

/// A store with the project `p1` registered. Returns the database path with it.
fn store_with_project(directory: &Path) -> (SqliteStore, PathBuf) {
    let path = directory.join("store.sqlite");
    let store = SqliteStore::new(&path).unwrap();
    store
        .register_project(&Project {
            id: "p1".to_owned(),
            root_path: "/work/p1".to_owned(),
            settings: serde_json::json!({}),
        })
        .unwrap();
    (store, path)
}

/// A task row with `state` as the store writes it (JSON text, so `"Running"` carries quotes).
fn add_task(path: &Path, id: &str, state: &str) {
    sql(
        path,
        "INSERT INTO tasks (id, project_id, parent_id, state, contract_json, version) \
         VALUES (?, 'p1', NULL, ?, '{}', 1)",
        &[id, &format!("\"{state}\"")],
    );
}

fn archived_ids(store: &SqliteStore) -> HashSet<String> {
    store.archived_task_ids().unwrap()
}

#[test]
fn finished_tasks_archive_once_keep_their_rows_and_each_emit_one_event() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = store_with_project(directory.path());
    for (id, state) in [
        ("t-ok", "Succeeded"),
        ("t-failed", "Failed"),
        ("t-verify", "VerificationFailed"),
        ("t-cancel", "Cancelled"),
    ] {
        add_task(&path, id, state);
    }

    let ids: Vec<String> = ["t-cancel", "t-ok", "t-failed", "t-verify"]
        .map(str::to_owned)
        .to_vec();
    assert_eq!(store.archive_tasks(&ids).unwrap(), ids);
    assert_eq!(
        archived_ids(&store),
        HashSet::from(["t-ok", "t-failed", "t-verify", "t-cancel"].map(str::to_owned))
    );
    assert_eq!(
        scalars(&path, "SELECT id FROM tasks ORDER BY id").len(),
        4,
        "archived rows stay"
    );
    assert_eq!(
        scalars(&path, "SELECT state FROM tasks WHERE id = 't-failed'"),
        vec![r#""Failed""#]
    );
    assert_eq!(
        scalars(
            &path,
            "SELECT task_id FROM durable_events WHERE kind = 'task.archived' ORDER BY sequence"
        ),
        ["t-cancel", "t-ok", "t-failed", "t-verify"]
    );

    // Re-archiving is a no-op: nothing is returned, and no second event is written.
    assert!(store.archive_tasks(&ids).unwrap().is_empty());
    assert_eq!(
        scalars(
            &path,
            "SELECT task_id FROM durable_events WHERE kind = 'task.archived'"
        )
        .len(),
        4
    );
}

#[test]
fn a_task_that_is_not_finished_refuses_the_whole_call_and_writes_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = store_with_project(directory.path());
    add_task(&path, "t-done", "Succeeded");

    for (index, state) in ["Created", "Queued", "Running", "Verifying"]
        .into_iter()
        .enumerate()
    {
        let open = format!("t-open-{index}");
        add_task(&path, &open, state);
        let error = store
            .archive_tasks(&["t-done".to_owned(), open.clone()])
            .unwrap_err();
        assert_eq!(
            ArchiveError::of(&error),
            Some(ArchiveError::NotFinished),
            "{state}"
        );
        assert!(archived_ids(&store).is_empty(), "{state}");
        assert!(
            scalars(&path, "SELECT task_id FROM durable_events").is_empty(),
            "{state}"
        );
    }
}

#[test]
fn an_unknown_task_is_not_found_and_writes_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = store_with_project(directory.path());
    add_task(&path, "t-done", "Succeeded");

    let error = store
        .archive_tasks(&["t-done".to_owned(), "missing".to_owned()])
        .unwrap_err();
    assert!(
        matches!(
            error,
            StoreError::RecordNotFound { entity: "task", ref id } if id == "missing"
        ),
        "{error:?}"
    );
    assert!(archived_ids(&store).is_empty());
}
