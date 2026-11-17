//! Standing grants and their audit trail (DPES-1105 phase 5a, migration 0047).
//!
//! The store's public API does the writes. A second raw connection reads the tables and tries
//! the writes the database itself must refuse, as another process would. Time is injected: a
//! grant is compared with the `now_ms` the test passes, and no test sleeps.

use std::fs;
use std::path::Path;
use std::sync::{Arc, Barrier};
use std::thread;

use orchestrator_core::orchestrator_action::ActionKind;
use orchestrator_core::standing_grant::{GrantRefusal, GrantScope, GrantTerms};
use orchestrator_store::{GrantError, GrantEventKind, GrantOutcome, SqliteStore, StoreError};
use sqlx::migrate::Migrator;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::Connection;

/// 2026-11-17 in the afternoon, UTC.
const NOW: i64 = 1_794_901_798_000;
const HOUR_MS: u64 = 3_600_000;
const DIGEST: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const OTHER_DIGEST: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn now() -> u64 {
    u64::try_from(NOW).unwrap()
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

/// Runs one statement on a second connection with `binds` bound in order, and reports whether
/// the database took it.
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

/// The first column of every row `statement` returns, as text, read on a second connection.
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

fn count(path: &Path, statement: &'static str) -> String {
    scalars(path, statement).remove(0)
}

fn terms(max_uses: u32) -> GrantTerms {
    GrantTerms {
        scope: GrantScope::Project("p1".to_owned()),
        kinds: vec![ActionKind::TaskPropose, ActionKind::TaskRetry],
        expires_in_secs: 3600,
        max_uses,
        token_budget: 1000,
    }
}

fn store_at(directory: &Path) -> (SqliteStore, std::path::PathBuf) {
    let path = directory.join("store.sqlite");
    (SqliteStore::new(&path).unwrap(), path)
}

fn refusal(error: &StoreError) -> Option<GrantError> {
    GrantError::of(error)
}

fn consume(
    store: &SqliteStore,
    id: &str,
    proposal: &str,
    at: u64,
) -> Result<(), Option<GrantError>> {
    store
        .consume_grant_use(id, proposal, DIGEST, at)
        .map(drop)
        .map_err(|error| refusal(&error))
}

/// A task with one session, so a usage receipt can belong to it.
fn add_task_with_session(path: &Path, task: &str) {
    sql(
        path,
        "INSERT INTO projects (id, root_path, settings) VALUES ('p1', '/work/p1', '{}') ON CONFLICT DO NOTHING",
        &[],
    );
    sql(
        path,
        "INSERT INTO tasks (id, project_id, parent_id, state, contract_json, version) \
         VALUES (?, 'p1', NULL, '\"Running\"', '{}', 1)",
        &[task],
    );
    sql(
        path,
        "INSERT INTO attempts (id, task_id, attempt_number, state) VALUES (?, ?, 1, 'Running')",
        &[&format!("{task}-attempt"), task],
    );
    sql(
        path,
        "INSERT INTO sessions (id, task_id, attempt_id, adapter, state) \
         VALUES (?, ?, ?, 'codex', 'Running')",
        &[&format!("{task}-session"), task, &format!("{task}-attempt")],
    );
}

/// One receipt. `tokens` is input, output and reasoning, each `None` for unreported.
fn add_receipt(
    path: &Path,
    task: &str,
    source: &str,
    key: &str,
    window_end_ms: u64,
    tokens: [Option<u64>; 3],
) {
    let text = |value: Option<u64>| value.map(|number| number.to_string());
    let [input, output, reasoning] = tokens.map(text);
    let end = window_end_ms.to_string();
    // Test-only: NULL is passed as a missing value by building the statement per figure.
    let figure = |value: &Option<String>| value.clone().unwrap_or_else(|| "NULL".to_owned());
    let statement = format!(
        "INSERT INTO usage_receipts (task_id, session_id, source, receipt_key, window_start_ms, \
         window_end_ms, reported_at_ms, input_tokens, output_tokens, reasoning_tokens) \
         VALUES (?, ?, ?, ?, 0, {end}, {end}, {}, {}, {})",
        figure(&input),
        figure(&output),
        figure(&reasoning),
    );
    sql(
        path,
        &statement,
        &[task, &format!("{task}-session"), source, key],
    );
}

#[test]
fn migration_0047_applies_to_a_database_at_the_latest_version_and_keeps_every_row() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("store.sqlite");
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
        if version < 47 {
            fs::copy(&entry, legacy_dir.join(entry.file_name().unwrap())).unwrap();
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
        let newest: i64 = sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations")
            .fetch_one(&mut connection)
            .await
            .unwrap();
        assert!(newest < 47);
        connection.close().await.unwrap();
    });
    sql(
        &path,
        "INSERT INTO projects (id, root_path, settings) VALUES ('old', '/work/old', '{}')",
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
        "INSERT INTO action_proposals \
         (id, scope, turn_id, idx, kind, params_json, digest, subject, state) \
         VALUES ('proposal-old', 'conversation:global', 'turn-1', 0, 'project.remove', '{}', ?, 'Subject', 'pending')",
        &[DIGEST],
    );

    let dump = |path: &Path| -> Vec<Vec<String>> {
        [
            "SELECT json_array(id, root_path, settings) FROM projects ORDER BY id",
            "SELECT json_array(rowid, id, scope, kind, digest, state, params_json, subject) \
             FROM action_proposals ORDER BY rowid",
        ]
        .into_iter()
        .map(|statement| scalars(path, statement))
        .collect()
    };
    let before = dump(&path);
    assert!(scalars(
        &path,
        "SELECT name FROM sqlite_master WHERE name IN ('standing_grants', 'standing_grant_events')"
    )
    .is_empty());

    let store = SqliteStore::new(&path).unwrap();
    assert_eq!(
        scalars(
            &path,
            "SELECT CAST(success AS TEXT) FROM _sqlx_migrations WHERE version = 47"
        ),
        vec!["1"]
    );
    assert_eq!(dump(&path), before);
    assert_eq!(
        count(&path, "SELECT CAST(COUNT(*) AS TEXT) FROM standing_grants"),
        "0"
    );
    // A proposal that existed before the migration is never matched, and the flag can flip.
    assert_eq!(
        scalars(
            &path,
            "SELECT CAST(grant_tried AS TEXT) FROM action_proposals"
        ),
        vec!["1"]
    );
    sql(&path, "UPDATE action_proposals SET grant_tried = 0", &[]);
    assert_eq!(
        scalars(
            &path,
            "SELECT CAST(grant_tried AS TEXT) FROM action_proposals"
        ),
        vec!["0"]
    );
    // The new tables work on the migrated database.
    let grant = store
        .create_standing_grant("grant-1", &terms(3), now())
        .unwrap();
    assert_eq!(grant.uses_used, 0);
}

