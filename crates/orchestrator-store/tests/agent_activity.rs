//! Live agent activity (DPES-1044, migration 0074): `tool_started` and `tool_finished` entries
//! in chat turns and in `session_activity`.

use std::fs;
use std::path::Path;

use orchestrator_store::{
    ConversationTarget, SqliteStore, TurnOutcome, MAX_THINKING_BYTES_PER_SESSION, MAX_TOOL_STARTS,
};
use sqlx::migrate::Migrator;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::Connection;

const MARKER: &str = "[thinking truncated: limit reached]";
const TOOL_MARKER: &str = r#"{"truncated":true}"#;

fn open(directory: &tempfile::TempDir) -> SqliteStore {
    SqliteStore::new(&directory.path().join("store.sqlite")).unwrap()
}

fn begin(store: &SqliteStore, turn: &str) {
    store
        .begin_conversation_turn(
            &ConversationTarget::Global,
            turn,
            "hi",
            "test-runtime",
            "1.0.0",
        )
        .unwrap();
}

fn kinds(store: &SqliteStore) -> Vec<(String, String)> {
    store
        .conversation_page(&ConversationTarget::Global, 0, 5000)
        .unwrap()
        .entries
        .into_iter()
        .map(|entry| (entry.kind, entry.body))
        .collect()
}

fn count(store: &SqliteStore, kind: &str) -> usize {
    kinds(store).iter().filter(|(k, _)| k == kind).count()
}

#[test]
fn thinking_tools_and_reply_keep_sequence_order() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(&directory);
    begin(&store, "t1");
    store.append_thinking_chunk("t1", "plan").unwrap();
    store
        .append_tool_started("t1", "a", "Read", "src/lib.rs")
        .unwrap()
        .unwrap();
    store
        .append_tool_finished("t1", "a", true, "42 lines")
        .unwrap()
        .unwrap();
    store.append_reply_chunk("t1", "done").unwrap();

    let all = kinds(&store);
    let order: Vec<_> = all.iter().map(|(kind, _)| kind.as_str()).collect();
    assert_eq!(
        order,
        [
            "user_message",
            "thinking_chunk",
            "tool_started",
            "tool_finished",
            "reply_chunk"
        ]
    );
    let started: serde_json::Value = serde_json::from_str(&all[2].1).unwrap();
    assert_eq!(
        started,
        serde_json::json!({"id": "a", "name": "Read", "summary": "src/lib.rs"})
    );
    let finished: serde_json::Value = serde_json::from_str(&all[3].1).unwrap();
    assert_eq!(
        finished,
        serde_json::json!({"id": "a", "ok": true, "summary": "42 lines"})
    );
}

#[test]
fn finish_rows_need_a_stored_start_and_store_once() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(&directory);
    begin(&store, "t1");
    assert!(store
        .append_tool_finished("t1", "ghost", true, "")
        .unwrap()
        .is_none());
    store
        .append_tool_started("t1", "a", "Bash", "ls")
        .unwrap()
        .unwrap();
    assert!(store
        .append_tool_started("t1", "a", "Bash", "ls")
        .unwrap()
        .is_none());
    store
        .append_tool_finished("t1", "a", false, "boom")
        .unwrap()
        .unwrap();
    assert!(store
        .append_tool_finished("t1", "a", true, "again")
        .unwrap()
        .is_none());
    assert!(store.append_tool_started("t1", "", "Bash", "ls").is_err());
    assert_eq!(count(&store, "tool_started"), 1);
    assert_eq!(count(&store, "tool_finished"), 1);

    store
        .finish_conversation_turn("t1", &TurnOutcome::Completed)
        .unwrap()
        .unwrap();
    assert!(store.append_tool_started("t1", "b", "Bash", "ls").is_err());
}

#[test]
fn the_start_cap_stores_one_marker_and_drops_later_starts() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(&directory);
    begin(&store, "t1");
    for n in 0..MAX_TOOL_STARTS + 5 {
        store
            .append_tool_started("t1", &format!("id{n}"), "Read", "x")
            .unwrap();
    }
    let bodies: Vec<_> = kinds(&store)
        .into_iter()
        .filter(|(kind, _)| kind == "tool_started")
        .map(|(_, body)| body)
        .collect();
    assert_eq!(bodies.len(), MAX_TOOL_STARTS + 1);
    assert_eq!(bodies.last().unwrap(), TOOL_MARKER);
    assert_eq!(bodies.iter().filter(|body| *body == TOOL_MARKER).count(), 1);

    // A start stored before the cap still finishes; a dropped one does not.
    assert!(store
        .append_tool_finished("t1", "id0", true, "")
        .unwrap()
        .is_some());
    let dropped = format!("id{}", MAX_TOOL_STARTS + 1);
    assert!(store
        .append_tool_finished("t1", &dropped, true, "")
        .unwrap()
        .is_none());
}

