//! Worker status, waits and replies (DPES-1108 phase 4, migration 0065).
//!
//! The store's public API does the writes. A raw second connection tries the writes the
//! database itself must refuse, as another process would.

use std::fs;
use std::path::Path;

use orchestrator_core::contract::{
    AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, VerificationPolicy,
};
use orchestrator_core::task::{
    Attempt, AttemptState, Session, SessionState, TaskEvent, TaskState, WaitReason,
};
use orchestrator_core::worker_status::{StatusLine, StatusVerb};
use orchestrator_core::Project;
use orchestrator_store::{
    MessageAuthor, QuestionError, QuestionRaiser, ReplyKind, SqliteStore, Store, StoreError,
    WaitError, WorkerQuestion, MAX_QUEUED_MESSAGES,
};
use sqlx::migrate::Migrator;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::Connection;

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn try_sql(path: &Path, sql: &str) -> Result<(), sqlx::Error> {
    runtime().block_on(async {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .foreign_keys(true);
        let mut connection = sqlx::SqliteConnection::connect_with(&options).await?;
        // Test-only helper: every statement passed in is a literal written in this file.
        sqlx::query(sqlx::AssertSqlSafe(sql.to_owned()))
            .execute(&mut connection)
            .await?;
        Ok::<(), sqlx::Error>(())
    })
}

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

fn open(directory: &Path) -> SqliteStore {
    let store = SqliteStore::new(&directory.join("store.sqlite")).unwrap();
    store
        .register_project(&Project {
            id: "p".into(),
            root_path: "/repo".into(),
            settings: serde_json::json!({}),
        })
        .unwrap();
    store
}

/// A task with a Running attempt `{id}-a1` and a Running session `{id}-s1`, as the executor
/// leaves a live run.
fn run_task(store: &SqliteStore, id: &str) {
    store
        .create(&TaskContract {
            id: id.into(),
            project_id: "p".into(),
            parent_id: None,
            objective: "Pick a database".into(),
            kind: TaskKind::Implement,
            agent: "codex".into(),
            dependencies: Vec::new(),
            verification: VerificationPolicy::None,
            delivery: DeliveryPolicy::LocalReview,
            autonomy: AutonomyPolicy::WorkspaceWrites,
            max_retries: 0,
            max_repairs: 0,
            model_selection: None,
            work_item: None,
            permission: Default::default(),
        })
        .unwrap();
    store.transition(id, TaskEvent::Admit).unwrap();
    let attempt = format!("{id}-a1");
    store
        .add_attempt(
            id,
            &Attempt {
                id: attempt.clone(),
                task_id: id.into(),
                attempt_number: 1,
                state: AttemptState::Pending,
                failure_reason: None,
                summary: None,
                changed_files: None,
            },
        )
        .unwrap();
    store.transition(id, TaskEvent::ExecutionStarted).unwrap();
    store
        .update_attempt_state(id, &attempt, AttemptState::Running)
        .unwrap();
    store
        .add_session(
            id,
            &Session {
                id: format!("{id}-s1"),
                attempt_id: attempt,
                adapter: "codex".into(),
                state: SessionState::Spawning,
            },
        )
        .unwrap();
    store
        .update_session_state(id, &format!("{id}-s1"), SessionState::Running)
        .unwrap();
}

fn line(verb: StatusVerb, key: Option<&str>, note: Option<&str>) -> StatusLine {
    StatusLine {
        verb,
        key: key.map(str::to_owned),
        note: note.map(str::to_owned),
    }
}

fn question(id: &str, text: &str, key: Option<&str>) -> WorkerQuestion {
    WorkerQuestion {
        id: id.into(),
        question: text.into(),
        key: key.map(str::to_owned),
    }
}

fn wait_for_decision(store: &SqliteStore, id: &str) {
    store
        .enter_waiting(
            id,
            &format!("{id}-a1"),
            &format!("{id}-s1"),
            WaitReason::Decision,
            &[question("q-1", "SQLite or Postgres?", Some("db"))],
        )
        .unwrap();
}

fn resumed_session(id: &str, number: u32) -> Session {
    Session {
        id: format!("{id}-s{number}"),
        attempt_id: format!("{id}-a1"),
        adapter: "codex".into(),
        state: SessionState::Spawning,
    }
}

