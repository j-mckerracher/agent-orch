//! Migration 0042 and the approve-time denial (DPES-1105 phase 4).
//!
//! The kinds are tested as raw strings through SQL: core learns the new ones later. Denial
//! tests use `task.propose` rows, which the store can already decode.

use std::fs;
use std::path::Path;

use orchestrator_core::task_envelope::ACTION_DENIED;
use orchestrator_store::{
    ConversationTarget, ProposalState, ResolveError, SqliteStore, DEPENDENCY_NOT_EXECUTED,
};
use sqlx::migrate::Migrator;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::Connection;

const PROPOSAL_DUMP: &str = "SELECT json_array(rowid, id, scope, turn_id, idx, kind, params_json, \
    digest, subject, depends_on, state, result_ref, error_code, denial, created_at, updated_at) \
    FROM action_proposals ORDER BY rowid";
const EVENT_DUMP: &str = "SELECT json_array(sequence, proposal_id, scope, from_state, to_state, \
    actor, digest, error_code, result_ref, created_at) \
    FROM action_proposal_events ORDER BY sequence";
const TRIGGERS: &str = "SELECT name FROM sqlite_master WHERE type = 'trigger' \
    AND tbl_name IN ('action_proposals', 'action_proposal_events') ORDER BY name";
const UNCHANGED_TRIGGER_SQL: &str = "SELECT sql FROM sqlite_master WHERE type = 'trigger' \
    AND name IN ('action_proposals_gated_delete', 'action_proposals_state_transitions') \
    ORDER BY name";

const KINDS: [&str; 14] = [
    "workstream.create",
    "workstream.update",
    "objective.add",
    "objective.revise",
    "task.propose",
    "task.cancel",
    "task.retry",
    "workstream.link_task",
    "task.revise",
    "delivery.request",
    "delivery.merge",
    "project.register",
    "project.remove",
    "decision.hold",
];

/// One row for turn t1 of any kind. Binds: id, index, kind, digest, state, error code and
/// denial (empty for none), depends_on (empty for none).
const RAW_ROW: &str = "INSERT INTO action_proposals \
    (id, scope, turn_id, idx, kind, params_json, digest, subject, state, error_code, denial, \
     depends_on) \
    VALUES (?, 'conversation:global', 't1', CAST(? AS INTEGER), ?, '{}', ?, 'x', ?, \
            NULLIF(?, ''), NULLIF(?, ''), NULLIF(?, ''))";

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn try_sql(path: &Path, sql: &'static str, binds: &[&str]) -> Result<(), sqlx::Error> {
    runtime().block_on(async {
        let options = SqliteConnectOptions::new().filename(path).foreign_keys(true);
        let mut connection = sqlx::SqliteConnection::connect_with(&options).await?;
        let mut query = sqlx::query(sql);
        for value in binds {
            query = query.bind(*value);
        }
        query.execute(&mut connection).await?;
        Ok::<(), sqlx::Error>(())
    })
}

fn run_sql(path: &Path, sql: &'static str, binds: &[&str]) {
    try_sql(path, sql, binds).unwrap();
}

fn scalars(path: &Path, sql: &'static str) -> Vec<String> {
    runtime().block_on(async {
        let options = SqliteConnectOptions::new().filename(path).foreign_keys(true);
        let mut connection = sqlx::SqliteConnection::connect_with(&options)
            .await
            .unwrap();
        sqlx::query_scalar::<_, String>(sql)
            .fetch_all(&mut connection)
            .await
            .unwrap()
    })
}

fn digest(index: usize) -> String {
    format!("{:064x}", index + 1)
}

/// A store with turn t1 on the global conversation, so raw rows have a parent.
fn open_with_turn(directory: &Path) -> (SqliteStore, std::path::PathBuf) {
    let path = directory.join("store.sqlite");
    let store = SqliteStore::new(&path).unwrap();
    store
        .begin_conversation_turn(&ConversationTarget::Global, "t1", "hi", "test-runtime", "1.0.0")
        .unwrap();
    (store, path)
}

/// Inserts one `task.propose` row at `index` in the given state.
fn insert_task(
    path: &Path,
    id: &str,
    index: usize,
    state: &str,
    error_code: &str,
    depends_on: &str,
) {
    let (idx, digest) = (index.to_string(), digest(index));
    run_sql(
        path,
        RAW_ROW,
        &[id, &idx, "task.propose", &digest, state, error_code, "", depends_on],
    );
}

