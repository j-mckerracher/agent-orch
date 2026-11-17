//! Clearing conversation history and removing project coordinators (DPES-1105).
//!
//! These tests use the store's public API and raw SQLite connections, so they see
//! what the daemon sees: the migrations, the triggers, and every file in the data
//! directory.

use std::fs;
use std::path::{Path, PathBuf};

use orchestrator_core::orchestrator_action::{parse_block, proposal_id, BlockScope};
use orchestrator_core::records::Project;
use orchestrator_store::{
    ConversationError, ConversationTarget, IdempotencyStart, ProposalState, SqliteStore, TurnBlock,
    TurnOutcome,
};
use serde_json::json;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::Connection;

const GLOBAL_MESSAGE: &str = "MARKER-GLOBAL-MESSAGE-7f3a";
const GLOBAL_REPLY: &str = "MARKER-GLOBAL-REPLY-91bc";
const COORDINATOR_NOTE: &str = "MARKER-COORDINATOR-NOTE-2c8d";
const COORDINATOR_PROPOSAL: &str = "MARKER-COORDINATOR-PROPOSAL-5e0f";
const ACTION_MARKER: &str = "MARKER-ACTION-TITLE-4d7e";
const MARKED_CREATE: &str = r#"{"kind": "workstream.create", "title": "MARKER-ACTION-TITLE-4d7e"}"#;

fn store_path(directory: &Path) -> PathBuf {
    directory.join("store.sqlite")
}

fn open(directory: &Path) -> SqliteStore {
    SqliteStore::new(&store_path(directory)).unwrap()
}

fn project(id: &str) -> Project {
    Project {
        id: id.into(),
        root_path: format!("/work/{id}"),
        settings: json!({}),
    }
}

fn fingerprint(seed: u64) -> String {
    format!("{seed:064x}")
}

fn begin_global(store: &SqliteStore, turn: &str, text: &str) {
    store
        .begin_conversation_turn(
            &ConversationTarget::Global,
            turn,
            text,
            "test-runtime",
            "1.0.0",
        )
        .unwrap();
}

/// Every regular file under `directory` whose bytes contain `needle`.
fn files_containing(directory: &Path, needle: &str) -> Vec<PathBuf> {
    fn walk(path: &Path, needle: &[u8], found: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, needle, found);
            } else if fs::read(&path)
                .unwrap()
                .windows(needle.len())
                .any(|window| window == needle)
            {
                found.push(path);
            }
        }
    }
    let mut found = Vec::new();
    walk(directory, needle.as_bytes(), &mut found);
    found
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn raw_connect(path: &Path) -> sqlx::SqliteConnection {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .foreign_keys(true);
    runtime()
        .block_on(sqlx::SqliteConnection::connect_with(&options))
        .unwrap()
}

/// One text value read over a raw connection.
fn raw_scalar(directory: &Path, sql: &'static str) -> String {
    let rt = runtime();
    let mut connection = raw_connect(&store_path(directory));
    rt.block_on(sqlx::query_scalar::<_, String>(sql).fetch_one(&mut connection))
        .unwrap()
}

/// Runs one statement on a raw connection and reports whether the database accepted it.
fn accepted(path: &Path, sql: &'static str) -> bool {
    let runtime = runtime();
    let mut connection = raw_connect(path);
    runtime
        .block_on(sqlx::query(sql).execute(&mut connection))
        .is_ok()
}