#[test]
fn status_lines_are_stored_once_per_line_and_reads_continue_after_the_last() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    run_task(&store, "t1");
    let lines = [
        (1, line(StatusVerb::Working, None, Some("reading"))),
        (
            3,
            line(StatusVerb::Blocked, Some("net"), Some("no network")),
        ),
    ];
    store
        .record_status_events("t1", "t1-a1", "t1-s1", &lines)
        .unwrap();
    // A second read of the same file adds nothing.
    store
        .record_status_events("t1", "t1-a1", "t1-s1", &lines)
        .unwrap();
    let events = store.status_events("t1").unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[1].verb, StatusVerb::Blocked);
    assert_eq!(events[1].key.as_deref(), Some("net"));
    assert_eq!(events[1].note.as_deref(), Some("no network"));
    assert_eq!(store.last_status_line("t1", "t1-a1").unwrap(), 3);
    assert_eq!(store.last_status_line("t1", "t1-a2").unwrap(), 0);

    let path = directory.path().join("store.sqlite");
    assert!(try_sql(&path, "UPDATE task_status_events SET note = 'x'").is_err());
    assert!(try_sql(&path, "DELETE FROM task_status_events").is_err());
    // The database refuses a decision line with no question, and an unknown verb.
    assert!(try_sql(
        &path,
        "INSERT INTO task_status_events (task_id, attempt_id, session_id, line_no, verb) \
         VALUES ('t1', 't1-a1', 't1-s1', 9, 'needs-decision')"
    )
    .is_err());
    assert!(try_sql(
        &path,
        "INSERT INTO task_status_events (task_id, attempt_id, session_id, line_no, verb) \
         VALUES ('t1', 't1-a1', 't1-s1', 9, 'paused')"
    )
    .is_err());
}

#[test]
fn an_agent_session_id_is_written_once() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    run_task(&store, "t1");
    let id = "01a1b721-1f91-7393-ae1d-6aace8e4646a";
    store.record_agent_session_id("t1", "t1-s1", id).unwrap();
    store.record_agent_session_id("t1", "t1-s1", id).unwrap();
    assert!(store
        .record_agent_session_id("t1", "t1-s1", "another-id")
        .is_err());
    assert!(store
        .record_agent_session_id("t1", "t1-s1", "not an id")
        .is_err());
}

#[test]
fn a_wait_parks_session_attempt_and_task_together_and_raises_the_workers_questions() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    run_task(&store, "t1");
    store
        .enter_waiting(
            "t1",
            "t1-a1",
            "t1-s1",
            WaitReason::Decision,
            &[
                question("q-1", "SQLite or Postgres?", Some("db")),
                question("q-2", "Same database question again?", Some("db")),
                question("q-3", "Keep the old schema?", None),
            ],
        )
        .unwrap();
    let task = store.get("t1").unwrap();
    assert_eq!(task.state, TaskState::Waiting);
    assert_eq!(task.attempts[0].state, AttemptState::Waiting);
    assert_eq!(task.sessions[0].state, SessionState::Exited);
    assert_eq!(
        task.events.last(),
        Some(&orchestrator_core::task::AuditEvent::WorkerNeedsDecision)
    );
    // One question per key: the second db question found the first.
    let open = store.open_questions().unwrap();
    let ids: Vec<_> = open.iter().map(|q| q.id.as_str()).collect();
    assert_eq!(ids, ["q-1", "q-3"]);
    assert!(open.iter().all(|q| q.raised_by == QuestionRaiser::Worker));
    // Nothing to deliver yet: the questions are open.
    assert!(store.tasks_ready_to_resume().unwrap().is_empty());
    // A task that does not run cannot park.
    assert!(matches!(
        store.enter_waiting("t1", "t1-a1", "t1-s1", WaitReason::Blocker, &[]),
        Err(StoreError::IllegalTransition(_))
    ));
}

