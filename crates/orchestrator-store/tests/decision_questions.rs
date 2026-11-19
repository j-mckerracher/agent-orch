//! User decisions (DPES-1108 phase 3, migration 0064): questions held for the user.
//!
//! The store's public API does the writes. A second raw connection tries the writes the
//! database itself must refuse, as another process would. Time is injected; no test sleeps.

use std::fs;
use std::path::{Path, PathBuf};

use orchestrator_core::Project;
use orchestrator_store::{
    ClosingKind, HoldSubject, QuestionError, QuestionRaiser, SqliteStore, StoreError,
};
use sqlx::migrate::Migrator;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::Connection;

/// 2026-11-17 in the afternoon, UTC.
const NOW_NOV_17: i64 = 1_794_901_798_000;
/// The last millisecond of 2026-11-18, UTC.
const END_NOV_18: i64 = 1_795_046_399_999;

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn try_sql(path: &Path, sql: &str, binds: &[&str]) -> Result<(), sqlx::Error> {
    runtime().block_on(async {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .foreign_keys(true);
        let mut connection = sqlx::SqliteConnection::connect_with(&options).await?;
        // Test-only helper: every statement passed in is a literal written in this file.
        let mut query = sqlx::query(sqlx::AssertSqlSafe(sql.to_owned()));
        for value in binds {
            query = query.bind(*value);
        }
        query.execute(&mut connection).await?;
        Ok::<(), sqlx::Error>(())
    })
}

fn sql(path: &Path, statement: &str, binds: &[&str]) {
    try_sql(path, statement, binds).unwrap();
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

fn add_task(path: &Path, id: &str, state: &str) {
    sql(
        path,
        "INSERT INTO tasks (id, project_id, parent_id, state, contract_json, version) \
         VALUES (?, 'p1', NULL, ?, '{}', 1)",
        &[id, &format!("\"{state}\"")],
    );
}

/// A store with project `p1` and the queued task `t1`.
fn seeded(directory: &Path) -> (SqliteStore, PathBuf) {
    let path = directory.join("store.sqlite");
    let store = SqliteStore::new(&path).unwrap();
    store
        .register_project(&Project {
            id: "p1".to_owned(),
            root_path: "/work/p1".to_owned(),
            settings: serde_json::json!({}),
        })
        .unwrap();
    add_task(&path, "t1", "Queued");
    (store, path)
}

fn ask(store: &SqliteStore, id: &str, key: Option<&str>) -> orchestrator_store::DecisionQuestion {
    let options = vec!["SQLite".to_owned(), "Postgres".to_owned()];
    store
        .raise_question(
            id,
            "t1",
            "Which database?",
            &options,
            key,
            QuestionRaiser::Orchestrator,
        )
        .unwrap()
}

fn refusal(error: &StoreError) -> Option<QuestionError> {
    QuestionError::of(error)
}

#[test]
fn a_raised_question_is_open_listed_and_holds_its_task() {
    let directory = tempfile::tempdir().unwrap();
    let (store, _path) = seeded(directory.path());

    let question = ask(&store, "q1", None);
    assert!(question.is_open());
    assert_eq!(question.task_id, "t1");
    assert_eq!(question.options, ["SQLite", "Postgres"]);
    assert_eq!(question.raised_by, QuestionRaiser::Orchestrator);
    assert_eq!(store.open_questions().unwrap(), vec![question.clone()]);
    assert!(store.tasks_with_open_questions().unwrap().contains("t1"));
    // A question is not a hold: the hold lists never show it.
    assert!(store.active_decision_holds().unwrap().is_empty());
    assert!(store
        .decision_holds_in_force(NOW_NOV_17)
        .unwrap()
        .is_empty());
    assert_eq!(store.decision_hold("q1").unwrap(), None);

    // The same id asks once.
    assert_eq!(ask(&store, "q1", None), question);
    assert_eq!(store.open_questions().unwrap().len(), 1);
}

#[test]
fn a_key_asks_once_per_task_and_an_id_names_one_task() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = seeded(directory.path());
    add_task(&path, "t2", "Queued");

    let first = ask(&store, "q1", Some("db"));
    // Another wording under the same key finds the open question.
    let again = store
        .raise_question("q2", "t1", "DB?", &[], Some("db"), QuestionRaiser::Worker)
        .unwrap();
    assert_eq!(again, first);
    // Without a key, or with another, a task may have several open questions.
    ask(&store, "q3", None);
    ask(&store, "q4", Some("cache"));
    assert_eq!(store.open_questions().unwrap().len(), 3);

    let taken = store
        .raise_question(
            "q1",
            "t2",
            "Other?",
            &[],
            None,
            QuestionRaiser::Orchestrator,
        )
        .unwrap_err();
    assert_eq!(refusal(&taken), Some(QuestionError::IdInUse));
    let missing = store
        .raise_question("q9", "nope", "Q?", &[], None, QuestionRaiser::Orchestrator)
        .unwrap_err();
    assert_eq!(refusal(&missing), Some(QuestionError::NoTask));
    let blank = store
        .raise_question("q9", "t1", "  ", &[], None, QuestionRaiser::Orchestrator)
        .unwrap_err();
    assert_eq!(refusal(&blank), Some(QuestionError::BadQuestion));
}