#[test]
fn a_cleared_conversation_leaves_its_text_in_no_file_of_the_data_directory() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    store.register_project(&project("p1")).unwrap();
    store.restore_coordinator("p1").unwrap();
    begin_global(&store, "t1", GLOBAL_MESSAGE);
    store.append_reply_chunk("t1", GLOBAL_REPLY).unwrap();
    store
        .finish_conversation_turn("t1", &TurnOutcome::Completed)
        .unwrap();
    store
        .append_coordinator_entry("c1", "p1", "message", "user", COORDINATOR_NOTE, None)
        .unwrap();
    store
        .append_coordinator_entry(
            "c2",
            "p1",
            "proposal",
            "coordinator",
            COORDINATOR_PROPOSAL,
            Some(&json!({"status": "invalid", "errors": []})),
        )
        .unwrap();
    // Idempotency rows for both sends, as the daemon writes them: references in the
    // result and a fingerprint that stands in for the request.
    store
        .begin_idempotency("k-global", "conversations.send", &fingerprint(1))
        .unwrap();
    store
        .complete_idempotency(
            "k-global",
            "conversations.send",
            &fingerprint(1),
            &json!({"status": "success", "body": {"entry_id": "t1:user", "turn_id": "t1"}}),
        )
        .unwrap();
    store
        .begin_idempotency("k-note", "coordinator.send_message", &fingerprint(2))
        .unwrap();
    store
        .complete_idempotency(
            "k-note",
            "coordinator.send_message",
            &fingerprint(2),
            &json!({"status": "success", "body": {"entry_id": "c1"}}),
        )
        .unwrap();

    // Control: before the clear, the text is on disk, so the search below means something.
    for marker in [
        GLOBAL_MESSAGE,
        GLOBAL_REPLY,
        COORDINATOR_NOTE,
        COORDINATOR_PROPOSAL,
    ] {
        assert!(
            !files_containing(directory.path(), marker).is_empty(),
            "{marker} not on disk"
        );
    }

    store
        .clear_conversation_history(&ConversationTarget::Global)
        .unwrap();
    store.clear_coordinator_history("p1").unwrap();

    for marker in [
        GLOBAL_MESSAGE,
        GLOBAL_REPLY,
        COORDINATOR_NOTE,
        COORDINATOR_PROPOSAL,
    ] {
        assert_eq!(
            files_containing(directory.path(), marker),
            Vec::<PathBuf>::new(),
            "{marker} survived the clear"
        );
    }
}

#[test]
fn a_clear_deletes_only_its_conversation_and_records_counts_without_content() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    store.register_project(&project("p1")).unwrap();
    begin_global(&store, "t1", GLOBAL_MESSAGE);
    store
        .finish_conversation_turn("t1", &TurnOutcome::Completed)
        .unwrap();
    store
        .append_coordinator_entry("c1", "p1", "message", "user", COORDINATOR_NOTE, None)
        .unwrap();

    let outcome = store
        .clear_conversation_history(&ConversationTarget::Global)
        .unwrap();
    assert!(outcome.cleared);
    // The message and the turn's terminal marker.
    assert_eq!((outcome.entries_deleted, outcome.turns_deleted), (2, 1));
    assert_eq!(outcome.clear_generation, 1);

    let page = store
        .conversation_page(&ConversationTarget::Global, 0, 100)
        .unwrap();
    assert!(page.entries.is_empty());
    assert_eq!(page.clear_generation, 1);
    assert!(store.conversation_entry("t1:user").unwrap().is_none());
    // The project's coordinator log is another conversation and stays.
    let notes = store.coordinator_entries_after("p1", 0, 100).unwrap();
    assert_eq!(notes.len(), 1);

    let audit = store.conversation_audit().unwrap();
    assert_eq!(audit.len(), 1);
    assert_eq!(audit[0].action, "clear");
    assert_eq!(audit[0].scope, "conversation:global");
    assert_eq!(audit[0].actor, None);
    assert_eq!((audit[0].entries_deleted, audit[0].turns_deleted), (2, 1));
    // The audit row is the only record of the clear, so it must not hold the text.
    assert!(!format!("{audit:?}").contains(GLOBAL_MESSAGE));
}

#[test]
fn a_clear_with_nothing_to_delete_changes_nothing_and_is_not_audited() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    let outcome = store
        .clear_conversation_history(&ConversationTarget::Global)
        .unwrap();
    assert!(!outcome.cleared);
    assert_eq!(outcome.clear_generation, 0);
    assert!(store.conversation_audit().unwrap().is_empty());
}