#[test]
fn answers_queue_replies_and_a_resume_delivers_them_once_in_a_new_session() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    run_task(&store, "t1");
    store
        .record_agent_session_id("t1", "t1-s1", "abc-123")
        .unwrap();
    wait_for_decision(&store, "t1");

    store.answer_question("q-1", "  Postgres.\n").unwrap();
    assert_eq!(
        store.tasks_ready_to_resume().unwrap(),
        ["t1".to_owned()].into()
    );
    let replies = store.pending_replies("t1").unwrap();
    assert_eq!(replies.len(), 1);
    assert_eq!(replies[0].kind, ReplyKind::Answer);
    assert_eq!(replies[0].question.as_deref(), Some("SQLite or Postgres?"));
    assert_eq!(replies[0].text, "  Postgres.\n");

    let target = store.resume_target("t1").unwrap().unwrap();
    assert_eq!(target.attempt_id, "t1-a1");
    assert_eq!(target.sessions, 1);
    assert_eq!(target.agent_session_id.as_deref(), Some("abc-123"));

    // A user message accepted after the replies were read is not in that set. A resume with the
    // stale set is refused and writes nothing, so the task stays waiting.
    store
        .queue_task_message(
            "t1",
            "u-race",
            "Use the scratch database.",
            MessageAuthor::User,
        )
        .unwrap();
    assert_eq!(
        WaitError::of(
            &store
                .begin_resume("t1", &resumed_session("t1", 2), "abc-123", &[replies[0].id])
                .unwrap_err()
        ),
        Some(WaitError::ReplyChanged)
    );
    let waiting = store.get("t1").unwrap();
    assert_eq!(waiting.state, TaskState::Waiting);
    assert_eq!(waiting.sessions.len(), 1);
    assert_eq!(store.pending_replies("t1").unwrap().len(), 2);

    // Planned again from the replies as they are now, the resume delivers both.
    let ids: Vec<i64> = store
        .pending_replies("t1")
        .unwrap()
        .iter()
        .map(|reply| reply.id)
        .collect();
    store
        .begin_resume("t1", &resumed_session("t1", 2), "abc-123", &ids)
        .unwrap();
    let task = store.get("t1").unwrap();
    assert_eq!(task.state, TaskState::Running);
    assert_eq!(task.attempts.len(), 1, "a resume is the same attempt");
    assert_eq!(task.attempts[0].state, AttemptState::Running);
    assert_eq!(task.sessions.len(), 2);
    assert_eq!(task.sessions[1].state, SessionState::Spawning);
    assert!(store.pending_replies("t1").unwrap().is_empty());
    assert!(store.tasks_ready_to_resume().unwrap().is_empty());
    assert_eq!(store.resume_target("t1").unwrap(), None);
    // A second resume of a running task is refused, and delivery is never undone.
    assert_eq!(
        WaitError::of(
            &store
                .begin_resume("t1", &resumed_session("t1", 3), "abc-123", &[])
                .unwrap_err()
        ),
        Some(WaitError::NotWaiting)
    );
    let path = directory.path().join("store.sqlite");
    assert!(try_sql(
        &path,
        "UPDATE task_messages SET delivered_session_id = NULL"
    )
    .is_err());
    assert!(try_sql(&path, "DELETE FROM task_messages").is_err());
}

#[test]
fn a_reconcile_is_a_reply_and_a_question_on_a_running_task_queues_none() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    run_task(&store, "t1");
    store
        .raise_question(
            "q-run",
            "t1",
            "Ship it?",
            &[],
            None,
            QuestionRaiser::Orchestrator,
        )
        .unwrap();
    store.answer_question("q-run", "Yes").unwrap();
    assert!(store.pending_replies("t1").unwrap().is_empty());

    wait_for_decision(&store, "t1");
    store
        .reconcile_question("q-1", "The user picked Postgres in chat.")
        .unwrap();
    let replies = store.pending_replies("t1").unwrap();
    assert_eq!(replies[0].kind, ReplyKind::Reconcile);
    assert_eq!(replies[0].text, "The user picked Postgres in chat.");
}

#[test]
fn a_message_is_taken_only_by_a_waiting_task_and_only_up_to_the_queue() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    run_task(&store, "t1");
    let refusal = store
        .queue_task_message(
            "t1",
            "k0",
            "The network is back.",
            MessageAuthor::Orchestrator,
        )
        .unwrap_err();
    assert_eq!(WaitError::of(&refusal), Some(WaitError::NotWaiting));

    store
        .enter_waiting("t1", "t1-a1", "t1-s1", WaitReason::Blocker, &[])
        .unwrap();
    assert_eq!(
        store.get("t1").unwrap().events.last(),
        Some(&orchestrator_core::task::AuditEvent::WorkerBlocked)
    );
    for number in 0..MAX_QUEUED_MESSAGES {
        let key = format!("k{number}");
        store
            .queue_task_message(
                "t1",
                &key,
                "The network is back.",
                MessageAuthor::Orchestrator,
            )
            .unwrap();
        // A rerun of the same approved action queues nothing.
        store
            .queue_task_message(
                "t1",
                &key,
                "The network is back.",
                MessageAuthor::Orchestrator,
            )
            .unwrap();
        assert!(store.task_message_queued(&key).unwrap());
    }
    let full = store
        .queue_task_message("t1", "k-full", "Again.", MessageAuthor::Orchestrator)
        .unwrap_err();
    assert_eq!(WaitError::of(&full), Some(WaitError::QueueFull));
    assert!(store
        .queue_task_message("t1", "k-blank", " ", MessageAuthor::Orchestrator)
        .is_err());
    assert!(!store.task_message_queued("k-full").unwrap());
    assert_eq!(
        store.tasks_ready_to_resume().unwrap(),
        ["t1".to_owned()].into()
    );
    let replies = store.pending_replies("t1").unwrap();
    assert!(replies
        .iter()
        .all(|reply| reply.kind == ReplyKind::Message && reply.question.is_none()));
}