/// Builds the database as 0041 left it, one proposal per state plus a denied one, each with
/// an event, at rowids that are not contiguous. Returns nothing: the file is the result.
fn database_at_0041(directory: &Path) {
    let legacy_dir = directory.join("legacy-migrations");
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
        if version <= 41 {
            fs::copy(&entry, legacy_dir.join(entry.file_name().unwrap())).unwrap();
        }
    }
    runtime().block_on(async {
        let options = SqliteConnectOptions::new()
            .filename(directory.join("store.sqlite"))
            .create_if_missing(true)
            .foreign_keys(false);
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
        assert_eq!(newest, 41);
        for sql in [
            "INSERT INTO conversations (id, target_kind) VALUES ('conversation:global', 'global')",
            "INSERT INTO conversation_turns (id, conversation_id, state, runtime_name, runtime_version) \
             VALUES ('t1', 'conversation:global', 'completed', 'test-runtime', '1.0.0')",
        ] {
            sqlx::query(sql).execute(&mut connection).await.unwrap();
        }
        // (state, error_code, denial, result_ref)
        let rows = [
            ("pending", None, None, None),
            ("approved", None, None, None),
            ("executed", None, None, Some("result-1")),
            ("failed", Some("boom"), None, None),
            ("rejected", None, None, None),
            ("superseded", None, None, None),
            ("expired", None, None, None),
            ("failed", Some("action_denied"), Some("delivery_not_authorized"), None),
        ];
        for (index, (state, error_code, denial, result_ref)) in rows.into_iter().enumerate() {
            sqlx::query(
                "INSERT INTO action_proposals \
                 (rowid, id, scope, turn_id, idx, kind, params_json, digest, subject, state, \
                  result_ref, error_code, denial) \
                 VALUES (?, ?, 'conversation:global', 't1', ?, 'task.propose', '{}', ?, 'x', ?, ?, ?, ?)",
            )
            .bind(i64::try_from(index + 1).unwrap() * 7)
            .bind(format!("proposal-{index}"))
            .bind(i64::try_from(index).unwrap())
            .bind(digest(index))
            .bind(state)
            .bind(result_ref)
            .bind(error_code)
            .bind(denial)
            .execute(&mut connection)
            .await
            .unwrap();
        }
        // A creation event for every proposal, and a second event for the denied one.
        sqlx::query(
            "INSERT INTO action_proposal_events (proposal_id, scope, from_state, to_state, actor, digest) \
             SELECT id, scope, NULL, 'pending', 'agent', digest FROM action_proposals ORDER BY rowid",
        )
        .execute(&mut connection)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO action_proposal_events \
             (proposal_id, scope, from_state, to_state, actor, digest, error_code) \
             SELECT id, scope, 'pending', 'failed', 'agent', digest, error_code \
             FROM action_proposals WHERE denial IS NOT NULL",
        )
        .execute(&mut connection)
        .await
        .unwrap();
        connection.close().await.unwrap();
    });
}

#[test]
fn migration_0042_applies_to_a_0041_database_and_keeps_every_proposal_event_and_trigger() {
    let directory = tempfile::tempdir().unwrap();
    database_at_0041(directory.path());
    let path = directory.path().join("store.sqlite");
    let proposals = scalars(&path, PROPOSAL_DUMP);
    let events = scalars(&path, EVENT_DUMP);
    let triggers = scalars(&path, TRIGGERS);
    let trigger_sql = scalars(&path, UNCHANGED_TRIGGER_SQL);
    assert_eq!(proposals.len(), 8);
    assert_eq!(events.len(), 9);

    // Opening the store applies 0042.
    let _store = SqliteStore::new(&path).unwrap();
    assert_eq!(
        scalars(
            &path,
            "SELECT CAST(success AS TEXT) FROM _sqlx_migrations WHERE version = 42"
        ),
        vec!["1"]
    );

    // Every row survived with its rowid, and so did every event.
    assert_eq!(scalars(&path, PROPOSAL_DUMP), proposals);
    assert_eq!(scalars(&path, EVENT_DUMP), events);
    assert!(scalars(&path, "PRAGMA foreign_key_check").is_empty());

    // The triggers are the old ones plus the denial trigger; the two that did not change
    // are byte for byte the same, and the frozen-column trigger no longer names denial.
    let mut expected = triggers;
    expected.push("action_proposals_denial_once".to_owned());
    expected.sort();
    assert_eq!(scalars(&path, TRIGGERS), expected);
    assert_eq!(scalars(&path, UNCHANGED_TRIGGER_SQL), trigger_sql);
    assert_eq!(
        scalars(
            &path,
            "SELECT CAST(instr(sql, 'denial') AS TEXT) FROM sqlite_master \
             WHERE name = 'action_proposals_fixed_parameters'"
        ),
        vec!["0"]
    );
    assert_eq!(
        scalars(
            &path,
            "SELECT name FROM sqlite_master WHERE type = 'index' AND tbl_name = 'action_proposals' \
             AND name NOT LIKE 'sqlite_autoindex%' ORDER BY name"
        ),
        vec![
            "action_proposals_by_scope",
            "action_proposals_grant_untried",
            "action_proposals_one_pending_digest",
            "action_proposals_pending_subject",
        ]
    );

    // The rebuilt rules still hold on migrated data: a terminal row stays terminal, the
    // events stay append-only, and the delete gate is shut.
    assert!(try_sql(
        &path,
        "UPDATE action_proposals SET state = 'failed', error_code = 'x' WHERE id = 'proposal-2'",
        &[]
    )
    .is_err());
    assert!(try_sql(&path, "UPDATE action_proposal_events SET actor = 'user'", &[]).is_err());
    assert!(try_sql(&path, "DELETE FROM action_proposals", &[]).is_err());
}