#[test]
fn a_running_turn_refuses_the_clear_and_nothing_is_deleted() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    begin_global(&store, "t1", GLOBAL_MESSAGE);

    let refused = store.clear_conversation_history(&ConversationTarget::Global);
    assert!(matches!(
        refused,
        Err(ConversationError::TurnRunning { ref turn_id }) if turn_id == "t1"
    ));
    let page = store
        .conversation_page(&ConversationTarget::Global, 0, 100)
        .unwrap();
    assert_eq!(page.entries.len(), 1);
    assert!(store.conversation_audit().unwrap().is_empty());

    store
        .finish_conversation_turn("t1", &TurnOutcome::Completed)
        .unwrap();
    assert!(
        store
            .clear_conversation_history(&ConversationTarget::Global)
            .unwrap()
            .cleared
    );
}

#[test]
fn a_cleared_send_keeps_its_key_spent_with_a_tombstone_fingerprint() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    begin_global(&store, "t1", GLOBAL_MESSAGE);
    store
        .finish_conversation_turn("t1", &TurnOutcome::Completed)
        .unwrap();
    store
        .begin_idempotency("k1", "conversations.send", &fingerprint(9))
        .unwrap();
    // The daemon stores every success as {"status": "success", "body": ...}.
    let reference = json!({"status": "success", "body": {"entry_id": "t1:user", "turn_id": "t1"}});
    store
        .complete_idempotency("k1", "conversations.send", &fingerprint(9), &reference)
        .unwrap();

    store
        .clear_conversation_history(&ConversationTarget::Global)
        .unwrap();

    // The stored reference is returned for the same key, whatever the request text,
    // because the text it would be compared with has been deleted.
    assert_eq!(
        store
            .begin_idempotency("k1", "conversations.send", &fingerprint(9))
            .unwrap(),
        IdempotencyStart::Completed(reference.clone())
    );
    assert_eq!(
        store
            .begin_idempotency("k1", "conversations.send", &fingerprint(10))
            .unwrap(),
        IdempotencyStart::Completed(reference.clone())
    );
    // The fingerprint itself is gone from the row, not just ignored.
    let fingerprint_now = raw_scalar(
        directory.path(),
        "SELECT fingerprint FROM idempotency_records WHERE key = 'k1'",
    );
    assert_eq!(fingerprint_now, "0".repeat(64));
    // A different method on the same key is still a conflict.
    assert!(store
        .begin_idempotency("k1", "conversations.cancel", &fingerprint(9))
        .is_err());
}

#[test]
fn a_removed_coordinator_stays_removed_across_a_reopen_and_restores_empty() {
    let directory = tempfile::tempdir().unwrap();
    {
        let store = open(directory.path());
        store.register_project(&project("p1")).unwrap();
        store.restore_coordinator("p1").unwrap();
        store
            .append_coordinator_entry("c1", "p1", "message", "user", COORDINATOR_NOTE, None)
            .unwrap();
        let change = store.remove_coordinator("p1").unwrap();
        assert!(change.changed && change.removed);
        assert_eq!(change.entries_deleted, 1);
        assert_eq!(change.clear_generation, 1);
        // Idempotent: removing again changes nothing.
        let again = store.remove_coordinator("p1").unwrap();
        assert!(!again.changed && again.removed);
        assert_eq!(again.clear_generation, 1);
    }

    let store = open(directory.path());
    assert!(store.coordinator_scope("p1").unwrap().removed);
    assert_eq!(
        store.projects_without_coordinator().unwrap(),
        vec!["p1".to_owned()]
    );
    assert!(store
        .coordinator_entries_after("p1", 0, 100)
        .unwrap()
        .is_empty());
    assert!(store.clear_coordinator_history("p1").is_err());

    let restored = store.restore_coordinator("p1").unwrap();
    assert!(restored.changed && !restored.removed);
    assert!(!store.coordinator_scope("p1").unwrap().removed);
    assert!(store
        .coordinator_entries_after("p1", 0, 100)
        .unwrap()
        .is_empty());
    assert!(!store.restore_coordinator("p1").unwrap().changed);

    let actions: Vec<String> = store
        .conversation_audit()
        .unwrap()
        .into_iter()
        .map(|record| format!("{}:{}", record.action, record.scope))
        .collect();
    assert_eq!(
        actions,
        vec![
            "restore:coordinator:p1",
            "remove:coordinator:p1",
            "restore:coordinator:p1"
        ]
    );
    // The project and its registration are untouched.
    assert_eq!(store.get_project("p1").unwrap().id, "p1");
}