#[test]
fn tool_entries_survive_reopen_and_clear() {
    let directory = tempfile::tempdir().unwrap();
    {
        let store = open(&directory);
        begin(&store, "t1");
        store
            .append_tool_started("t1", "a", "Read", "x")
            .unwrap()
            .unwrap();
    }
    let store = open(&directory);
    assert_eq!(count(&store, "tool_started"), 1);
    store
        .finish_conversation_turn("t1", &TurnOutcome::Completed)
        .unwrap()
        .unwrap();
    let outcome = store
        .clear_conversation_history(&ConversationTarget::Global)
        .unwrap();
    assert!(outcome.cleared);
    assert!(kinds(&store).is_empty());
}

#[test]
fn session_activity_pages_every_kind_and_thinking_stays_thinking_only() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(&directory);
    store.append_session_thinking("s1", "one").unwrap();
    assert!(store
        .append_session_tool_started("s1", "a", "Read", "f")
        .unwrap());
    assert!(store
        .append_session_tool_started("other", "a", "Read", "f")
        .unwrap());
    assert!(!store
        .append_session_tool_started("s1", "a", "Read", "f")
        .unwrap());
    assert!(!store
        .append_session_tool_finished("s1", "ghost", true, "")
        .unwrap());
    assert!(store
        .append_session_tool_finished("s1", "a", true, "ok")
        .unwrap());
    assert!(!store
        .append_session_tool_finished("s1", "a", true, "ok")
        .unwrap());
    store.append_session_thinking("s1", "two").unwrap();

    let page = store.session_activity("s1", 0, 100).unwrap();
    let order: Vec<_> = page.entries.iter().map(|e| e.kind.as_str()).collect();
    assert_eq!(
        order,
        ["thinking", "tool_started", "tool_finished", "thinking"]
    );
    assert!(!page.truncated);
    assert!(page
        .entries
        .windows(2)
        .all(|pair| pair[0].sequence < pair[1].sequence));

    let first = store.session_activity("s1", 0, 2).unwrap();
    assert_eq!(first.entries.len(), 2);
    let rest = store
        .session_activity("s1", first.next_cursor, 100)
        .unwrap();
    assert_eq!(rest.entries.len(), 2);
    let none = store.session_activity("s1", rest.next_cursor, 100).unwrap();
    assert!(none.entries.is_empty());
    assert_eq!(none.next_cursor, rest.next_cursor);

    let thinking = store.session_thinking("s1", 0, 100).unwrap();
    let bodies: Vec<_> = thinking.chunks.iter().map(|c| c.body.as_str()).collect();
    assert_eq!(bodies, ["one", "two"]);
}

#[test]
fn session_tool_cap_marks_once_and_thinking_cap_counts_only_thinking() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(&directory);
    for n in 0..MAX_TOOL_STARTS + 3 {
        store
            .append_session_tool_started("s1", &format!("id{n}"), "Read", "x")
            .unwrap();
    }
    let page = store.session_activity("s1", 0, 500).unwrap();
    let starts: Vec<_> = page
        .entries
        .iter()
        .filter(|e| e.kind == "tool_started")
        .collect();
    assert_eq!(starts.len(), MAX_TOOL_STARTS + 1);
    assert_eq!(starts.last().unwrap().body, TOOL_MARKER);
    assert!(page.truncated);
    assert!(!store
        .append_session_tool_finished("s1", &format!("id{}", MAX_TOOL_STARTS + 1), true, "")
        .unwrap());

    // Tool rows take no room from the thinking budget, and the thinking marker still comes once.
    let block = "s".repeat(MAX_THINKING_BYTES_PER_SESSION / 8);
    for _ in 0..8 {
        store.append_session_thinking("s2", &block).unwrap();
        store
            .append_session_tool_started("s2", "x", "Read", &"y".repeat(500))
            .unwrap();
    }
    assert!(!store.session_thinking("s2", 0, 1).unwrap().truncated);
    store.append_session_thinking("s2", "over").unwrap();
    let mut cursor = 0;
    let mut markers = 0;
    loop {
        let page = store.session_thinking("s2", cursor, 500).unwrap();
        if page.chunks.is_empty() {
            break;
        }
        assert!(page.truncated);
        cursor = page.next_cursor;
        markers += page.chunks.iter().filter(|c| c.body == MARKER).count();
    }
    assert_eq!(markers, 1);
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

