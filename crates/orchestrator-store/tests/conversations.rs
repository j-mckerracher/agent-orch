use orchestrator_store::{
    ConversationError, ConversationTarget, SqliteStore, TurnOutcome, TurnState,
    INTERRUPTED_BY_RESTART, MAX_CONVERSATION_MESSAGE_BYTES, MAX_REPLY_BYTES_PER_TURN,
    MAX_REPLY_CHUNK_BYTES,
};

fn open(directory: &tempfile::TempDir) -> SqliteStore {
    SqliteStore::new(&directory.path().join("store.sqlite")).unwrap()
}

fn begin(
    store: &SqliteStore,
    target: &ConversationTarget,
    turn: &str,
    text: &str,
) -> Result<orchestrator_store::BegunTurn, ConversationError> {
    store.begin_conversation_turn(target, turn, text, "test-runtime", "1.0.0")
}

#[test]
fn messages_and_replies_are_ordered_and_correlated_by_turn() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(&directory);
    let target = ConversationTarget::Global;
    begin(&store, &target, "t1", "hello").unwrap();
    store.append_reply_chunk("t1", "one ").unwrap();
    store.append_reply_chunk("t1", "two").unwrap();
    store
        .finish_conversation_turn("t1", &TurnOutcome::Completed)
        .unwrap()
        .unwrap();
    begin(&store, &target, "t2", "again").unwrap();

    let page = store.conversation_page(&target, 0, 100).unwrap();
    let kinds: Vec<_> = page
        .entries
        .iter()
        .map(|entry| (entry.turn_id.as_str(), entry.kind.as_str()))
        .collect();
    assert_eq!(
        kinds,
        [
            ("t1", "user_message"),
            ("t1", "reply_chunk"),
            ("t1", "reply_chunk"),
            ("t1", "turn_completed"),
            ("t2", "user_message"),
        ]
    );
    assert!(page
        .entries
        .windows(2)
        .all(|w| w[0].sequence < w[1].sequence));
    assert_eq!(page.latest_sequence, page.entries[4].sequence);
    assert_eq!(page.latest_turn.unwrap().id, "t2");
}

#[test]
fn repeating_a_turn_id_returns_the_stored_turn_without_new_entries() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(&directory);
    let target = ConversationTarget::Global;
    let first = begin(&store, &target, "t1", "hello").unwrap();
    assert!(first.created);
    let again = begin(&store, &target, "t1", "hello").unwrap();
    assert!(!again.created);
    assert_eq!(again.user_entry, first.user_entry);
    assert_eq!(
        store
            .conversation_page(&target, 0, 100)
            .unwrap()
            .entries
            .len(),
        1
    );

    let other = ConversationTarget::Workstream("w1".into());
    assert!(matches!(
        begin(&store, &other, "t1", "hello"),
        Err(ConversationError::InvalidInput(_))
    ));
}

#[test]
fn a_second_message_is_refused_while_a_turn_runs() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(&directory);
    let target = ConversationTarget::Global;
    begin(&store, &target, "t1", "hello").unwrap();
    let refused = begin(&store, &target, "t2", "overlap");
    assert!(matches!(
        refused,
        Err(ConversationError::TurnRunning { ref turn_id }) if turn_id == "t1"
    ));
    assert_eq!(
        store
            .conversation_page(&target, 0, 100)
            .unwrap()
            .entries
            .len(),
        1
    );
    assert!(store.conversation_turn("t2").unwrap().is_none());
}

#[test]
fn targets_are_independent_conversations() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(&directory);
    let global = ConversationTarget::Global;
    let w1 = ConversationTarget::Workstream("w1".into());
    let w2 = ConversationTarget::Workstream("w2".into());
    begin(&store, &global, "g1", "global").unwrap();
    begin(&store, &w1, "a1", "one").unwrap();
    begin(&store, &w2, "b1", "two").unwrap();
    for (target, body) in [(&global, "global"), (&w1, "one"), (&w2, "two")] {
        let page = store.conversation_page(target, 0, 100).unwrap();
        assert_eq!(page.entries.len(), 1);
        assert_eq!(page.entries[0].body, body);
    }
}

#[test]
fn the_first_terminal_state_wins_and_late_output_is_refused() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(&directory);
    let target = ConversationTarget::Global;
    begin(&store, &target, "t1", "hello").unwrap();
    store
        .finish_conversation_turn("t1", &TurnOutcome::Cancelled)
        .unwrap()
        .unwrap();
    assert!(store
        .finish_conversation_turn("t1", &TurnOutcome::Completed)
        .unwrap()
        .is_none());
    assert!(store
        .finish_conversation_turn("t1", &TurnOutcome::Failed("late".into()))
        .unwrap()
        .is_none());
    assert!(matches!(
        store.append_reply_chunk("t1", "late"),
        Err(ConversationError::TurnNotRunning(TurnState::Cancelled))
    ));
    assert!(matches!(
        store.finish_conversation_turn("missing", &TurnOutcome::Completed),
        Err(ConversationError::TurnNotFound)
    ));
    let page = store.conversation_page(&target, 0, 100).unwrap();
    assert_eq!(page.entries.len(), 2);
    assert_eq!(page.entries[1].kind, "turn_cancelled");
    assert_eq!(page.latest_turn.unwrap().state, TurnState::Cancelled);
}