#[test]
fn an_answer_keeps_the_exact_words_and_closes_the_question_once() {
    let directory = tempfile::tempdir().unwrap();
    let (store, _path) = seeded(directory.path());
    ask(&store, "q1", None);

    let words = "  Postgres, but only for prod.\nKeep SQLite in tests.  ";
    let answered = store.answer_question("q1", words).unwrap();
    let closing = answered.closing.clone().unwrap();
    assert_eq!(closing.kind, ClosingKind::Answer);
    assert_eq!(closing.text, words);
    assert!(!answered.is_open());
    assert!(store.open_questions().unwrap().is_empty());
    assert!(store.tasks_with_open_questions().unwrap().is_empty());
    assert_eq!(store.decision_question("q1").unwrap(), Some(answered));

    let twice = store.answer_question("q1", "SQLite").unwrap_err();
    assert_eq!(refusal(&twice), Some(QuestionError::Closed));
    let late = store.reconcile_question("q1", "moot").unwrap_err();
    assert_eq!(refusal(&late), Some(QuestionError::Closed));
    let deferred = store
        .defer_question("q1", "2026-11-18", END_NOV_18, NOW_NOV_17)
        .unwrap_err();
    assert_eq!(refusal(&deferred), Some(QuestionError::Closed));
    assert!(matches!(
        store.answer_question("nope", "x"),
        Err(StoreError::RecordNotFound { .. })
    ));
    let blank = store.answer_question("nope", " \n").unwrap_err();
    assert_eq!(refusal(&blank), Some(QuestionError::BadReply));
}

#[test]
fn a_reconcile_closes_with_evidence_never_as_an_answer() {
    let directory = tempfile::tempdir().unwrap();
    let (store, _path) = seeded(directory.path());
    ask(&store, "q1", None);

    let closed = store
        .reconcile_question("q1", "The task was cancelled; nothing to decide.")
        .unwrap();
    let closing = closed.closing.unwrap();
    assert_eq!(closing.kind, ClosingKind::Reconcile);
    assert_eq!(closing.text, "The task was cancelled; nothing to decide.");
    assert!(store.tasks_with_open_questions().unwrap().is_empty());
}

#[test]
fn a_deferral_hides_the_question_until_its_day_but_keeps_it_open_and_holding() {
    let directory = tempfile::tempdir().unwrap();
    let (store, _path) = seeded(directory.path());
    ask(&store, "q1", None);

    let deferred = store
        .defer_question("q1", "2026-11-18", END_NOV_18, NOW_NOV_17)
        .unwrap();
    assert!(deferred.is_open());
    assert_eq!(
        deferred.deferred_until,
        Some(("2026-11-18".to_owned(), END_NOV_18))
    );
    assert!(deferred.is_deferred_at(NOW_NOV_17));
    assert!(!deferred.is_deferred_at(END_NOV_18));
    assert!(store.tasks_with_open_questions().unwrap().contains("t1"));
    assert_eq!(store.open_questions().unwrap().len(), 1);

    // The date rules of a hold, on the caller's clock.
    let past = store
        .defer_question("q1", "2026-11-18", END_NOV_18, END_NOV_18)
        .unwrap_err();
    assert_eq!(refusal(&past), Some(QuestionError::NotInFuture));
    let wrong = store
        .defer_question("q1", "2026-11-18", END_NOV_18 - 1, NOW_NOV_17)
        .unwrap_err();
    assert_eq!(refusal(&wrong), Some(QuestionError::BadDate));

    // A deferred question is still answerable.
    assert!(!store.answer_question("q1", "Postgres").unwrap().is_open());
}