#[test]
fn a_coordinator_clear_keeps_the_project_and_is_refused_once_removed() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    store.register_project(&project("p1")).unwrap();
    store.restore_coordinator("p1").unwrap();
    store
        .append_coordinator_entry("c1", "p1", "message", "user", COORDINATOR_NOTE, None)
        .unwrap();
    let outcome = store.clear_coordinator_history("p1").unwrap();
    assert!(outcome.cleared);
    assert_eq!(outcome.clear_generation, 1);
    assert_eq!(store.get_project("p1").unwrap().id, "p1");
    assert!(store.coordinator_entry("c1").unwrap().is_none());
    // Nothing is left to clear, so a second clear is a no-op.
    assert!(!store.clear_coordinator_history("p1").unwrap().cleared);
    assert!(store.clear_coordinator_history("missing").is_err());
}

#[test]
fn a_registered_project_has_no_coordinator_until_one_is_added() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    store.register_project(&project("p1")).unwrap();

    let scope = store.coordinator_scope("p1").unwrap();
    assert!(scope.removed);
    assert_eq!(scope.clear_generation, 0);
    assert_eq!(
        store.projects_without_coordinator().unwrap(),
        vec!["p1".to_owned()]
    );
    assert!(store.clear_coordinator_history("p1").is_err());
    let removed = store.remove_coordinator("p1").unwrap();
    assert!(!removed.changed && removed.removed);

    let added = store.restore_coordinator("p1").unwrap();
    assert!(added.changed && !added.removed);
    assert_eq!(added.clear_generation, 0);
    assert!(!store.coordinator_scope("p1").unwrap().removed);
    assert!(store.projects_without_coordinator().unwrap().is_empty());
    let actions: Vec<String> = store
        .conversation_audit()
        .unwrap()
        .into_iter()
        .map(|record| format!("{}:{}", record.action, record.scope))
        .collect();
    assert_eq!(actions, vec!["restore:coordinator:p1"]);
}