#[test]
fn failures_carry_a_bounded_code_and_a_new_message_can_follow() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(&directory);
    let target = ConversationTarget::Global;
    begin(&store, &target, "t1", "hello").unwrap();
    assert!(matches!(
        store.finish_conversation_turn("t1", &TurnOutcome::Failed(String::new())),
        Err(ConversationError::InvalidInput(_))
    ));
    assert!(matches!(
        store.finish_conversation_turn("t1", &TurnOutcome::Failed("x".repeat(65))),
        Err(ConversationError::InvalidInput(_))
    ));
    let entry = store
        .finish_conversation_turn("t1", &TurnOutcome::Failed("credentials_missing".into()))
        .unwrap()
        .unwrap();
    assert_eq!(entry.error_code.as_deref(), Some("credentials_missing"));
    let turn = store.conversation_turn("t1").unwrap().unwrap();
    assert_eq!(turn.state, TurnState::Failed);
    assert_eq!(turn.error_code.as_deref(), Some("credentials_missing"));
    begin(&store, &target, "t2", "retry").unwrap();
}

#[test]
fn input_and_reply_bounds_are_enforced() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(&directory);
    let target = ConversationTarget::Global;
    for text in ["", "   ", &"x".repeat(MAX_CONVERSATION_MESSAGE_BYTES + 1)] {
        assert!(matches!(
            begin(&store, &target, "t0", text),
            Err(ConversationError::InvalidInput(_))
        ));
    }
    for id in ["", &"w".repeat(129), "w\n1"] {
        assert!(matches!(
            begin(
                &store,
                &ConversationTarget::Workstream(id.into()),
                "t0",
                "hi"
            ),
            Err(ConversationError::InvalidInput(_))
        ));
    }
    begin(&store, &target, "t1", "hello").unwrap();
    assert!(matches!(
        store.append_reply_chunk("t1", ""),
        Err(ConversationError::InvalidInput(_))
    ));
    assert!(matches!(
        store.append_reply_chunk("t1", &"x".repeat(MAX_REPLY_CHUNK_BYTES + 1)),
        Err(ConversationError::ReplyTooLarge)
    ));
    let chunk = "x".repeat(MAX_REPLY_CHUNK_BYTES);
    for _ in 0..MAX_REPLY_BYTES_PER_TURN / MAX_REPLY_CHUNK_BYTES {
        store.append_reply_chunk("t1", &chunk).unwrap();
    }
    assert!(matches!(
        store.append_reply_chunk("t1", "y"),
        Err(ConversationError::ReplyTooLarge)
    ));
    assert_eq!(
        store.conversation_turn("t1").unwrap().unwrap().reply_bytes,
        MAX_REPLY_BYTES_PER_TURN as u64
    );
}

#[test]
fn replay_returns_only_entries_after_the_cursor_and_survives_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let target = ConversationTarget::Global;
    let cursor = {
        let store = open(&directory);
        begin(&store, &target, "t1", "hello").unwrap();
        store.append_reply_chunk("t1", "a").unwrap();
        let seen = store
            .conversation_page(&target, 0, 100)
            .unwrap()
            .latest_sequence;
        store.append_reply_chunk("t1", "b").unwrap();
        store
            .finish_conversation_turn("t1", &TurnOutcome::Completed)
            .unwrap();
        seen
    };
    let store = open(&directory);
    let page = store.conversation_page(&target, cursor, 100).unwrap();
    let bodies: Vec<_> = page.entries.iter().map(|e| e.kind.as_str()).collect();
    assert_eq!(bodies, ["reply_chunk", "turn_completed"]);
    assert_eq!(page.entries[0].body, "b");

    let limited = store.conversation_page(&target, 0, 2).unwrap();
    assert_eq!(limited.entries.len(), 2);
    assert_eq!(limited.latest_sequence, page.latest_sequence);
    assert!(matches!(
        store.conversation_page(&target, -1, 10),
        Err(ConversationError::Store(_))
    ));
}

#[test]
fn an_unused_target_reads_as_an_empty_transcript() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(&directory);
    let page = store
        .conversation_page(&ConversationTarget::Workstream("w9".into()), 0, 10)
        .unwrap();
    assert!(page.conversation_id.is_none());
    assert!(page.entries.is_empty());
    assert_eq!(page.latest_sequence, 0);
    assert!(page.latest_turn.is_none());
}

#[test]
fn restart_fails_open_turns_and_keeps_the_transcript() {
    let directory = tempfile::tempdir().unwrap();
    let target = ConversationTarget::Global;
    {
        let store = open(&directory);
        begin(&store, &target, "t1", "hello").unwrap();
        store.append_reply_chunk("t1", "partial").unwrap();
    }
    let store = open(&directory);
    assert_eq!(
        store.interrupt_open_conversation_turns().unwrap(),
        ["t1".to_owned()]
    );
    assert!(store
        .interrupt_open_conversation_turns()
        .unwrap()
        .is_empty());
    let page = store.conversation_page(&target, 0, 100).unwrap();
    let kinds: Vec<_> = page.entries.iter().map(|e| e.kind.as_str()).collect();
    assert_eq!(kinds, ["user_message", "reply_chunk", "turn_failed"]);
    assert_eq!(
        page.entries[2].error_code.as_deref(),
        Some(INTERRUPTED_BY_RESTART)
    );
    assert!(matches!(
        store.append_reply_chunk("t1", "after restart"),
        Err(ConversationError::TurnNotRunning(TurnState::Failed))
    ));
    begin(&store, &target, "t2", "next").unwrap();
}

#[test]
fn the_transcript_is_append_only_at_the_database_level() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("store.sqlite");
    {
        let store = SqliteStore::new(&path).unwrap();
        begin(&store, &ConversationTarget::Global, "t1", "hello").unwrap();
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        use sqlx::Connection;
        let mut connection =
            sqlx::SqliteConnection::connect(&format!("sqlite://{}", path.display()))
                .await
                .unwrap();
        assert!(sqlx::query("UPDATE conversation_entries SET body = 'x'")
            .execute(&mut connection)
            .await
            .is_err());
        assert!(sqlx::query("DELETE FROM conversation_entries")
            .execute(&mut connection)
            .await
            .is_err());
    });
}