#[test]
fn a_grant_for_a_never_grantable_kind_is_refused() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = store_at(directory.path());
    for kind in [
        ActionKind::TaskCancel,
        ActionKind::TaskRevise,
        ActionKind::ProjectRemove,
    ] {
        let mut asked = terms(5);
        asked.kinds = vec![ActionKind::TaskPropose, kind];
        let error = store
            .create_standing_grant("grant-never", &asked, now())
            .unwrap_err();
        assert_eq!(
            refusal(&error),
            Some(GrantError::Invalid(GrantRefusal::KindNeverGrantable)),
            "{kind:?}"
        );
    }
    assert_eq!(
        count(&path, "SELECT CAST(COUNT(*) AS TEXT) FROM standing_grants"),
        "0"
    );
    assert_eq!(
        count(
            &path,
            "SELECT CAST(COUNT(*) AS TEXT) FROM standing_grant_events"
        ),
        "0"
    );
    // The database refuses the names as well, as another writer would meet it.
    for name in [
        "project.remove",
        "delivery.merge",
        "task.cancel",
        "task.revise",
    ] {
        let kinds = format!("[\"task.propose\",\"{name}\"]");
        assert!(
            try_sql(
                &path,
                "INSERT INTO standing_grants (id, scope_kind, scope_id, kinds, created_at_ms, \
                 expires_at_ms, max_uses, token_budget, proposal_floor) \
                 VALUES ('raw', 'project', 'p1', ?, 1000, 2000, 5, 10, 0)",
                &[&kinds],
            )
            .is_err(),
            "{name}"
        );
    }
}

