use orchestrator_store::{ConversationError, ConversationTarget, SqliteStore};

fn open(directory: &tempfile::TempDir) -> SqliteStore {
    SqliteStore::new(&directory.path().join("store.sqlite")).unwrap()
}

#[test]
fn a_turn_starts_unmarked_and_is_marked_once_and_for_good() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(&directory);
    store
        .begin_conversation_turn(
            &ConversationTarget::Global,
            "t1",
            "hello",
            "runtime",
            "1.0.0",
        )
        .unwrap();
    assert!(!store.turn_read_report("t1").unwrap());
    store.mark_turn_read_report("t1").unwrap();
    store.mark_turn_read_report("t1").unwrap();
    assert!(store.turn_read_report("t1").unwrap());
}

#[test]
fn marking_a_missing_turn_fails_and_reading_one_says_no() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(&directory);
    assert!(matches!(
        store.mark_turn_read_report("missing"),
        Err(ConversationError::TurnNotFound)
    ));
    assert!(!store.turn_read_report("missing").unwrap());
}
