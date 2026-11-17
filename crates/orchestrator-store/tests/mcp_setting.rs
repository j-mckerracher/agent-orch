//! The read-only MCP server setting (DPES-1105 phase 6b, migration 0049).
//!
//! The store's public API does the reads and writes. A second raw connection reads the column
//! back and tries the write the database itself must refuse.

use std::path::Path;

use orchestrator_store::{LoadPatch, McpPatch, SettingsPatch, SqliteStore};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::Connection;

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

/// Runs one statement on a second connection and reports whether the database took it.
fn try_sql(path: &Path, sql: &'static str) -> Result<(), sqlx::Error> {
    runtime().block_on(async {
        let options = SqliteConnectOptions::new().filename(path);
        let mut connection = sqlx::SqliteConnection::connect_with(&options).await?;
        sqlx::query(sql).execute(&mut connection).await?;
        Ok::<(), sqlx::Error>(())
    })
}

fn column(path: &Path) -> String {
    runtime().block_on(async {
        let options = SqliteConnectOptions::new().filename(path);
        let mut connection = sqlx::SqliteConnection::connect_with(&options)
            .await
            .unwrap();
        sqlx::query_scalar::<_, String>(
            "SELECT CAST(mcp_server_enabled AS TEXT) FROM orchestrator_settings WHERE id = 1",
        )
        .fetch_one(&mut connection)
        .await
        .unwrap()
    })
}

fn mcp(enabled: bool) -> SettingsPatch {
    SettingsPatch {
        mcp: McpPatch {
            enabled: Some(enabled),
        },
        ..SettingsPatch::default()
    }
}

#[test]
fn mcp_setting_is_off_by_default_and_the_database_keeps_it_zero_or_one() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("store.sqlite");
    let store = SqliteStore::new(&path).unwrap();
    assert!(!store.orchestrator_settings().unwrap().mcp.enabled);
    assert_eq!(column(&path), "0");
    assert!(try_sql(
        &path,
        "UPDATE orchestrator_settings SET mcp_server_enabled = 2"
    )
    .is_err());
    assert_eq!(column(&path), "0");
}

#[test]
fn mcp_setting_changes_only_by_a_patch_that_names_it() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("store.sqlite");
    let store = SqliteStore::new(&path).unwrap();

    // A change to a load limit leaves the setting off.
    store
        .update_orchestrator_settings(&SettingsPatch {
            load: LoadPatch {
                cpu: Some(70),
                ..LoadPatch::default()
            },
            ..SettingsPatch::default()
        })
        .unwrap();
    let after_load = store.orchestrator_settings().unwrap();
    assert!(!after_load.mcp.enabled);

    // Naming it changes that field and nothing else.
    let after_on = store.update_orchestrator_settings(&mcp(true)).unwrap();
    let mut expected = after_load.clone();
    expected.mcp.enabled = true;
    assert_eq!(after_on, expected);
    assert_eq!(column(&path), "1");

    // A later change to another group keeps it on; naming it again turns it off.
    store
        .update_orchestrator_settings(&SettingsPatch {
            chat_pr_delivery: Some(true),
            ..SettingsPatch::default()
        })
        .unwrap();
    assert!(store.orchestrator_settings().unwrap().mcp.enabled);
    assert!(
        !store
            .update_orchestrator_settings(&mcp(false))
            .unwrap()
            .mcp
            .enabled
    );
    assert_eq!(column(&path), "0");
}