#[test]
fn the_users_messages_are_their_own_and_queue_apart_from_the_orchestrators() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    run_task(&store, "t1");
    store
        .enter_waiting("t1", "t1-a1", "t1-s1", WaitReason::Blocker, &[])
        .unwrap();
    for number in 0..MAX_QUEUED_MESSAGES {
        store
            .queue_task_message(
                "t1",
                &format!("k{number}"),
                "The network is back.",
                MessageAuthor::Orchestrator,
            )
            .unwrap();
    }
    // A full orchestrator queue leaves the user room, and the user's queue fills on its own.
    for number in 0..MAX_QUEUED_MESSAGES {
        store
            .queue_task_message(
                "t1",
                &format!("u{number}"),
                "Use the other branch.",
                MessageAuthor::User,
            )
            .unwrap();
    }
    let full = store
        .queue_task_message("t1", "u-full", "Again.", MessageAuthor::User)
        .unwrap_err();
    assert_eq!(WaitError::of(&full), Some(WaitError::QueueFull));
    // A worker resumes only when its question closes, so the user's message is refused while
    // the question is open. The orchestrator's message keeps its path.
    run_task(&store, "t2");
    wait_for_decision(&store, "t2");
    let asked = store
        .queue_task_message(
            "t2",
            "u-asked",
            "Use the other branch.",
            MessageAuthor::User,
        )
        .unwrap_err();
    assert_eq!(WaitError::of(&asked), Some(WaitError::QuestionOpen));
    store
        .queue_task_message(
            "t2",
            "k-asked",
            "Use the other branch.",
            MessageAuthor::Orchestrator,
        )
        .unwrap();

    let replies = store.pending_replies("t1").unwrap();
    let kinds: Vec<ReplyKind> = replies.iter().map(|reply| reply.kind).collect();
    assert_eq!(
        kinds,
        [
            ReplyKind::Message,
            ReplyKind::Message,
            ReplyKind::UserMessage,
            ReplyKind::UserMessage,
        ]
    );
    assert_eq!(replies[2].text, "Use the other branch.");
}

#[test]
fn a_waiting_task_cancels_without_an_owner_and_is_not_archived_while_it_waits() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    run_task(&store, "t1");
    wait_for_decision(&store, "t1");
    assert!(store.archive_tasks(&["t1".to_owned()]).is_err());

    store.cancel_waiting_task("t1").unwrap();
    let task = store.get("t1").unwrap();
    assert_eq!(task.state, TaskState::Cancelled);
    assert_eq!(task.attempts[0].state, AttemptState::Cancelled);
    // Only the user closes the worker's question, even on a cancelled task.
    assert_eq!(store.open_questions().unwrap().len(), 1);
    assert!(store.cancel_waiting_task("t1").is_err());
}

#[test]
fn a_waiting_task_that_cannot_resume_fails_with_its_reason() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    run_task(&store, "t1");
    store
        .enter_waiting("t1", "t1-a1", "t1-s1", WaitReason::Blocker, &[])
        .unwrap();
    store
        .fail_waiting_task("t1", "Codex cannot resume at this permission level.")
        .unwrap();
    let task = store.get("t1").unwrap();
    assert_eq!(task.state, TaskState::Failed);
    assert_eq!(task.attempts[0].state, AttemptState::Failed);
    assert_eq!(
        task.attempts[0].failure_reason.as_deref(),
        Some("Codex cannot resume at this permission level.")
    );
    assert_eq!(
        WaitError::of(&store.fail_waiting_task("t1", "again").unwrap_err()),
        Some(WaitError::NotWaiting)
    );
}