#[test]
fn the_opt_in_migration_removes_every_coordinator_that_existed_and_its_notes() {
    const LEGACY_NOTE: &str = "MARKER-PRE-OPT-IN-NOTE-6a1b";
    let directory = tempfile::tempdir().unwrap();
    let path = store_path(directory.path());

    // Build the database as it was before 0053: only the migrations up to 0052.
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
        if version <= 52 {
            fs::copy(&entry, legacy_dir.join(entry.file_name().unwrap())).unwrap();
        }
    }
    let rt = runtime();
    let mut connection = raw_connect(&path);
    rt.block_on(async {
        sqlx::migrate::Migrator::new(legacy_dir.as_path())
            .await
            .unwrap()
            .run(&mut connection)
            .await
            .unwrap();
        // p1 never touched its coordinator (active by default before 0053), p2 cleared
        // it twice, and p3 removed it.
        for statement in [
            "INSERT INTO projects (id, root_path, settings) VALUES \
             ('p1', '/work/p1', '{}'), ('p2', '/work/p2', '{}'), ('p3', '/work/p3', '{}')",
            "INSERT INTO coordinator_scopes (project_id, removed, clear_generation) VALUES \
             ('p2', 0, 2), ('p3', 1, 4)",
            "INSERT INTO coordinator_entries (id, project_id, kind, author, body) VALUES \
             ('c1', 'p1', 'message', 'user', 'MARKER-PRE-OPT-IN-NOTE-6a1b'), \
             ('c2', 'p2', 'message', 'user', 'second note')",
            "INSERT INTO idempotency_records (key, method, fingerprint, result) VALUES \
             ('k-note', 'coordinator.send_message', \
              'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb', \
              '{\"status\":\"success\",\"body\":{\"entry_id\":\"c1\"}}')",
        ] {
            sqlx::query(statement)
                .execute(&mut connection)
                .await
                .unwrap();
        }
    });
    drop(connection);
    assert!(!files_containing(directory.path(), LEGACY_NOTE).is_empty());

    let store = open(directory.path());
    let scopes: Vec<(bool, i64)> = ["p1", "p2", "p3"]
        .iter()
        .map(|id| {
            let scope = store.coordinator_scope(id).unwrap();
            (scope.removed, scope.clear_generation)
        })
        .collect();
    assert_eq!(scopes, vec![(true, 1), (true, 3), (true, 4)]);
    assert_eq!(
        store.projects_without_coordinator().unwrap(),
        vec!["p1".to_owned(), "p2".to_owned(), "p3".to_owned()]
    );
    assert!(store.coordinator_entry("c1").unwrap().is_none());
    assert!(store.coordinator_entry("c2").unwrap().is_none());
    let audit: Vec<(String, String, i64)> = store
        .conversation_audit()
        .unwrap()
        .into_iter()
        .map(|record| (record.action, record.scope, record.entries_deleted))
        .collect();
    assert_eq!(
        audit,
        vec![
            ("remove".to_owned(), "coordinator:p1".to_owned(), 1),
            ("remove".to_owned(), "coordinator:p2".to_owned(), 1),
        ]
    );
    // The deleted note's key stays spent with a tombstone fingerprint.
    assert!(matches!(
        store
            .begin_idempotency("k-note", "coordinator.send_message", &fingerprint(1))
            .unwrap(),
        IdempotencyStart::Completed(_)
    ));
    assert_eq!(
        raw_scalar(
            directory.path(),
            "SELECT CAST(open AS TEXT) FROM conversation_clear_gate"
        ),
        "0"
    );
    assert_eq!(
        files_containing(directory.path(), LEGACY_NOTE),
        Vec::<PathBuf>::new()
    );

    // A project's coordinator can still be added back, empty.
    assert!(store.restore_coordinator("p1").unwrap().changed);
    assert!(store
        .coordinator_entries_after("p1", 0, 100)
        .unwrap()
        .is_empty());
}

#[test]
fn history_rows_are_only_deleted_by_a_clear_and_audit_rows_never() {
    let directory = tempfile::tempdir().unwrap();
    let path = store_path(directory.path());
    let store = open(directory.path());
    store.register_project(&project("p1")).unwrap();
    store.restore_coordinator("p1").unwrap();
    begin_global(&store, "t1", GLOBAL_MESSAGE);
    store
        .finish_conversation_turn("t1", &TurnOutcome::Completed)
        .unwrap();
    store
        .append_coordinator_entry("c1", "p1", "message", "user", COORDINATOR_NOTE, None)
        .unwrap();
    // A second project keeps a coordinator log, so the coordinator delete has a row to hit.
    store.register_project(&project("p2")).unwrap();
    store
        .append_coordinator_entry("c2", "p2", "message", "user", "KEPT-NOTE", None)
        .unwrap();
    store.remove_coordinator("p1").unwrap();
    drop(store);

    assert!(!accepted(
        &path,
        "UPDATE conversation_entries SET body = 'x'"
    ));
    assert!(!accepted(&path, "DELETE FROM conversation_entries"));
    assert!(!accepted(&path, "DELETE FROM conversation_turns"));
    assert!(!accepted(&path, "DELETE FROM coordinator_entries"));
    assert!(!accepted(
        &path,
        "UPDATE conversation_audit SET action = 'clear'"
    ));
    assert!(!accepted(&path, "DELETE FROM conversation_audit"));
}