/// Builds the database as 0073 left it, with thinking rows and chat entries to carry over.
fn database_at_0073(directory: &Path) {
    let legacy_dir = directory.join("legacy-migrations");
    fs::create_dir(&legacy_dir).unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    for entry in fs::read_dir(&source).unwrap() {
        let entry = entry.unwrap().path();
        let name = entry.file_name().unwrap().to_str().unwrap().to_owned();
        let version: u32 = name.split('_').next().unwrap().parse().unwrap();
        if version <= 73 {
            fs::copy(&entry, legacy_dir.join(name)).unwrap();
        }
    }
    runtime().block_on(async {
        let options = SqliteConnectOptions::new()
            .filename(directory.join("store.sqlite"))
            .create_if_missing(true)
            .foreign_keys(true);
        let mut connection = sqlx::SqliteConnection::connect_with(&options).await.unwrap();
        Migrator::new(legacy_dir.as_path()).await.unwrap().run(&mut connection).await.unwrap();
        for sql in [
            "INSERT INTO conversations (id, target_kind) VALUES ('conversation:global', 'global')",
            "INSERT INTO conversation_turns (id, conversation_id, state, runtime_name, runtime_version) \
             VALUES ('t1', 'conversation:global', 'running', 'rt', '1')",
            "INSERT INTO conversation_entries (id, conversation_id, turn_id, kind, author, body) \
             VALUES ('t1:user', 'conversation:global', 't1', 'user_message', 'user', 'hello'), \
                    ('t1:think:1', 'conversation:global', 't1', 'thinking_chunk', 'orchestrator', 'old thought')",
            "INSERT INTO session_thinking (session_id, body) VALUES ('s1', 'first'), ('s1', 'second')",
        ] {
            sqlx::query(sql).execute(&mut connection).await.unwrap();
        }
        connection.close().await.unwrap();
    });
}

#[test]
fn migration_0074_keeps_thinking_rows_and_accepts_tool_rows() {
    let directory = tempfile::tempdir().unwrap();
    database_at_0073(directory.path());
    let store = open(&directory);

    let old = store.session_activity("s1", 0, 100).unwrap();
    let rows: Vec<_> = old
        .entries
        .iter()
        .map(|e| (e.kind.as_str(), e.body.as_str()))
        .collect();
    assert_eq!(rows, [("thinking", "first"), ("thinking", "second")]);
    let chunks = store.session_thinking("s1", 0, 100).unwrap().chunks;
    assert_eq!(chunks.len(), 2);
    assert_eq!(count(&store, "thinking_chunk"), 1);

    store
        .append_tool_started("t1", "a", "Read", "x")
        .unwrap()
        .unwrap();
    assert!(store
        .append_session_tool_started("s1", "a", "Read", "x")
        .unwrap());
    let page = store.session_activity("s1", 0, 100).unwrap();
    assert_eq!(page.entries.len(), 3);
    assert_eq!(page.entries[2].kind, "tool_started");
}

#[test]
fn history_entries_skip_thinking_and_tool_rows_so_a_tool_heavy_turn_cannot_crowd_them_out() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(&directory);
    begin(&store, "t1");
    store.append_reply_chunk("t1", "one").unwrap();
    store
        .finish_conversation_turn("t1", &TurnOutcome::Completed)
        .unwrap()
        .unwrap();
    begin(&store, "t2");
    store.append_thinking_chunk("t2", "hmm").unwrap();
    for n in 0..MAX_TOOL_STARTS {
        let id = format!("id{n}");
        store.append_tool_started("t2", &id, "Read", "x").unwrap();
        store.append_tool_finished("t2", &id, true, "ok").unwrap();
    }
    store.append_reply_chunk("t2", "two").unwrap();
    store
        .finish_conversation_turn("t2", &TurnOutcome::Completed)
        .unwrap()
        .unwrap();

    let entries = store
        .conversation_history_entries(&ConversationTarget::Global, 400)
        .unwrap();
    let order: Vec<_> = entries
        .iter()
        .map(|e| (e.turn_id.as_str(), e.kind.as_str()))
        .collect();
    assert_eq!(
        order,
        [
            ("t1", "user_message"),
            ("t1", "reply_chunk"),
            ("t1", "turn_completed"),
            ("t2", "user_message"),
            ("t2", "reply_chunk"),
            ("t2", "turn_completed"),
        ]
    );
    let newest = store
        .conversation_history_entries(&ConversationTarget::Global, 2)
        .unwrap();
    assert_eq!(
        newest.iter().map(|e| e.kind.as_str()).collect::<Vec<_>>(),
        ["reply_chunk", "turn_completed"]
    );
}

#[test]
fn a_thinking_row_whose_body_is_not_json_never_breaks_tool_matching() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(&directory);
    store
        .append_session_thinking("s1", "plain {not json")
        .unwrap();
    assert!(store
        .append_session_tool_started("s1", "a", "Read", "x")
        .unwrap());
    assert!(store
        .append_session_tool_finished("s1", "a", true, "ok")
        .unwrap());
    assert!(!store
        .append_session_tool_finished("s1", "a", true, "ok")
        .unwrap());
}