#[test]
fn a_grant_cannot_be_edited_only_revoked() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = store_at(directory.path());
    store
        .create_standing_grant("grant-1", &terms(5), now())
        .unwrap();

    for statement in [
        "UPDATE standing_grants SET id = 'grant-2'",
        "UPDATE standing_grants SET scope_kind = 'workstream'",
        "UPDATE standing_grants SET scope_id = 'p2'",
        "UPDATE standing_grants SET kinds = '[\"task.retry\"]'",
        "UPDATE standing_grants SET created_at_ms = created_at_ms + 1",
        "UPDATE standing_grants SET expires_at_ms = expires_at_ms + 1",
        "UPDATE standing_grants SET max_uses = max_uses + 1",
        "UPDATE standing_grants SET token_budget = token_budget + 1",
        "UPDATE standing_grants SET proposal_floor = proposal_floor + 1",
        "UPDATE standing_grants SET uses_used = uses_used + 2",
        "UPDATE standing_grants SET uses_used = 0",
        "DELETE FROM standing_grants",
    ] {
        assert!(try_sql(&path, statement, &[]).is_err(), "{statement}");
    }
    assert_eq!(
        scalars(&path, "SELECT CAST(uses_used AS TEXT) FROM standing_grants"),
        vec!["0"]
    );
    // One use at a time is allowed, and a revoke is the only other change.
    sql(
        &path,
        "UPDATE standing_grants SET uses_used = uses_used + 1",
        &[],
    );

    let revoked = store.revoke_standing_grant("grant-1", now() + 5).unwrap();
    assert_eq!(revoked.revoked_at_ms, Some(now() + 5));
    for statement in [
        "UPDATE standing_grants SET revoked_at_ms = NULL",
        "UPDATE standing_grants SET revoked_at_ms = revoked_at_ms + 1",
        "UPDATE standing_grants SET uses_used = uses_used + 1",
    ] {
        assert!(try_sql(&path, statement, &[]).is_err(), "{statement}");
    }
}

#[test]
fn a_revoked_grant_cannot_be_used() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = store_at(directory.path());
    store
        .create_standing_grant("grant-1", &terms(5), now())
        .unwrap();
    consume(&store, "grant-1", "proposal-1", now() + 1).unwrap();

    let revoked = store.revoke_standing_grant("grant-1", now() + 2).unwrap();
    assert_eq!(revoked.uses_used, 1, "a revoke refunds nothing");
    // Revoking twice is the same revoke: one event, the first time stays.
    let again = store.revoke_standing_grant("grant-1", now() + 9).unwrap();
    assert_eq!(again.revoked_at_ms, Some(now() + 2));
    assert_eq!(
        count(
            &path,
            "SELECT CAST(COUNT(*) AS TEXT) FROM standing_grant_events WHERE event = 'revoked'"
        ),
        "1"
    );
    for (proposal, at) in [
        ("proposal-2", now() + 3),
        ("proposal-3", now() + HOUR_MS * 9),
    ] {
        assert_eq!(
            consume(&store, "grant-1", proposal, at),
            Err(Some(GrantError::Revoked))
        );
    }
    assert_eq!(
        scalars(&path, "SELECT CAST(uses_used AS TEXT) FROM standing_grants"),
        vec!["1"]
    );
}

