//! The user's orchestrator notes (DPES-1105 phase 5b, migration 0050).
//!
//! The store's public API does the reads and writes; a second raw connection tries the writes
//! the database itself must refuse.

use std::path::Path;

use orchestrator_store::{ConversationTarget, SqliteStore, StoreError};
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

fn lines(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

fn open() -> (tempfile::TempDir, SqliteStore) {
    let directory = tempfile::tempdir().unwrap();
    let store = SqliteStore::new(&directory.path().join("store.sqlite")).unwrap();
    (directory, store)
}

#[test]
fn notes_replace_the_scope_list_in_position_order() {
    let (_directory, store) = open();
    store
        .set_orchestrator_notes("global", &lines(&["first", "second", "third"]))
        .unwrap();
    assert_eq!(
        store.orchestrator_notes("global").unwrap(),
        lines(&["first", "second", "third"])
    );
    store
        .set_orchestrator_notes("global", &lines(&["only"]))
        .unwrap();
    assert_eq!(
        store.orchestrator_notes("global").unwrap(),
        lines(&["only"])
    );
    store.set_orchestrator_notes("global", &[]).unwrap();
    assert!(store.orchestrator_notes("global").unwrap().is_empty());
}

#[test]
fn a_refused_notes_write_keeps_the_old_list() {
    let (_directory, store) = open();
    store
        .set_orchestrator_notes("global", &lines(&["keep me"]))
        .unwrap();
    let eleven = vec!["note".to_owned(); 11];
    for refused in [
        eleven,
        lines(&["fine", ""]),
        lines(&["see /Users/someone/code"]),
        vec!["x".repeat(201)],
    ] {
        let error = store
            .set_orchestrator_notes("global", &refused)
            .unwrap_err();
        assert!(matches!(error, StoreError::Io(_)), "{error:?}");
    }
    assert!(store.set_orchestrator_notes("elsewhere", &[]).is_err());
    assert!(store.set_orchestrator_notes("workstream:", &[]).is_err());
    assert_eq!(
        store.orchestrator_notes("global").unwrap(),
        lines(&["keep me"])
    );
}

#[test]
fn the_notes_table_refuses_a_bad_scope_position_or_body() {
    let (directory, _store) = open();
    let path = directory.path().join("store.sqlite");
    for sql in [
        "INSERT INTO orchestrator_notes (scope, position, body) VALUES ('elsewhere', 0, 'x')",
        "INSERT INTO orchestrator_notes (scope, position, body) VALUES ('workstream:', 0, 'x')",
        "INSERT INTO orchestrator_notes (scope, position, body) VALUES ('global', 10, 'x')",
        "INSERT INTO orchestrator_notes (scope, position, body) VALUES ('global', 0, '')",
        "INSERT INTO orchestrator_notes (scope, position, body) \
         VALUES ('global', 0, printf('%0201d', 1))",
    ] {
        assert!(try_sql(&path, sql).is_err(), "{sql}");
    }
}

#[test]
fn a_workstream_conversation_lists_its_notes_before_the_global_ones_capped_at_ten() {
    let (_directory, store) = open();
    store
        .set_orchestrator_notes(
            "global",
            &lines(&["g0", "g1", "g2", "g3", "g4", "g5", "g6"]),
        )
        .unwrap();
    store
        .set_orchestrator_notes("workstream:w1", &lines(&["w0", "w1", "w2", "w3", "w4"]))
        .unwrap();
    assert_eq!(
        store
            .conversation_notes(&ConversationTarget::Global)
            .unwrap()
            .len(),
        7
    );
    let own = store
        .conversation_notes(&ConversationTarget::Workstream("w1".to_owned()))
        .unwrap();
    assert_eq!(
        own,
        lines(&["w0", "w1", "w2", "w3", "w4", "g0", "g1", "g2", "g3", "g4"])
    );
    // Another workstream sees only the global notes.
    let other = store
        .conversation_notes(&ConversationTarget::Workstream("w2".to_owned()))
        .unwrap();
    assert_eq!(other, lines(&["g0", "g1", "g2", "g3", "g4", "g5", "g6"]));
}

#[test]
fn clear_history_deletes_that_conversations_notes_only() {
    let (_directory, store) = open();
    store
        .set_orchestrator_notes("global", &lines(&["global note"]))
        .unwrap();
    store
        .set_orchestrator_notes("workstream:w1", &lines(&["w1 note"]))
        .unwrap();
    store
        .set_orchestrator_notes("workstream:w2", &lines(&["w2 note"]))
        .unwrap();

    store
        .clear_conversation_history(&ConversationTarget::Workstream("w1".to_owned()))
        .unwrap();
    assert!(store
        .orchestrator_notes("workstream:w1")
        .unwrap()
        .is_empty());
    assert_eq!(
        store.orchestrator_notes("workstream:w2").unwrap(),
        lines(&["w2 note"])
    );
    assert_eq!(
        store.orchestrator_notes("global").unwrap(),
        lines(&["global note"])
    );

    store
        .clear_conversation_history(&ConversationTarget::Global)
        .unwrap();
    assert!(store.orchestrator_notes("global").unwrap().is_empty());
    assert_eq!(
        store.orchestrator_notes("workstream:w2").unwrap(),
        lines(&["w2 note"])
    );
}