#[test]
fn the_kind_check_lists_exactly_the_fourteen_design_kinds() {
    let directory = tempfile::tempdir().unwrap();
    let (_store, path) = open_with_turn(directory.path());
    run_sql(
        &path,
        "INSERT INTO conversation_turns (id, conversation_id, state, runtime_name, runtime_version) \
         VALUES ('t2', 'conversation:global', 'completed', 'test-runtime', '1.0.0')",
        &[],
    );
    // Seven kinds per turn keeps every index under the cap of ten.
    for (index, kind) in KINDS.into_iter().enumerate() {
        let (id, idx, digest) = (format!("raw-{index}"), (index % 7).to_string(), digest(index));
        let sql = if index < 7 {
            RAW_ROW
        } else {
            "INSERT INTO action_proposals \
             (id, scope, turn_id, idx, kind, params_json, digest, subject, state, error_code, \
              denial, depends_on) \
             VALUES (?, 'conversation:global', 't2', CAST(? AS INTEGER), ?, '{}', ?, 'x', ?, \
                     NULLIF(?, ''), NULLIF(?, ''), NULLIF(?, ''))"
        };
        run_sql(&path, sql, &[&id, &idx, kind, &digest, "pending", "", "", ""]);
    }
    assert_eq!(
        scalars(
            &path,
            "SELECT CAST(COUNT(DISTINCT kind) AS TEXT) FROM action_proposals"
        ),
        vec!["14"]
    );
}

#[test]
fn a_raw_unknown_kind_is_refused_after_0042() {
    let directory = tempfile::tempdir().unwrap();
    let (_store, path) = open_with_turn(directory.path());
    for unknown in [
        "task.delete",
        "task.create",
        "project.add",
        "decision.release",
        "",
        "TASK.REVISE",
        "task.revise ",
    ] {
        let digest = digest(1);
        assert!(
            try_sql(
                &path,
                RAW_ROW,
                &["raw-unknown", "9", unknown, &digest, "pending", "", "", ""]
            )
            .is_err(),
            "{unknown}"
        );
    }
    assert_eq!(
        scalars(&path, "SELECT CAST(COUNT(*) AS TEXT) FROM action_proposals"),
        vec!["0"]
    );
}

#[test]
fn a_pending_proposal_can_be_denied_once_and_never_again() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = open_with_turn(directory.path());
    insert_task(&path, "parent", 0, "pending", "", "");
    insert_task(&path, "child", 1, "pending", "", "parent");

    let denied = store
        .deny_action_proposal("parent", "below_project_default", "user")
        .unwrap();
    assert_eq!(denied.state, ProposalState::Failed);
    assert_eq!(denied.error_code.as_deref(), Some(ACTION_DENIED));
    assert_eq!(denied.denial.as_deref(), Some("below_project_default"));
    assert_eq!(denied.result_ref, None);

    // A pending dependent cannot run without its parent, so it fails with it.
    let child = store.action_proposal("child").unwrap().unwrap();
    assert_eq!(child.state, ProposalState::Failed);
    assert_eq!(child.error_code.as_deref(), Some(DEPENDENCY_NOT_EXECUTED));
    assert_eq!(child.denial, None);

    // A second denial changes nothing, with a different reason too.
    let before = store.action_proposal_events("parent").unwrap().len();
    for reason in ["below_project_default", "other_reason"] {
        assert!(matches!(
            store.deny_action_proposal("parent", reason, "user"),
            Err(ResolveError::NotPending(ProposalState::Failed))
        ));
    }
    let after = store.action_proposal("parent").unwrap().unwrap();
    assert_eq!(after.denial.as_deref(), Some("below_project_default"));
    assert_eq!(store.action_proposal_events("parent").unwrap().len(), before);

    // A missing proposal is not found, and the database refuses a rewrite of the reason.
    assert!(matches!(
        store.deny_action_proposal("missing", "x", "user"),
        Err(ResolveError::NotFound)
    ));
    assert!(try_sql(
        &path,
        "UPDATE action_proposals SET denial = 'other_reason' WHERE id = 'parent'",
        &[]
    )
    .is_err());
}