#[test]
fn the_last_use_is_consumed_exactly_once_by_two_handles() {
    let directory = tempfile::tempdir().unwrap();
    let (first, path) = store_at(directory.path());
    let second = SqliteStore::new(&path).unwrap();
    first
        .create_standing_grant("grant-1", &terms(1), now())
        .unwrap();

    let barrier = Arc::new(Barrier::new(2));
    let handles: Vec<_> = [(first, "proposal-a"), (second, "proposal-b")]
        .into_iter()
        .map(|(store, proposal)| {
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                consume(&store, "grant-1", proposal, now() + 1)
            })
        })
        .collect();
    let results: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect();

    assert_eq!(
        results.iter().filter(|result| result.is_ok()).count(),
        1,
        "{results:?}"
    );
    assert!(
        results.contains(&Err(Some(GrantError::UsesExhausted))),
        "{results:?}"
    );
    assert_eq!(
        scalars(&path, "SELECT CAST(uses_used AS TEXT) FROM standing_grants"),
        vec!["1"]
    );
    assert_eq!(
        scalars(
            &path,
            "SELECT event FROM standing_grant_events ORDER BY sequence"
        ),
        vec!["created", "used"]
    );
}

#[test]
fn an_expired_grant_cannot_be_used() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = store_at(directory.path());
    let grant = store
        .create_standing_grant("grant-1", &terms(5), now())
        .unwrap();
    assert_eq!(grant.expires_at_ms, now() + HOUR_MS);

    // The last millisecond still works; the expiry instant does not; time never rewinds a grant.
    consume(&store, "grant-1", "proposal-1", grant.expires_at_ms - 1).unwrap();
    assert_eq!(
        consume(&store, "grant-1", "proposal-2", grant.expires_at_ms),
        Err(Some(GrantError::Expired))
    );
    assert_eq!(
        consume(
            &store,
            "grant-1",
            "proposal-3",
            grant.expires_at_ms + HOUR_MS
        ),
        Err(Some(GrantError::Expired))
    );
    assert_eq!(
        scalars(&path, "SELECT CAST(uses_used AS TEXT) FROM standing_grants"),
        vec!["1"]
    );
    // The database holds the same line for any writer: a grant lasts at most seven days.
    assert!(try_sql(
        &path,
        "INSERT INTO standing_grants (id, scope_kind, scope_id, kinds, created_at_ms, \
         expires_at_ms, max_uses, token_budget, proposal_floor) \
         VALUES ('long', 'project', 'p1', '[\"task.propose\"]', 0, 604800001, 5, 10, 0)",
        &[],
    )
    .is_err());
}

#[test]
fn a_grant_event_cannot_be_updated_or_deleted() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = store_at(directory.path());
    store
        .create_standing_grant("grant-1", &terms(5), now())
        .unwrap();
    consume(&store, "grant-1", "proposal-1", now() + 1).unwrap();

    for statement in [
        "UPDATE standing_grant_events SET at_ms = at_ms + 1",
        "UPDATE standing_grant_events SET code = 'x'",
        "DELETE FROM standing_grant_events",
        "DELETE FROM standing_grant_events WHERE event = 'used'",
    ] {
        assert!(try_sql(&path, statement, &[]).is_err(), "{statement}");
    }
    let events = store.grant_events("grant-1", 0, 10).unwrap();
    assert_eq!(
        events.iter().map(|event| event.event).collect::<Vec<_>>(),
        vec![GrantEventKind::Created, GrantEventKind::Used]
    );
    let used = &events[1];
    assert_eq!(used.proposal_id.as_deref(), Some("proposal-1"));
    assert_eq!(used.digest.as_deref(), Some(DIGEST));
    assert_eq!(used.at_ms, now() + 1);
}