#[test]
fn the_database_refuses_every_close_that_is_not_an_answer_or_a_reconcile() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = seeded(directory.path());
    ask(&store, "q1", None);
    store
        .defer_question("q1", "2026-11-18", END_NOV_18, NOW_NOV_17)
        .unwrap();

    // A deferral is on record, but it does not close a question.
    let release = "UPDATE decision_holds SET state = 'released', released_at = CURRENT_TIMESTAMP \
                   WHERE id = 'q1'";
    assert!(try_sql(&path, release, &[]).is_err());
    // Nor does a hold call: a question is not a hold.
    assert!(store.release_decision_hold("q1").is_err());
    assert_eq!(
        store
            .clear_decision_hold(HoldSubject::Approval, "t1")
            .unwrap(),
        None
    );
    assert_eq!(store.release_proposal_holds(&["q1".to_owned()]).unwrap(), 0);
    // Nor does anything that happens to the task.
    sql(
        &path,
        "UPDATE tasks SET state = '\"Succeeded\"' WHERE id = 't1'",
        &[],
    );
    assert!(store.decision_question("q1").unwrap().unwrap().is_open());

    // The question itself is fixed and never deleted.
    assert!(try_sql(
        &path,
        "UPDATE decision_holds SET question = 'x' WHERE id = 'q1'",
        &[]
    )
    .is_err());
    assert!(try_sql(&path, "DELETE FROM decision_holds WHERE id = 'q1'", &[]).is_err());
    // A question needs a task and starts open.
    let insert = |task: &str, state: &str, released: &str| {
        try_sql(
            &path,
            &format!(
                "INSERT INTO decision_holds (id, subject_kind, subject_id, set_by, state, \
                 released_at, question, options_json) \
                 VALUES ('q9', 'question', '{task}', 'orchestrator', '{state}', {released}, \
                 'Q?', '[]')"
            ),
            &[],
        )
    };
    assert!(insert("nope", "active", "NULL").is_err());
    assert!(insert("t1", "released", "CURRENT_TIMESTAMP").is_err());
    assert!(insert("t1", "active", "NULL").is_ok());
    // Only the user records what happens to a question.
    let by_agent = "INSERT INTO decision_answers (hold_id, kind, text, recorded_by) \
                    VALUES ('q1', 'answer', 'yes', 'orchestrator')";
    assert!(try_sql(&path, by_agent, &[]).is_err());

    // Once answered, the records are append-only and the question takes no more.
    store.answer_question("q1", "Postgres").unwrap();
    assert!(try_sql(&path, "UPDATE decision_answers SET text = 'SQLite'", &[]).is_err());
    assert!(try_sql(&path, "DELETE FROM decision_answers", &[]).is_err());
    let more = "INSERT INTO decision_answers (hold_id, kind, text, recorded_by) \
                VALUES ('q1', 'answer', 'SQLite', 'user')";
    assert!(try_sql(&path, more, &[]).is_err());
    let reopen = "UPDATE decision_holds SET state = 'active', released_at = NULL WHERE id = 'q1'";
    assert!(try_sql(&path, reopen, &[]).is_err());
    assert_eq!(
        scalars(
            &path,
            "SELECT text FROM decision_answers WHERE kind = 'answer'"
        ),
        ["Postgres"]
    );
}

#[test]
fn a_scout_report_is_not_relayed_while_its_worker_question_is_open() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = seeded(directory.path());
    sql(
        &path,
        "INSERT INTO task_reports (task_id, attempt_id, body, captured_at) \
         VALUES ('t1', 'a1', 'finding', '2026-11-17T00:00:00Z')",
        &[],
    );
    // A question the orchestrator raised does not hold the report back.
    ask(&store, "q1", None);
    store
        .raise_question("q2", "t1", "Go deeper?", &[], None, QuestionRaiser::Worker)
        .unwrap();
    let refused = store
        .mark_task_report_relayed("t1", "2026-11-17T01:00:00Z")
        .unwrap_err();
    assert_eq!(refusal(&refused), Some(QuestionError::WorkerQuestionOpen));
    assert_eq!(store.task_report("t1").unwrap().unwrap().relayed_at, None);

    store.answer_question("q2", "No, report now.").unwrap();
    assert!(store
        .mark_task_report_relayed("t1", "2026-11-17T01:00:00Z")
        .unwrap());
}

