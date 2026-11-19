//! Preferences and learnings (DPES-1108 phase 6, migrations 0067 and 0068).
//!
//! The store's public API does the reads and writes; a second raw connection tries the writes
//! the database itself must refuse.

use std::fs;
use std::path::Path;

use orchestrator_core::memory::{MemoryKind, MemoryScope, OverBudget};
use orchestrator_store::{
    ConversationTarget, MemoryRestoreOutcome, MemoryWriteOutcome, MemoryWriter, SqliteStore,
};
use sqlx::migrate::Migrator;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::Connection;

const PREFERENCE: MemoryKind = MemoryKind::Preference;
const LEARNING: MemoryKind = MemoryKind::Learning;
const USER: MemoryWriter = MemoryWriter::User;
const AGENT: MemoryWriter = MemoryWriter::Orchestrator;

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

/// Runs one statement on a second connection and reports whether the database took it.
fn try_sql(path: &Path, sql: &str) -> Result<(), sqlx::Error> {
    runtime().block_on(async {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .foreign_keys(true);
        let mut connection = sqlx::SqliteConnection::connect_with(&options).await?;
        // Test-only helper: every statement passed in is written in this file.
        sqlx::query(sqlx::AssertSqlSafe(sql.to_owned()))
            .execute(&mut connection)
            .await?;
        Ok::<(), sqlx::Error>(())
    })
}

fn scalars(path: &Path, statement: &str) -> Vec<String> {
    runtime().block_on(async {
        let options = SqliteConnectOptions::new().filename(path);
        let mut connection = sqlx::SqliteConnection::connect_with(&options)
            .await
            .unwrap();
        sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(statement.to_owned()))
            .fetch_all(&mut connection)
            .await
            .unwrap()
    })
}

fn open() -> (tempfile::TempDir, SqliteStore) {
    let directory = tempfile::tempdir().unwrap();
    let store = SqliteStore::new(&directory.path().join("store.sqlite")).unwrap();
    (directory, store)
}

fn keys_and_texts(store: &SqliteStore, kind: MemoryKind, scope: &MemoryScope) -> Vec<String> {
    store
        .memory_entries(kind, scope)
        .unwrap()
        .into_iter()
        .map(|entry| format!("{}={}", entry.key, entry.text))
        .collect()
}

#[test]
fn a_write_to_a_known_key_updates_the_entry_in_place() {
    let (_directory, store) = open();
    let global = MemoryScope::Global;
    let set = |key: &str, text: &str, now: u64| {
        store
            .set_memory_entry(PREFERENCE, &global, key, text, USER, now)
            .unwrap()
    };
    assert_eq!(
        set("pr-size", "Small pull requests.", 10),
        MemoryWriteOutcome::Created
    );
    assert_eq!(
        set("tests", "Run the tests first.", 20),
        MemoryWriteOutcome::Created
    );
    assert_eq!(
        set("pr-size", "Pull requests under 400 lines.", 30),
        MemoryWriteOutcome::Updated
    );
    // Most recently confirmed first, and still one entry per key.
    assert_eq!(
        keys_and_texts(&store, PREFERENCE, &global),
        [
            "pr-size=Pull requests under 400 lines.",
            "tests=Run the tests first."
        ]
    );
    assert_eq!(
        set("tests", "Run the tests first.", 40),
        MemoryWriteOutcome::Confirmed
    );
    let entries = store.memory_entries(PREFERENCE, &global).unwrap();
    assert_eq!(entries[0].key, "tests");
    assert_eq!(entries[0].created_at_ms, 20);
    assert_eq!(entries[0].confirmed_at_ms, 40);
    // The replaced text is archived, so nothing is lost; a confirm archives nothing.
    let archive = store.memory_archive(PREFERENCE, &global).unwrap();
    assert_eq!(archive.len(), 1);
    assert_eq!(archive[0].text, "Small pull requests.");
    assert_eq!(archive[0].reason, "replaced");
    assert!(!archive[0].restored);
    // Another kind or scope is another document.
    assert!(store.memory_entries(LEARNING, &global).unwrap().is_empty());
    let ws = MemoryScope::Workstream("w1".to_owned());
    assert!(store.memory_entries(PREFERENCE, &ws).unwrap().is_empty());
}

