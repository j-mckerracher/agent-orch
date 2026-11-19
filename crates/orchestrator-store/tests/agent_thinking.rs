//! Agent thinking (DPES-1106, migration 0063): chat-turn `thinking_chunk` entries with their own
//! budget, and the capped per-session thinking table.

use std::fs;
use std::path::Path;

use orchestrator_store::{
    ConversationError, ConversationTarget, SqliteStore, TurnOutcome, TurnState,
    MAX_REPLY_BYTES_PER_TURN, MAX_REPLY_CHUNK_BYTES, MAX_THINKING_BYTES_PER_SESSION,
    MAX_THINKING_BYTES_PER_TURN, MAX_THINKING_CHUNK_BYTES,
};
use sqlx::migrate::Migrator;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::Connection;

const MARKER: &str = "[thinking truncated: limit reached]";

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
        .conversation_page(&ConversationTarget::Global, 0, 1000)
        .unwrap()
        .entries
        .into_iter()
        .map(|entry| (entry.kind, entry.body))
        .collect()
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

/// Runs `sql` on the store's file through a second, raw connection.
fn raw(directory: &Path, sql: &'static str) -> Result<(), sqlx::Error> {
    runtime().block_on(async {
        let options = SqliteConnectOptions::new()
            .filename(directory.join("store.sqlite"))
            .foreign_keys(true);
        let mut connection = sqlx::SqliteConnection::connect_with(&options).await?;
        sqlx::query(sql).execute(&mut connection).await.map(|_| ())
    })
}

/// Builds the database as 0062 left it: two completed-turn entries, one deleted entry (so the
/// AUTOINCREMENT counter is ahead of the newest row), and a running turn.
fn database_at_0062(directory: &Path) {
    let legacy_dir = directory.join("legacy-migrations");
    fs::create_dir(&legacy_dir).unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    for entry in fs::read_dir(&source).unwrap() {
        let entry = entry.unwrap().path();
        let name = entry.file_name().unwrap().to_str().unwrap().to_owned();
        let version: u32 = name.split('_').next().unwrap().parse().unwrap();
        if version <= 62 {
            fs::copy(&entry, legacy_dir.join(name)).unwrap();
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
        for sql in [
            "INSERT INTO conversations (id, target_kind) VALUES ('conversation:global', 'global')",
            "INSERT INTO conversation_turns (id, conversation_id, state, runtime_name, runtime_version) \
             VALUES ('t1', 'conversation:global', 'completed', 'rt', '1'), \
                    ('t2', 'conversation:global', 'running', 'rt', '1')",
            "INSERT INTO conversation_entries (id, conversation_id, turn_id, kind, author, body) \
             VALUES ('t1:user', 'conversation:global', 't1', 'user_message', 'user', 'old message'), \
                    ('t1:chunk:1', 'conversation:global', 't1', 'reply_chunk', 'orchestrator', 'old reply'), \
                    ('t1:end', 'conversation:global', 't1', 'turn_completed', 'system', '')",
            "UPDATE conversation_clear_gate SET open = 1",
            "DELETE FROM conversation_entries WHERE id = 't1:end'",
            "UPDATE conversation_clear_gate SET open = 0",
        ] {
            sqlx::query(sql).execute(&mut connection).await.unwrap();
        }
        connection.close().await.unwrap();
    });
}

#[test]
fn migration_0063_keeps_entries_and_the_sequence_counter() {
    let directory = tempfile::tempdir().unwrap();
    database_at_0062(directory.path());
    let store = open(&directory);

    let page = store
        .conversation_page(&ConversationTarget::Global, 0, 100)
        .unwrap();
    let old: Vec<_> = page
        .entries
        .iter()
        .map(|entry| (entry.sequence, entry.kind.as_str(), entry.body.as_str()))
        .collect();
    assert_eq!(
        old,
        [
            (1, "user_message", "old message"),
            (2, "reply_chunk", "old reply")
        ]
    );

    // The deleted row's sequence (3) is not reused, and the new kind is accepted.
    let thought = store.append_thinking_chunk("t2", "hmm").unwrap();
    assert_eq!(thought.sequence, 4);
    let reply = store.append_reply_chunk("t2", "answer").unwrap();
    assert_eq!(reply.sequence, 5);
    assert!(store
        .session_thinking("s", 0, 10)
        .unwrap()
        .chunks
        .is_empty());
}