#[test]
fn a_database_from_before_the_clear_migrations_is_migrated_and_its_replies_scrubbed() {
    let directory = tempfile::tempdir().unwrap();
    let path = store_path(directory.path());

    // Build the database as it was before 0032: only the migrations up to 0031.
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
        if version <= 31 {
            fs::copy(&entry, legacy_dir.join(entry.file_name().unwrap())).unwrap();
        }
    }
    let rt = runtime();
    let mut connection = raw_connect(&path);
    rt.block_on(async {
        sqlx::migrate::Migrator::new(legacy_dir.as_path())
            .await
            .unwrap()
            .run(&mut connection)
            .await
            .unwrap();
        sqlx::query("INSERT INTO projects (id, root_path, settings) VALUES ('p1', '/work/p1', '{}')")
            .execute(&mut connection)
            .await
            .unwrap();
        sqlx::query("INSERT INTO conversations (id, target_kind) VALUES ('conversation:global', 'global')")
            .execute(&mut connection)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO conversation_turns (id, conversation_id, state, runtime_name, runtime_version) \
             VALUES ('t1', 'conversation:global', 'completed', 'test-runtime', '1.0.0')",
        )
        .execute(&mut connection)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO conversation_entries (id, conversation_id, turn_id, kind, author, body) \
             VALUES ('t1:user', 'conversation:global', 't1', 'user_message', 'user', 'LEGACY-MESSAGE')",
        )
        .execute(&mut connection)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO coordinator_entries (id, project_id, kind, author, body) \
             VALUES ('c1', 'p1', 'message', 'user', 'LEGACY-NOTE')",
        )
        .execute(&mut connection)
        .await
        .unwrap();
        let legacy_send = json!({
            "status": "success",
            "body": {
                "authority": "none",
                "target": {"kind": "global"},
                "entry": {"sequence": 1, "id": "t1:user", "turn_id": "t1", "kind": "user_message",
                          "author": "user", "body": "LEGACY-MESSAGE", "error_code": null,
                          "created_at": "2026-11-01 10:00:00"},
                "turn": {"id": "t1", "state": "completed"},
                "epoch": "epoch-1"
            }
        });
        let legacy_note = json!({
            "status": "success",
            "body": {"sequence": 1, "id": "c1", "project_id": "p1", "kind": "message",
                     "author": "user", "body": "LEGACY-NOTE", "validation": null,
                     "created_at": "2026-11-01 10:01:00"}
        });
        let failed = json!({"status": "failure", "body": "conflict"});
        for (key, method, result, fp) in [
            ("k-send", "conversations.send", legacy_send, "a"),
            ("k-note", "coordinator.send_message", legacy_note, "b"),
            ("k-failed", "conversations.send", failed, "c"),
        ] {
            sqlx::query(
                "INSERT INTO idempotency_records (key, method, fingerprint, result) VALUES (?, ?, ?, ?)",
            )
            .bind(key)
            .bind(method)
            .bind(fp.repeat(64))
            .bind(result.to_string())
            .execute(&mut connection)
            .await
            .unwrap();
        }
    });
    drop(connection);

    // Opening the store applies 0032 to 0034 to the existing data.
    let store = open(directory.path());
    let send = store
        .begin_idempotency("k-send", "conversations.send", &"a".repeat(64))
        .unwrap();
    let IdempotencyStart::Completed(send) = send else {
        panic!("send replay should be completed, not {send:?}");
    };
    assert_eq!(send["body"]["entry_id"], "t1:user");
    assert_eq!(send["body"]["turn_id"], "t1");
    assert_eq!(send["body"]["entry_sequence"], 1);
    assert_eq!(send["body"]["epoch"], "epoch-1");
    let note = store
        .begin_idempotency("k-note", "coordinator.send_message", &"b".repeat(64))
        .unwrap();
    let IdempotencyStart::Completed(note) = note else {
        panic!("note replay should be completed");
    };
    assert_eq!(note["body"]["entry_id"], "c1");
    assert_eq!(note["body"]["entry_sequence"], 1);
    // A failed reply carries no body and is kept as it was.
    assert_eq!(
        store
            .begin_idempotency("k-failed", "conversations.send", &"c".repeat(64))
            .unwrap(),
        IdempotencyStart::Completed(json!({"status": "failure", "body": "conflict"}))
    );
    // The conversation message is not cleared, so it still reads back. The coordinator
    // note is gone: 0053 removed every coordinator that existed before they became
    // opt-in.
    assert_eq!(
        store.conversation_entry("t1:user").unwrap().unwrap().body,
        "LEGACY-MESSAGE"
    );
    assert!(store.coordinator_entry("c1").unwrap().is_none());
    drop(store);

    let rt = runtime();
    let mut connection = raw_connect(&path);
    let results: Vec<String> = rt
        .block_on(
            sqlx::query_scalar("SELECT result FROM idempotency_records").fetch_all(&mut connection),
        )
        .unwrap();
    for result in results {
        assert!(
            !result.contains("LEGACY-"),
            "a replay result still holds message text: {result}"
        );
    }
}