#[test]
fn a_write_past_the_budget_is_refused_with_the_size_and_writes_nothing() {
    let (_directory, store) = open();
    let scope = MemoryScope::Project("/work/p1".to_owned());
    // Four learnings of 3 + 250 bytes fill 1,012 of the 1,024-byte budget.
    let text = "word ".repeat(50);
    for index in 0..4 {
        store
            .set_memory_entry(LEARNING, &scope, &format!("l-{index}"), &text, AGENT, 1)
            .unwrap();
    }
    let refused = store
        .set_memory_entry(LEARNING, &scope, "l-4", "ten bytes!", AGENT, 2)
        .unwrap();
    assert_eq!(
        refused,
        MemoryWriteOutcome::OverBudget(OverBudget {
            bytes: 1012 + 3 + 10,
            budget: 1024
        })
    );
    assert_eq!(store.memory_entries(LEARNING, &scope).unwrap().len(), 4);
    // An in-place update that does not grow the document still fits.
    assert_eq!(
        store
            .set_memory_entry(LEARNING, &scope, "l-0", &"draw ".repeat(50), AGENT, 3)
            .unwrap(),
        MemoryWriteOutcome::Updated
    );
}

#[test]
fn a_refused_text_or_key_writes_nothing() {
    let (_directory, store) = open();
    let global = MemoryScope::Global;
    for (key, text) in [
        ("ok", ""),
        ("ok", "two\nlines"),
        ("ok", "the repo is at /Users/someone/code"),
        ("Bad Key", "fine"),
        ("ok", &"x ".repeat(141)),
    ] {
        assert!(
            store
                .set_memory_entry(PREFERENCE, &global, key, text, USER, 1)
                .is_err(),
            "{key} {text}"
        );
    }
    assert!(store
        .memory_entries(PREFERENCE, &global)
        .unwrap()
        .is_empty());
}

#[test]
fn a_removed_entry_is_archived_and_can_be_restored_once() {
    let (directory, store) = open();
    let ws = MemoryScope::Workstream("w1".to_owned());
    store
        .set_memory_entry(PREFERENCE, &ws, "freeze", "Freeze starts Monday.", USER, 5)
        .unwrap();
    assert!(store
        .remove_memory_entry(PREFERENCE, &ws, "freeze", USER, 6)
        .unwrap());
    assert!(!store
        .remove_memory_entry(PREFERENCE, &ws, "freeze", USER, 7)
        .unwrap());
    assert!(store.memory_entries(PREFERENCE, &ws).unwrap().is_empty());
    let archive = store.memory_archive(PREFERENCE, &ws).unwrap();
    assert_eq!(archive.len(), 1);
    assert_eq!(archive[0].reason, "removed");
    let id = archive[0].id;

    // The key is in use again: the restore waits until the user clears it.
    store
        .set_memory_entry(PREFERENCE, &ws, "freeze", "Freeze moved.", USER, 8)
        .unwrap();
    assert_eq!(
        store.restore_memory_entry(id, 9).unwrap().0,
        MemoryRestoreOutcome::KeyInUse
    );
    store
        .remove_memory_entry(PREFERENCE, &ws, "freeze", USER, 10)
        .unwrap();
    let (outcome, document) = store.restore_memory_entry(id, 11).unwrap();
    assert_eq!(outcome, MemoryRestoreOutcome::Restored);
    assert_eq!(document, Some((PREFERENCE, ws.clone())));
    let entries = store.memory_entries(PREFERENCE, &ws).unwrap();
    assert_eq!(entries[0].text, "Freeze starts Monday.");
    assert_eq!(entries[0].created_at_ms, 5);
    assert_eq!(entries[0].confirmed_at_ms, 11);
    assert_eq!(
        store.restore_memory_entry(id, 12).unwrap().0,
        MemoryRestoreOutcome::AlreadyRestored
    );
    assert_eq!(
        store.restore_memory_entry(9999, 12).unwrap().0,
        MemoryRestoreOutcome::NotFound
    );

    // The archive is append-only: a row is never deleted, and only its restore is recorded.
    let path = directory.path().join("store.sqlite");
    assert!(try_sql(&path, "DELETE FROM memory_archive").is_err());
    assert!(try_sql(&path, "UPDATE memory_archive SET text = 'planted'").is_err());
    assert!(try_sql(&path, "UPDATE memory_archive SET restored_at_ms = 1").is_err());
    assert!(try_sql(
        &path,
        "INSERT INTO memory_entries (kind, scope, key, text, written_by, created_at_ms, \
         confirmed_at_ms) VALUES ('note', 'global', 'k', 'x', 'user', 1, 1)"
    )
    .is_err());
    assert!(try_sql(
        &path,
        "INSERT INTO memory_entries (kind, scope, key, text, written_by, created_at_ms, \
         confirmed_at_ms) VALUES ('preference', 'elsewhere', 'k', 'x', 'user', 1, 1)"
    )
    .is_err());
}