#[test]
fn thinking_entries_round_trip_but_stay_append_only_and_clearable() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(&directory);
    begin(&store, "t1");
    let entry = store.append_thinking_chunk("t1", "step one").unwrap();
    assert_eq!(
        (
            entry.kind.as_str(),
            entry.author.as_str(),
            entry.id.as_str()
        ),
        ("thinking_chunk", "orchestrator", "t1:think:1")
    );
    store.append_thinking_chunk("t1", "step two").unwrap();
    assert_eq!(
        store.append_thinking_chunk("t1", "step two").unwrap().id,
        "t1:think:3"
    );
    store.append_reply_chunk("t1", "answer").unwrap();
    // Thinking never reaches the reply text the daemon reads back.
    assert_eq!(store.turn_reply_text("t1", 1024).unwrap(), "answer");

    assert!(matches!(
        store.append_thinking_chunk("t1", ""),
        Err(ConversationError::InvalidInput(_))
    ));
    assert!(matches!(
        store.append_thinking_chunk("t1", &"x".repeat(MAX_REPLY_CHUNK_BYTES + 1)),
        Err(ConversationError::ReplyTooLarge)
    ));
    assert!(matches!(
        store.append_thinking_chunk("missing", "x"),
        Err(ConversationError::TurnNotFound)
    ));

    let text = |sql: &'static str| raw(directory.path(), sql).unwrap_err().to_string();
    assert!(
        text("UPDATE conversation_entries SET body = 'x' WHERE id = 't1:think:1'")
            .contains("append-only")
    );
    assert!(
        text("DELETE FROM conversation_entries WHERE id = 't1:think:1'")
            .contains("only by a history clear")
    );
    assert!(text(
        "INSERT INTO conversation_entries (id, conversation_id, turn_id, kind, author, body) \
         SELECT 'bad', conversation_id, id, 'thinking_chunk', 'user', 'x' FROM conversation_turns"
    )
    .contains("CHECK"));

    store
        .finish_conversation_turn("t1", &TurnOutcome::Completed)
        .unwrap();
    assert!(matches!(
        store.append_thinking_chunk("t1", "late"),
        Err(ConversationError::TurnNotRunning(TurnState::Completed))
    ));

    let outcome = store
        .clear_conversation_history(&ConversationTarget::Global)
        .unwrap();
    assert!(outcome.cleared);
    assert!(kinds(&store).is_empty());
}

#[test]
fn thinking_does_not_use_the_reply_budget_and_overflow_marks_once() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(&directory);
    begin(&store, "t1");
    let chunk = "t".repeat(MAX_REPLY_CHUNK_BYTES);
    for _ in 0..MAX_THINKING_BYTES_PER_TURN / MAX_REPLY_CHUNK_BYTES {
        store.append_thinking_chunk("t1", &chunk).unwrap();
    }
    assert_eq!(
        store.conversation_turn("t1").unwrap().unwrap().reply_bytes,
        0
    );

    // Over the cap: one marker entry, then plain errors.
    for _ in 0..3 {
        assert!(matches!(
            store.append_thinking_chunk("t1", "more"),
            Err(ConversationError::ThinkingFull)
        ));
    }
    let markers = kinds(&store)
        .into_iter()
        .filter(|(kind, body)| kind == "thinking_chunk" && body == MARKER)
        .count();
    assert_eq!(markers, 1);

    // The turn still runs and the whole reply budget is still available.
    let reply = "r".repeat(MAX_REPLY_CHUNK_BYTES);
    for _ in 0..MAX_REPLY_BYTES_PER_TURN / MAX_REPLY_CHUNK_BYTES {
        store.append_reply_chunk("t1", &reply).unwrap();
    }
    let turn = store.conversation_turn("t1").unwrap().unwrap();
    assert_eq!(turn.state, TurnState::Running);
    assert_eq!(turn.reply_bytes, MAX_REPLY_BYTES_PER_TURN as u64);
    store
        .finish_conversation_turn("t1", &TurnOutcome::Completed)
        .unwrap()
        .unwrap();
}