#[test]
fn create_is_idempotent_and_refuses_the_same_id_with_other_terms() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = store_at(directory.path());
    sql(
        &path,
        "INSERT INTO action_proposal_events (proposal_id, scope, from_state, to_state, actor, digest) \
         VALUES ('old-proposal', 'conversation:global', NULL, 'pending', 'agent', ?)",
        &[DIGEST],
    );
    let first = store
        .create_standing_grant("grant-1", &terms(5), now())
        .unwrap();
    assert_eq!(
        first.proposal_floor, 1,
        "the floor is the newest proposal event"
    );
    assert_eq!(
        first.kinds,
        vec![ActionKind::TaskPropose, ActionKind::TaskRetry]
    );

    let again = store
        .create_standing_grant("grant-1", &terms(5), now() + 50)
        .unwrap();
    assert_eq!(again, first);
    assert_eq!(
        count(
            &path,
            "SELECT CAST(COUNT(*) AS TEXT) FROM standing_grant_events"
        ),
        "1"
    );
    let error = store
        .create_standing_grant("grant-1", &terms(6), now())
        .unwrap_err();
    assert_eq!(refusal(&error), Some(GrantError::IdInUse));

    type Break = fn(&mut GrantTerms);
    let cases: [(Break, GrantRefusal); 4] = [
        (|t| t.expires_in_secs = 59, GrantRefusal::ExpiryOutOfRange),
        (|t| t.max_uses = 201, GrantRefusal::UsesOutOfRange),
        (|t| t.token_budget = 0, GrantRefusal::BudgetOutOfRange),
        (|t| t.kinds.clear(), GrantRefusal::NoKinds),
    ];
    for (change, expected) in cases {
        let mut asked = terms(5);
        change(&mut asked);
        let error = store
            .create_standing_grant("grant-bad", &asked, now())
            .unwrap_err();
        assert_eq!(refusal(&error), Some(GrantError::Invalid(expected)));
    }
    assert_eq!(
        count(&path, "SELECT CAST(COUNT(*) AS TEXT) FROM standing_grants"),
        "1"
    );
}

#[test]
fn a_use_refuses_a_spent_budget_a_repeated_proposal_and_an_unknown_grant() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = store_at(directory.path());
    store
        .create_standing_grant("grant-1", &terms(5), now())
        .unwrap();

    assert_eq!(
        consume(&store, "missing", "proposal-1", now()),
        Err(Some(GrantError::NotFound))
    );
    consume(&store, "grant-1", "proposal-1", now() + 1).unwrap();
    assert_eq!(
        consume(&store, "grant-1", "proposal-1", now() + 2),
        Err(Some(GrantError::AlreadyUsed))
    );
    // Bad digests never reach the database.
    let error = store
        .consume_grant_use("grant-1", "proposal-2", "NOT-HEX", now())
        .unwrap_err();
    assert_eq!(refusal(&error), Some(GrantError::BadInput));

    // The task the grant started reports 1000 tokens: the budget is spent for new uses.
    store
        .record_grant_outcome(
            "grant-1",
            "proposal-1",
            DIGEST,
            GrantOutcome::Executed {
                task_id: Some("task-1"),
            },
            now() + 3,
        )
        .unwrap();
    add_task_with_session(&path, "task-1");
    add_receipt(
        &path,
        "task-1",
        "provider_reported",
        "r1",
        now() + 4,
        [Some(400), Some(500), Some(100)],
    );
    assert_eq!(store.grant_tokens_spent("grant-1").unwrap(), 1000);
    assert_eq!(
        consume(&store, "grant-1", "proposal-2", now() + 5),
        Err(Some(GrantError::BudgetExhausted))
    );
    assert_eq!(
        scalars(&path, "SELECT CAST(uses_used AS TEXT) FROM standing_grants"),
        vec!["1"]
    );
}