#[test]
fn a_conversation_reads_the_shared_preferences_and_only_its_own_documents() {
    let (_directory, store) = open();
    let global = MemoryScope::Global;
    let w1 = MemoryScope::Workstream("w1".to_owned());
    let p1 = MemoryScope::Project("/work/p1".to_owned());
    let set = |kind, scope: &MemoryScope, key: &str| {
        store
            .set_memory_entry(kind, scope, key, "A fact.", USER, 1)
            .unwrap();
    };
    set(PREFERENCE, &global, "shared");
    set(LEARNING, &global, "global-learning");
    set(PREFERENCE, &w1, "w1-pref");
    set(LEARNING, &w1, "w1-learning");
    set(PREFERENCE, &p1, "p1-pref");
    set(
        LEARNING,
        &MemoryScope::Workstream("w2".to_owned()),
        "w2-learning",
    );
    let keys = |entries: Vec<orchestrator_store::MemoryEntry>| -> Vec<String> {
        entries.into_iter().map(|entry| entry.key).collect()
    };

    let memory = store
        .conversation_memory(&ConversationTarget::Global)
        .unwrap();
    assert_eq!(keys(memory.shared), ["shared"]);
    assert!(memory.local.is_empty());
    assert_eq!(keys(memory.learnings), ["global-learning"]);

    let memory = store
        .conversation_memory(&ConversationTarget::Workstream("w1".to_owned()))
        .unwrap();
    assert_eq!(keys(memory.shared), ["shared"]);
    assert_eq!(keys(memory.local), ["w1-pref"]);
    assert_eq!(keys(memory.learnings), ["w1-learning"]);

    let memory = store
        .conversation_memory(&ConversationTarget::Project("/work/p1".to_owned()))
        .unwrap();
    assert_eq!(keys(memory.shared), ["shared"]);
    assert_eq!(keys(memory.local), ["p1-pref"]);
    assert!(memory.learnings.is_empty());
}