#[test]
fn migration_0064_applies_to_a_0063_style_database_and_keeps_every_row() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("store.sqlite");
    let legacy_dir = directory.path().join("legacy-migrations");
    fs::create_dir(&legacy_dir).unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    for entry in fs::read_dir(&source).unwrap() {
        let entry = entry.unwrap().path();
        let name = entry.file_name().unwrap().to_str().unwrap().to_owned();
        let version: u32 = name.split('_').next().unwrap().parse().unwrap();
        if version < 64 {
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
    sql(
        &path,
        "INSERT INTO projects (id, root_path, settings) VALUES ('p1', '/work/p1', '{}')",
        &[],
    );
    add_task(&path, "t1", "Queued");
    sql(
        &path,
        "INSERT INTO approvals (id, task_id, action, status, granted_by) \
         VALUES ('a1', 't1', '{}', 'requested', NULL)",
        &[],
    );
    sql(
        &path,
        "INSERT INTO decision_holds (id, subject_kind, subject_id, until_date, until_ms, set_by) \
         VALUES ('h1', 'approval', 't1', '2026-11-18', 1795046399999, 'user')",
        &[],
    );
    sql(
        &path,
        "INSERT INTO conversations (id, target_kind) VALUES ('conversation:global', 'global')",
        &[],
    );
    sql(
        &path,
        "INSERT INTO conversation_turns (id, conversation_id, state, runtime_name, runtime_version) \
         VALUES ('turn-1', 'conversation:global', 'completed', 'test-runtime', '1.0.0')",
        &[],
    );
    sql(
        &path,
        "INSERT INTO action_proposals (id, scope, turn_id, idx, kind, params_json, digest, subject) \
         VALUES ('pr1', 'conversation:global', 'turn-1', 0, 'decision.hold', '{}', \
                 '0000000000000000000000000000000000000000000000000000000000000001', 's')",
        &[],
    );
    let dump = |path: &Path| -> Vec<Vec<String>> {
        [
            "SELECT json_array(rowid, id, subject_kind, subject_id, until_date, until_ms, set_by, \
             state) FROM decision_holds ORDER BY rowid",
            "SELECT json_array(rowid, id, kind, state, grant_tried) FROM action_proposals \
             ORDER BY rowid",
        ]
        .into_iter()
        .map(|statement| scalars(path, statement))
        .collect()
    };
    let before = dump(&path);
    assert!(try_sql(
        &path,
        "INSERT INTO action_proposals (id, scope, turn_id, idx, kind, params_json, digest, subject) \
         VALUES ('pr2', 'conversation:global', 'turn-1', 1, 'decision.ask', '{}', \
                 '0000000000000000000000000000000000000000000000000000000000000002', 's')",
        &[],
    )
    .is_err());

    let store = SqliteStore::new(&path).unwrap();
    assert_eq!(dump(&path), before);
    assert_eq!(store.active_decision_holds().unwrap().len(), 1);
    // The new kind is accepted, and the old rules came back with the table.
    sql(
        &path,
        "INSERT INTO action_proposals (id, scope, turn_id, idx, kind, params_json, digest, subject) \
         VALUES ('pr2', 'conversation:global', 'turn-1', 1, 'decision.ask', '{}', \
                 '0000000000000000000000000000000000000000000000000000000000000002', 's')",
        &[],
    );
    assert!(try_sql(&path, "DELETE FROM action_proposals WHERE id = 'pr1'", &[]).is_err());
    assert!(try_sql(
        &path,
        "UPDATE action_proposals SET kind = 'task.retry'",
        &[]
    )
    .is_err());
    assert!(try_sql(&path, "DELETE FROM decision_holds", &[]).is_err());
    // The hold insert rule still reads proposals.
    assert!(try_sql(
        &path,
        "INSERT INTO decision_holds (id, subject_kind, subject_id, until_date, until_ms, set_by) \
         VALUES ('h2', 'proposal', 'missing', '2026-11-18', 1795046399999, 'user')",
        &[],
    )
    .is_err());
    sql(
        &path,
        "INSERT INTO decision_holds (id, subject_kind, subject_id, until_date, until_ms, set_by) \
         VALUES ('h2', 'proposal', 'pr1', '2026-11-18', 1795046399999, 'user')",
        &[],
    );
}