#[test]
fn no_candidate_is_recorded_while_a_workers_question_is_open() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    run_task(&store, "t1");
    store
        .raise_question("q-w", "t1", "Which one?", &[], None, QuestionRaiser::Worker)
        .unwrap();
    let refused = store
        .transition("t1", TaskEvent::CandidateReady)
        .unwrap_err();
    assert_eq!(
        QuestionError::of(&refused),
        Some(QuestionError::WorkerQuestionOpen)
    );
    assert_eq!(store.get("t1").unwrap().state, TaskState::Running);
    store.answer_question("q-w", "The first.").unwrap();
    store.transition("t1", TaskEvent::CandidateReady).unwrap();
}

#[test]
fn a_waiting_task_is_neither_running_work_nor_a_stall_and_its_note_reaches_the_wake_facts() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    run_task(&store, "t1");
    assert_eq!(store.running_attempts().unwrap().len(), 1);
    store
        .record_status_events(
            "t1",
            "t1-a1",
            "t1-s1",
            &[(
                2,
                line(StatusVerb::Blocked, None, Some("the test database is down")),
            )],
        )
        .unwrap();
    store
        .enter_waiting("t1", "t1-a1", "t1-s1", WaitReason::Blocker, &[])
        .unwrap();
    assert!(store.running_attempts().unwrap().is_empty());
    assert!(store.supervision_load().unwrap().is_empty());
    let facts = store.task_wake_facts("t1").unwrap().unwrap();
    assert_eq!(
        facts.wait_note.as_deref(),
        Some("the test database is down")
    );
}