#[test]
fn a_history_clear_keeps_the_conversations_preferences_and_learnings() {
    let (_directory, store) = open();
    let w1 = MemoryScope::Workstream("w1".to_owned());
    store
        .set_memory_entry(PREFERENCE, &w1, "keep", "Kept across a clear.", USER, 1)
        .unwrap();
    store
        .set_memory_entry(PREFERENCE, &MemoryScope::Global, "keep", "Shared.", USER, 1)
        .unwrap();
    store
        .clear_conversation_history(&ConversationTarget::Workstream("w1".to_owned()))
        .unwrap();
    store
        .clear_conversation_history(&ConversationTarget::Global)
        .unwrap();
    assert_eq!(store.memory_entries(PREFERENCE, &w1).unwrap().len(), 1);
    assert_eq!(
        store
            .memory_entries(PREFERENCE, &MemoryScope::Global)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn migration_0067_moves_every_note_into_the_preferences_of_its_scope() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("store.sqlite");
    let legacy_dir = directory.path().join("legacy-migrations");
    fs::create_dir(&legacy_dir).unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    for entry in fs::read_dir(&source).unwrap() {
        let entry = entry.unwrap().path();
        let name = entry.file_name().unwrap().to_str().unwrap().to_owned();
        let version: u32 = name.split('_').next().unwrap().parse().unwrap();
        if version < 67 {
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
    // Ten full 200-byte notes in each scope, as many as 0050 allowed: 2,061 bytes once keyed,
    // over the 2,048-byte budget, and copied anyway. A note the Rust rules would refuse now
    // (a tab) is copied too: the migration never fails on a stored row.
    let full = "n".repeat(200);
    for scope in ["global", "workstream:w1"] {
        for position in 0..10 {
            try_sql(
                &path,
                &format!(
                    "INSERT INTO orchestrator_notes (scope, position, body, updated_at) \
                     VALUES ('{scope}', {position}, '{full}', '2026-11-01 12:00:00')"
                ),
            )
            .unwrap();
        }
    }
    try_sql(
        &path,
        "INSERT INTO orchestrator_notes (scope, position, body, updated_at) \
         VALUES ('workstream:w2', 0, 'tab' || char(9) || 'note', 'not a date')",
    )
    .unwrap();
    for statement in [
        "INSERT INTO conversations (id, target_kind) VALUES ('conversation:global', 'global')",
        "INSERT INTO conversation_turns (id, conversation_id, state, runtime_name, runtime_version) \
         VALUES ('turn-1', 'conversation:global', 'completed', 'test-runtime', '1.0.0')",
        "INSERT INTO action_proposals (id, scope, turn_id, idx, kind, params_json, digest, subject) \
         VALUES ('pr1', 'conversation:global', 'turn-1', 0, 'task.message', '{}', \
                 '0000000000000000000000000000000000000000000000000000000000000001', 's')",
    ] {
        try_sql(&path, statement).unwrap();
    }
    let proposals = "SELECT json_array(rowid, id, kind, state, grant_tried) FROM action_proposals \
                     ORDER BY rowid";
    let before = scalars(&path, proposals);
    let new_kind = "INSERT INTO action_proposals \
         (id, scope, turn_id, idx, kind, params_json, digest, subject) \
         VALUES ('pr2', 'conversation:global', 'turn-1', 1, 'memory.set', '{}', \
                 '0000000000000000000000000000000000000000000000000000000000000002', 's')";
    assert!(try_sql(&path, new_kind).is_err());

    let store = SqliteStore::new(&path).unwrap();
    let global = store
        .memory_entries(PREFERENCE, &MemoryScope::Global)
        .unwrap();
    assert_eq!(global.len(), 10);
    let mut keys: Vec<String> = global.iter().map(|entry| entry.key.clone()).collect();
    keys.sort();
    let mut expected: Vec<String> = (1..=10).map(|n| format!("note-{n}")).collect();
    expected.sort();
    assert_eq!(keys, expected);
    assert!(global.iter().all(|entry| entry.text == full
        && entry.written_by == USER
        && entry.confirmed_at_ms == 1_793_534_400_000));
    let w1 = MemoryScope::Workstream("w1".to_owned());
    assert_eq!(store.memory_entries(PREFERENCE, &w1).unwrap().len(), 10);
    let w2 = store
        .memory_entries(PREFERENCE, &MemoryScope::Workstream("w2".to_owned()))
        .unwrap();
    assert_eq!(w2.len(), 1);
    assert!(
        w2[0].confirmed_at_ms > 1_793_534_400_000,
        "an unreadable date reads as now"
    );
    // The migrated document is over budget: it may shrink, never grow.
    assert!(matches!(
        store
            .set_memory_entry(PREFERENCE, &MemoryScope::Global, "new", "more", USER, 1)
            .unwrap(),
        MemoryWriteOutcome::OverBudget(_)
    ));
    assert_eq!(
        store
            .set_memory_entry(PREFERENCE, &MemoryScope::Global, "note-1", "short", USER, 1)
            .unwrap(),
        MemoryWriteOutcome::Updated
    );
    // The notes table stays, unread, as the source of the copy.
    assert_eq!(
        scalars(
            &path,
            "SELECT CAST(COUNT(*) AS TEXT) FROM orchestrator_notes"
        ),
        ["21"]
    );

    // 0068: every proposal kept, and memory.set now allowed.
    assert_eq!(scalars(&path, proposals), before);
    try_sql(&path, new_kind).unwrap();
    assert!(try_sql(&path, "DELETE FROM action_proposals WHERE id = 'pr1'").is_err());
    assert!(try_sql(&path, "UPDATE action_proposals SET kind = 'task.retry'").is_err());
    assert!(try_sql(
        &path,
        "INSERT INTO decision_holds (id, subject_kind, subject_id, until_date, until_ms, set_by) \
         VALUES ('h2', 'proposal', 'missing', '2026-11-18', 1795046399999, 'user')"
    )
    .is_err());
    try_sql(
        &path,
        "INSERT INTO decision_holds (id, subject_kind, subject_id, until_date, until_ms, set_by) \
         VALUES ('h2', 'proposal', 'pr1', '2026-11-18', 1795046399999, 'user')",
    )
    .unwrap();
}