#[test]
fn a_settled_proposal_cannot_be_denied() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = open_with_turn(directory.path());
    let settled = [
        ("approved", ProposalState::Approved, ""),
        ("executed", ProposalState::Executed, ""),
        ("rejected", ProposalState::Rejected, ""),
        ("superseded", ProposalState::Superseded, ""),
        ("expired", ProposalState::Expired, ""),
        ("failed", ProposalState::Failed, "boom"),
    ];
    for (index, (state, _, error_code)) in settled.iter().enumerate() {
        insert_task(&path, state, index, state, error_code, "");
    }
    for (state, expected, error_code) in settled {
        match store.deny_action_proposal(state, "below_project_default", "user") {
            Err(ResolveError::NotPending(found)) => assert_eq!(found, expected, "{state}"),
            other => panic!("{state}: {other:?}"),
        }
        let row = store.action_proposal(state).unwrap().unwrap();
        assert_eq!(row.state, expected, "{state}");
        assert_eq!(row.denial, None, "{state}");
        assert_eq!(row.error_code.as_deref().unwrap_or(""), error_code, "{state}");
        assert!(store.action_proposal_events(state).unwrap().is_empty(), "{state}");
    }
}

#[test]
fn the_denial_column_is_still_frozen_except_the_one_write() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = open_with_turn(directory.path());
    insert_task(&path, "pending", 0, "pending", "", "");
    insert_task(&path, "approved", 1, "approved", "", "");

    // A reason alone fails the table CHECK: it needs the code and the failed state.
    assert!(try_sql(
        &path,
        "UPDATE action_proposals SET denial = 'a' WHERE id = 'pending'",
        &[]
    )
    .is_err());
    // An approved row is past the one write, though the statement passes every CHECK.
    assert!(try_sql(
        &path,
        "UPDATE action_proposals SET state = 'failed', error_code = 'action_denied', denial = 'a' \
         WHERE id = 'approved'",
        &[]
    )
    .is_err());
    assert_eq!(
        store.action_proposal("approved").unwrap().unwrap().state,
        ProposalState::Approved
    );

    // The one write: a pending row, with the state and the code in the same statement.
    run_sql(
        &path,
        "UPDATE action_proposals SET state = 'failed', error_code = 'action_denied', denial = 'a' \
         WHERE id = 'pending'",
        &[],
    );
    assert_eq!(
        store.action_proposal("pending").unwrap().unwrap().denial.as_deref(),
        Some("a")
    );

    // After it the reason never changes, not even to the same value.
    for reason in ["a", "b"] {
        assert!(try_sql(
            &path,
            "UPDATE action_proposals SET denial = ? WHERE id = 'pending'",
            &[reason]
        )
        .is_err());
    }

    // The other parameters stay frozen on every row.
    for sql in [
        "UPDATE action_proposals SET params_json = '{\"a\":1}' WHERE id = ?",
        "UPDATE action_proposals SET kind = 'task.revise' WHERE id = ?",
        "UPDATE action_proposals SET subject = 'y' WHERE id = ?",
        "UPDATE action_proposals SET depends_on = 'z' WHERE id = ?",
        "UPDATE action_proposals SET created_at = '2000-01-01 00:00:00' WHERE id = ?",
        "DELETE FROM action_proposals WHERE id = ?",
    ] {
        for id in ["pending", "approved"] {
            assert!(try_sql(&path, sql, &[id]).is_err(), "{sql}");
        }
    }
}

#[test]
fn deny_action_proposal_records_one_event_with_the_actor() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = open_with_turn(directory.path());
    insert_task(&path, "p1", 0, "pending", "", "");

    let denied = store
        .deny_action_proposal("p1", "below_project_default", "grant:g-1")
        .unwrap();
    let events = store.action_proposal_events("p1").unwrap();
    assert_eq!(events.len(), 1);
    let event = &events[0];
    assert_eq!(event.proposal_id, "p1");
    assert_eq!(event.scope, denied.scope);
    assert_eq!(event.from_state, Some(ProposalState::Pending));
    assert_eq!(event.to_state, ProposalState::Failed);
    assert_eq!(event.actor, "grant:g-1");
    assert_eq!(event.digest, denied.digest);
    assert_eq!(event.error_code.as_deref(), Some(ACTION_DENIED));
    assert_eq!(event.result_ref, None);
}