#[test]
fn migration_0065_applies_to_a_0064_style_database_and_keeps_every_row() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("store.sqlite");
    let legacy_dir = directory.path().join("legacy-migrations");
    fs::create_dir(&legacy_dir).unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    for entry in fs::read_dir(&source).unwrap() {
        let entry = entry.unwrap().path();
        let name = entry.file_name().unwrap().to_str().unwrap().to_owned();
        let version: u32 = name.split('_').next().unwrap().parse().unwrap();
        if version < 65 {
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
    for statement in [
        "INSERT INTO projects (id, root_path, settings) VALUES ('p1', '/work/p1', '{}')",
        "INSERT INTO tasks (id, project_id, parent_id, state, contract_json, version) \
         VALUES ('t1', 'p1', NULL, '\"Queued\"', '{}', 1)",
        "INSERT INTO conversations (id, target_kind) VALUES ('conversation:global', 'global')",
        "INSERT INTO conversation_turns (id, conversation_id, state, runtime_name, runtime_version) \
         VALUES ('turn-1', 'conversation:global', 'completed', 'test-runtime', '1.0.0')",
        "INSERT INTO action_proposals (id, scope, turn_id, idx, kind, params_json, digest, subject) \
         VALUES ('pr1', 'conversation:global', 'turn-1', 0, 'decision.ask', '{}', \
                 '0000000000000000000000000000000000000000000000000000000000000001', 's')",
        "INSERT INTO wake_records (id, scope, event_kind, subject, dedupe_key, notice, \
         needs_model, state) VALUES ('w1', 'conversation:global', 'candidate_ready', 't1', \
         'candidate:t1:1', 'Task t1 finished.', 1, 'pending')",
        "INSERT INTO decision_holds (id, subject_kind, subject_id, set_by, question, options_json) \
         VALUES ('q1', 'question', 't1', 'worker', 'Which?', '[]')",
    ] {
        try_sql(&path, statement).unwrap();
    }
    let dump = |path: &Path| -> Vec<Vec<String>> {
        [
            "SELECT json_array(rowid, id, kind, state, grant_tried) FROM action_proposals \
             ORDER BY rowid",
            "SELECT json_array(rowid, id, event_kind, state, notice) FROM wake_records \
             ORDER BY rowid",
            "SELECT json_array(rowid, id, subject_kind, state) FROM decision_holds ORDER BY rowid",
        ]
        .into_iter()
        .map(|statement| scalars(path, statement))
        .collect()
    };
    let before = dump(&path);
    let new_kind = "INSERT INTO action_proposals \
         (id, scope, turn_id, idx, kind, params_json, digest, subject) \
         VALUES ('pr2', 'conversation:global', 'turn-1', 1, 'task.message', '{}', \
                 '0000000000000000000000000000000000000000000000000000000000000002', 's')";
    let new_wake = "INSERT INTO wake_records (id, scope, event_kind, subject, dedupe_key, \
         notice, needs_model, state) VALUES ('w2', 'conversation:global', \
         'worker_needs_decision', 't1', 'wait:t1:2', 'Task t1 waits.', 1, 'pending')";
    assert!(try_sql(&path, new_kind).is_err());
    assert!(try_sql(&path, new_wake).is_err());

    let _store = SqliteStore::new(&path).unwrap();
    assert_eq!(dump(&path), before);
    try_sql(&path, new_kind).unwrap();
    try_sql(&path, new_wake).unwrap();
    // The old rules came back with the rebuilt tables.
    assert!(try_sql(&path, "DELETE FROM action_proposals WHERE id = 'pr1'").is_err());
    assert!(try_sql(&path, "UPDATE action_proposals SET kind = 'task.retry'").is_err());
    assert!(try_sql(&path, "DELETE FROM wake_records").is_err());
    assert!(try_sql(&path, "UPDATE wake_records SET event_kind = 'task_failed'").is_err());
    // The hold insert rule still reads proposals.
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

#[test]
fn migration_0073_applies_to_a_0072_database_and_keeps_every_row() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("store.sqlite");
    let legacy_dir = directory.path().join("legacy-migrations");
    fs::create_dir(&legacy_dir).unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    for entry in fs::read_dir(&source).unwrap() {
        let entry = entry.unwrap().path();
        let name = entry.file_name().unwrap().to_str().unwrap().to_owned();
        let version: u32 = name.split('_').next().unwrap().parse().unwrap();
        if version < 73 {
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
    for statement in [
        "INSERT INTO projects (id, root_path, settings) VALUES ('p1', '/work/p1', '{}')",
        "INSERT INTO tasks (id, project_id, parent_id, state, contract_json, version) \
         VALUES ('t1', 'p1', NULL, '\"Waiting\"', '{}', 1)",
        "INSERT INTO attempts (task_id, id, attempt_number, state) \
         VALUES ('t1', 't1-a1', 1, '\"Waiting\"')",
        "INSERT INTO task_messages (task_id, attempt_id, kind, text, message_key) \
         VALUES ('t1', 't1-a1', 'message', 'The network is back.', 'k0')",
        "INSERT INTO decision_holds (id, subject_kind, subject_id, set_by, question, options_json) \
         VALUES ('q1', 'question', 't1', 'worker', 'Which?', '[]')",
        // The user's answer, delivered, and reconcile, which the migration must mark as the
        // user's. The answer's delivery is the row the old trigger would refuse to change.
        "INSERT INTO task_messages (task_id, attempt_id, kind, question_id) \
         VALUES ('t1', 't1-a1', 'answer', 'q1')",
        "UPDATE task_messages SET delivered_session_id = 't1-s1' WHERE kind = 'answer'",
        "INSERT INTO task_messages (task_id, attempt_id, kind, question_id) \
         VALUES ('t1', 't1-a1', 'reconcile', 'q1')",
    ] {
        try_sql(&path, statement).unwrap();
    }
    let dump = "SELECT json_array(id, task_id, kind, text, message_key) FROM task_messages \
         ORDER BY id";
    let before = scalars(&path, dump);
    let user = "INSERT INTO task_messages (task_id, attempt_id, kind, text, message_key, author) \
         VALUES ('t1', 't1-a1', 'message', 'Use the other branch.', 'u0', 'user')";
    let robot = "INSERT INTO task_messages (task_id, attempt_id, kind, text, message_key, author) \
         VALUES ('t1', 't1-a1', 'message', 'x', 'k1', 'robot')";

    let _store = SqliteStore::new(&path).unwrap();
    assert_eq!(scalars(&path, dump), before);
    assert_eq!(
        scalars(&path, "SELECT author FROM task_messages ORDER BY id"),
        vec!["orchestrator", "user", "user"]
    );
    let refused = try_sql(&path, robot).unwrap_err();
    assert!(
        refused.to_string().contains("CHECK constraint failed"),
        "{refused}"
    );
    try_sql(&path, user).unwrap();
    // The author is fixed with the row. Setting the delivery alone is what the resume does.
    assert!(try_sql(
        &path,
        "UPDATE task_messages SET delivered_session_id = 't1-s2', author = 'orchestrator' \
         WHERE message_key = 'u0'"
    )
    .is_err());
    try_sql(
        &path,
        "UPDATE task_messages SET delivered_session_id = 't1-s2' WHERE message_key = 'u0'",
    )
    .unwrap();
}