#[test]
fn a_migrated_database_reaches_the_clear_path_with_its_pre_existing_rows() {
    let directory = tempfile::tempdir().unwrap();
    {
        let store = open(directory.path());
        begin_global(&store, "t1", GLOBAL_MESSAGE);
        store
            .finish_conversation_turn("t1", &TurnOutcome::Completed)
            .unwrap();
    }
    let store = open(directory.path());
    assert!(
        store
            .clear_conversation_history(&ConversationTarget::Global)
            .unwrap()
            .cleared
    );
    assert!(files_containing(directory.path(), GLOBAL_MESSAGE).is_empty());
    assert_eq!(
        store
            .conversation_page(&ConversationTarget::Global, 0, 10)
            .unwrap()
            .clear_generation,
        1
    );
}

/// An accepted block in `scope`, parsed from the fenced text a reply would carry.
fn accepted_block(scope: BlockScope<'_>, actions: &str) -> TurnBlock {
    let held = format!("```mcao-actions\n{{\"mcao_actions\": 1, \"actions\": [{actions}]}}\n```");
    TurnBlock::Accepted(parse_block(&held, scope).unwrap())
}

#[test]
fn a_clear_deletes_its_proposals_and_keeps_their_events() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    let workstream = ConversationTarget::Workstream("ws-1".to_owned());
    begin_global(&store, "t1", GLOBAL_MESSAGE);
    store
        .complete_conversation_turn("t1", accepted_block(BlockScope::Global, MARKED_CREATE))
        .unwrap();
    store
        .begin_conversation_turn(&workstream, "t2", "hi", "test-runtime", "1.0.0")
        .unwrap();
    store
        .complete_conversation_turn(
            "t2",
            accepted_block(
                BlockScope::Workstream("ws-1"),
                r#"{"kind": "objective.add", "workstream_id": "ws-1", "title": "Plan"}"#,
            ),
        )
        .unwrap();
    let global_id = proposal_id("t1", 0);
    // Control: the proposal's text is on disk before the clear, so the search below means something.
    assert!(!files_containing(directory.path(), ACTION_MARKER).is_empty());

    assert!(
        store
            .clear_conversation_history(&ConversationTarget::Global)
            .unwrap()
            .cleared
    );
    assert!(store.action_proposal(&global_id).unwrap().is_none());
    assert!(store
        .action_proposals(&ConversationTarget::Global, 10)
        .unwrap()
        .is_empty());
    // The other scope's proposal is untouched.
    assert_eq!(store.action_proposals(&workstream, 10).unwrap().len(), 1);
    // The events outlive the clear. They hold digests, never the proposal's text.
    let events = store.action_proposal_events(&global_id).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].to_state, ProposalState::Pending);
    assert_eq!(events[0].digest.len(), 64);
    assert!(files_containing(directory.path(), ACTION_MARKER).is_empty());
}