#[test]
fn session_thinking_pages_by_cursor() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(&directory);
    store.append_session_thinking("s1", "").unwrap();
    assert!(store
        .session_thinking("s1", 0, 10)
        .unwrap()
        .chunks
        .is_empty());

    store.append_session_thinking("s1", "one").unwrap();
    store.append_session_thinking("other", "not mine").unwrap();
    store.append_session_thinking("s1", "two").unwrap();
    // Two pieces: the cut lands on a character boundary.
    let long = "é".repeat(MAX_THINKING_CHUNK_BYTES / 2 + 5);
    store.append_session_thinking("s1", &long).unwrap();

    let first = store.session_thinking("s1", 0, 2).unwrap();
    let bodies: Vec<_> = first.chunks.iter().map(|c| c.body.as_str()).collect();
    assert_eq!(bodies, ["one", "two"]);
    assert_eq!(first.next_cursor, first.chunks[1].sequence);
    assert!(!first.truncated);

    let rest = store
        .session_thinking("s1", first.next_cursor, 500)
        .unwrap();
    assert_eq!(rest.chunks.len(), 2);
    assert!(rest
        .chunks
        .iter()
        .all(|c| c.body.len() <= MAX_THINKING_CHUNK_BYTES));
    assert_eq!(
        rest.chunks
            .iter()
            .map(|c| c.body.as_str())
            .collect::<String>(),
        long
    );
    assert!(rest.chunks[0].sequence > first.next_cursor);

    let none = store.session_thinking("s1", rest.next_cursor, 10).unwrap();
    assert!(none.chunks.is_empty());
    assert_eq!(none.next_cursor, rest.next_cursor);
    assert_eq!(store.session_thinking("s1", 0, 0).unwrap().chunks.len(), 0);

    assert!(
        raw(directory.path(), "UPDATE session_thinking SET body = 'x'")
            .unwrap_err()
            .to_string()
            .contains("append-only")
    );
}

#[test]
fn session_thinking_writes_one_marker_at_the_cap_then_drops_text() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(&directory);
    let block = "s".repeat(MAX_THINKING_BYTES_PER_SESSION / 8);
    for _ in 0..8 {
        store.append_session_thinking("s1", &block).unwrap();
    }
    assert!(!store.session_thinking("s1", 0, 1).unwrap().truncated);

    store.append_session_thinking("s1", "over").unwrap();
    store.append_session_thinking("s1", "dropped").unwrap();
    store.append_session_thinking("s1", &block).unwrap();

    let mut cursor = 0;
    let mut bodies = Vec::new();
    loop {
        let page = store.session_thinking("s1", cursor, 500).unwrap();
        assert!(page.truncated);
        if page.chunks.is_empty() {
            break;
        }
        cursor = page.next_cursor;
        bodies.extend(page.chunks.into_iter().map(|chunk| chunk.body));
    }
    assert_eq!(bodies.iter().filter(|body| *body == MARKER).count(), 1);
    assert_eq!(bodies.last().unwrap(), MARKER);
    assert!(!bodies.iter().any(|body| body == "dropped"));
    // Another session is unaffected.
    store.append_session_thinking("s2", "fine").unwrap();
    assert!(!store.session_thinking("s2", 0, 10).unwrap().truncated);
}