#[test]
fn the_token_total_counts_reported_figures_of_started_tasks_since_creation() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = store_at(directory.path());
    store
        .create_standing_grant("grant-1", &terms(5), now())
        .unwrap();
    for (proposal, task) in [("proposal-1", "task-1"), ("proposal-2", "task-2")] {
        consume(&store, "grant-1", proposal, now() + 1).unwrap();
        store
            .record_grant_outcome(
                "grant-1",
                proposal,
                DIGEST,
                GrantOutcome::Executed {
                    task_id: Some(task),
                },
                now() + 2,
            )
            .unwrap();
        add_task_with_session(&path, task);
    }
    add_task_with_session(&path, "task-other");

    // Counted: reported figures of the two started tasks, NULL as zero.
    add_receipt(
        &path,
        "task-1",
        "provider_reported",
        "r1",
        now() + 3,
        [Some(10), None, None],
    );
    add_receipt(
        &path,
        "task-2",
        "provider_reported",
        "r2",
        now() + 3,
        [None, Some(20), Some(5)],
    );
    // Not counted: before the grant, the estimated twin, a task the grant did not start.
    add_receipt(
        &path,
        "task-1",
        "provider_reported",
        "r0",
        now() - 1,
        [Some(9999), None, None],
    );
    add_receipt(
        &path,
        "task-1",
        "estimated",
        "e1",
        now() + 3,
        [Some(9999), None, None],
    );
    add_receipt(
        &path,
        "task-other",
        "provider_reported",
        "ro",
        now() + 3,
        [Some(9999), None, None],
    );

    assert_eq!(store.grant_tokens_spent("grant-1").unwrap(), 35);
    assert_eq!(store.grant_tokens_spent("missing").unwrap(), 0);
}

#[test]
fn an_outcome_is_recorded_once_and_only_for_a_recorded_use() {
    let directory = tempfile::tempdir().unwrap();
    let (store, _path) = store_at(directory.path());
    store
        .create_standing_grant("grant-1", &terms(5), now())
        .unwrap();

    let early = store
        .record_grant_outcome(
            "grant-1",
            "proposal-1",
            DIGEST,
            GrantOutcome::Executed { task_id: None },
            now(),
        )
        .unwrap_err();
    assert_eq!(refusal(&early), Some(GrantError::NotUsed));

    consume(&store, "grant-1", "proposal-1", now() + 1).unwrap();
    let wrong_digest = store
        .record_grant_outcome(
            "grant-1",
            "proposal-1",
            OTHER_DIGEST,
            GrantOutcome::Stopped {
                code: "grant_revoked",
            },
            now() + 2,
        )
        .unwrap_err();
    assert_eq!(refusal(&wrong_digest), Some(GrantError::NotUsed));

    for _ in 0..2 {
        store
            .record_grant_outcome(
                "grant-1",
                "proposal-1",
                DIGEST,
                GrantOutcome::Stopped {
                    code: "grant_revoked",
                },
                now() + 3,
            )
            .unwrap();
    }
    let events = store.grant_events("grant-1", 0, 10).unwrap();
    assert_eq!(
        events.iter().map(|event| event.event).collect::<Vec<_>>(),
        vec![
            GrantEventKind::Created,
            GrantEventKind::Used,
            GrantEventKind::Stopped
        ]
    );
    assert_eq!(events[2].code.as_deref(), Some("grant_revoked"));
}

#[test]
fn the_grant_list_shows_open_grants_and_all_grants_newest_first() {
    let directory = tempfile::tempdir().unwrap();
    let (store, _path) = store_at(directory.path());
    store
        .create_standing_grant("grant-old", &terms(5), now())
        .unwrap();
    store
        .create_standing_grant("grant-new", &terms(1), now() + 10)
        .unwrap();
    store
        .create_standing_grant("grant-revoked", &terms(5), now() + 20)
        .unwrap();
    store
        .revoke_standing_grant("grant-revoked", now() + 21)
        .unwrap();
    consume(&store, "grant-new", "proposal-1", now() + 22).unwrap();

    let ids = |grants: Vec<orchestrator_core::standing_grant::StandingGrant>| {
        grants.into_iter().map(|grant| grant.id).collect::<Vec<_>>()
    };
    assert_eq!(
        ids(store.standing_grants(false, now() + 30, 10).unwrap()),
        vec!["grant-old"]
    );
    assert_eq!(
        ids(store.standing_grants(true, now() + 30, 10).unwrap()),
        vec!["grant-revoked", "grant-new", "grant-old"]
    );
    assert!(store
        .standing_grants(false, now() + HOUR_MS * 2, 10)
        .unwrap()
        .is_empty());
    assert_eq!(store.standing_grants(true, now(), 1).unwrap().len(), 1);
    assert!(store.standing_grant("missing").unwrap().is_none());
}