#[test]
fn action_proposals_are_deleted_only_by_a_clear_and_events_never() {
    let directory = tempfile::tempdir().unwrap();
    let path = store_path(directory.path());
    {
        let store = open(directory.path());
        begin_global(&store, "t1", GLOBAL_MESSAGE);
        store
            .complete_conversation_turn("t1", accepted_block(BlockScope::Global, MARKED_CREATE))
            .unwrap();
    }

    assert!(!accepted(&path, "DELETE FROM action_proposals"));
    assert!(!accepted(
        &path,
        "UPDATE action_proposals SET params_json = '{}'"
    ));
    assert!(!accepted(
        &path,
        "UPDATE action_proposals SET digest = '0000000000000000000000000000000000000000000000000000000000000000'"
    ));
    assert!(!accepted(
        &path,
        "UPDATE action_proposals SET subject = 'x'"
    ));
    assert!(!accepted(&path, "DELETE FROM action_proposal_events"));
    assert!(!accepted(
        &path,
        "UPDATE action_proposal_events SET to_state = 'approved'"
    ));
    assert!(!accepted(
        &path,
        "INSERT INTO action_proposal_events (proposal_id, scope, to_state, actor, digest) \
         VALUES ('p1', 'conversation:global', 'pending', 'bogus', \
         '0000000000000000000000000000000000000000000000000000000000000000')"
    ));
    assert!(!accepted(
        &path,
        "UPDATE conversation_turns SET action_block = 'bogus'"
    ));
    // Only the state may change, and only along the legal edges: pending -> approved,
    // approved -> executed. A skipped edge and any change to a terminal row abort.
    assert!(!accepted(
        &path,
        "UPDATE action_proposals SET state = 'executed'"
    ));
    assert!(accepted(
        &path,
        "UPDATE action_proposals SET state = 'approved'"
    ));
    assert!(!accepted(
        &path,
        "UPDATE action_proposals SET state = 'pending'"
    ));
    assert!(accepted(
        &path,
        "UPDATE action_proposals SET state = 'executed'"
    ));
    assert!(!accepted(
        &path,
        "UPDATE action_proposals SET state = 'failed'"
    ));
    assert!(accepted(
        &path,
        "INSERT INTO action_proposal_events (proposal_id, scope, to_state, actor, digest) \
         VALUES ('p1', 'conversation:global', 'pending', 'grant:g1', \
         '0000000000000000000000000000000000000000000000000000000000000000')"
    ));
}

#[test]
fn a_database_from_0036_is_migrated_with_its_turns_and_no_action_block() {
    let directory = tempfile::tempdir().unwrap();
    let path = store_path(directory.path());

    // Build the database as it was before 0037: only the migrations up to 0036.
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
        if version <= 36 {
            fs::copy(&entry, legacy_dir.join(entry.file_name().unwrap())).unwrap();
        }
    }
    let rt = runtime();
    let mut connection = raw_connect(&path);
    rt.block_on(async {
        sqlx::migrate::Migrator::new(legacy_dir.as_path())
            .await
            .unwrap()
            .run(&mut connection)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO conversations (id, target_kind) VALUES ('conversation:global', 'global')",
        )
        .execute(&mut connection)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO conversation_turns (id, conversation_id, state, runtime_name, runtime_version) \
             VALUES ('t1', 'conversation:global', 'completed', 'test-runtime', '1.0.0')",
        )
        .execute(&mut connection)
        .await
        .unwrap();
    });
    drop(connection);

    let store = open(directory.path());
    assert_eq!(
        store.conversation_turn("t1").unwrap().unwrap().action_block,
        None
    );
    assert_eq!(
        raw_scalar(
            directory.path(),
            "SELECT name FROM sqlite_master WHERE name = 'action_proposal_events'",
        ),
        "action_proposal_events"
    );
    // The migrated database takes a block, and a clear still deletes what it holds.
    begin_global(&store, "t2", GLOBAL_MESSAGE);
    assert_eq!(
        store
            .complete_conversation_turn("t2", accepted_block(BlockScope::Global, MARKED_CREATE))
            .unwrap(),
        Some(vec![proposal_id("t2", 0)])
    );
    assert!(
        store
            .clear_conversation_history(&ConversationTarget::Global)
            .unwrap()
            .cleared
    );
    assert!(store.conversation_turn("t2").unwrap().is_none());
}
